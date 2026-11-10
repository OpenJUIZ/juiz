//! `juiz pkg add process|container <name>` (docs/spec/pkg.md §2.4)
//!
//! `juiz pkg create` で作った雛形の Component に、型を 1 つ (Container なら ContainerProcess 2 つと一緒に) 足す。
//! 新しいファイルは作らず、Component のソース 1 つと app/app.conf の 2 つを書き換える
//! (1 雛形 = 1 Component、§4)。足すコードは templates/add/<lang>/<kind>.tmpl。

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use yaml_rust2::{Yaml, YamlLoader};

use crate::cli::AddArgs;
use crate::render::{render, Vars};
use crate::{names, templates, Lang, PkgError};

/// 足す型の種類
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum AddKind {
    /// A Process (an idempotent function): <name> (a: int, b: int) -> int
    Process,
    /// A Container <name> with the ContainerProcesses <name>_increment and <name>_get
    Container,
}

impl AddKind {
    fn dir(self) -> &'static str {
        match self {
            AddKind::Process => "process",
            AddKind::Container => "container",
        }
    }
}

/// 書き換えるファイル 1 つ (雛形からの相対パスと、書き換えた後の中身)
#[derive(Debug, Clone)]
pub struct EditedFile {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// `juiz pkg add` の結果
#[derive(Debug, Clone)]
pub struct Added {
    pub files: Vec<EditedFile>,
    pub summary: String,
}

/// app/app.conf から読んだ雛形の情報
#[derive(Debug, Clone)]
pub struct Package {
    /// Component の名前 (plugins.components のキー)
    pub component: String,
    pub lang: Lang,
    /// conf にある型名とインスタンス名
    pub names_in_conf: Vec<String>,
}

fn err(msg: impl Into<String>) -> PkgError {
    PkgError::new(msg)
}

fn read(dir: &Path, rel: &str) -> Result<String, PkgError> {
    let p = dir.join(rel);
    std::fs::read_to_string(&p)
        .map(|s| s.replace("\r\n", "\n"))
        .map_err(|e| err(format!("cannot read {}: {e}", p.display())))
}

fn lang_from_conf(language: &str) -> Option<Lang> {
    match language.to_ascii_lowercase().as_str() {
        "rust" => Some(Lang::Rust),
        "python" => Some(Lang::Python),
        "c++" | "cpp" => Some(Lang::Cpp),
        "haskell" => Some(Lang::Haskell),
        _ => None,
    }
}

impl Package {
    /// app/app.conf を読む。plugins.components に Component が 1 つだけあること
    pub fn from_conf(conf: &str) -> Result<Package, PkgError> {
        let docs = YamlLoader::load_from_str(conf)
            .map_err(|e| err(format!("app/app.conf is not valid YAML: {e}")))?;
        let doc = docs.first().cloned().unwrap_or(Yaml::Null);
        let components = match &doc["plugins"]["components"] {
            Yaml::Hash(h) => h.clone(),
            _ => {
                return Err(err(
                    "app/app.conf has no plugins.components: juiz pkg add works on a skeleton made by juiz pkg create",
                ))
            }
        };
        if components.len() != 1 {
            return Err(err(format!(
                "app/app.conf has {} components in plugins.components; juiz pkg add needs exactly one (a skeleton made by juiz pkg create)",
                components.len()
            )));
        }
        let (k, v) = components.iter().next().unwrap();
        let component = k
            .as_str()
            .ok_or_else(|| err("the key of plugins.components in app/app.conf is not a string"))?
            .to_owned();
        let language = v["language"].as_str().unwrap_or("");
        let lang = lang_from_conf(language).ok_or_else(|| {
            err(format!(
                "plugins.components.{component}.language in app/app.conf is '{language}'; expected rust, python, c++ or haskell"
            ))
        })?;

        let mut names_in_conf = Vec::new();
        let mut push = |y: &Yaml| {
            for key in ["type_name", "name"] {
                if let Some(s) = y[key].as_str() {
                    names_in_conf.push(s.to_owned());
                }
            }
        };
        if let Yaml::Array(ps) = &doc["processes"] {
            ps.iter().for_each(&mut push);
        }
        if let Yaml::Array(cs) = &doc["containers"] {
            for c in cs {
                push(c);
                if let Yaml::Array(ps) = &c["processes"] {
                    ps.iter().for_each(&mut push);
                }
            }
        }
        Ok(Package {
            component,
            lang,
            names_in_conf,
        })
    }

    /// Component のソースのファイル (§6)
    pub fn source_path(&self) -> String {
        match self.lang {
            Lang::Rust => "src/lib.rs".to_owned(),
            Lang::Python => format!("app/python/{}.py", self.component),
            Lang::Cpp => format!("src/{}.cpp", self.component),
            Lang::Haskell => format!("src/{}.hs", names::camel(&self.component)),
        }
    }
}

/// 足す型名 (conf に書く順)
pub fn type_names(kind: AddKind, name: &str) -> Vec<String> {
    match kind {
        AddKind::Process => vec![name.to_owned()],
        AddKind::Container => vec![
            name.to_owned(),
            format!("{name}_increment"),
            format!("{name}_get"),
        ],
    }
}

/// ソースに足す識別子。どれかが既にソースにあればエラーにする (マクロが作る名前も含める)
fn new_identifiers(kind: AddKind, name: &str, lang: Lang) -> Vec<String> {
    let mut v = Vec::new();
    for t in type_names(kind, name) {
        for suffix in [
            "",
            "_manifest",
            "_manif",
            "_entry_point",
            "_manifest_entry_point",
            "_create",
        ] {
            v.push(format!("{t}{suffix}"));
        }
    }
    if kind == AddKind::Container {
        let camel = names::camel(name);
        if lang == Lang::Haskell {
            v.push(format!("{camel}State"));
        } else {
            v.push(camel);
        }
    }
    v
}

/// 識別子として (前後が英数字・_ でない所に) 出てくるか
fn contains_word(text: &str, word: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut start = 0;
    while let Some(i) = text[start..].find(word) {
        let at = start + i;
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            return true;
        }
        start = at + word.len();
    }
    false
}

/// 足すコード (templates/add/<lang>/<kind>.tmpl)
fn snippet(kind: AddKind, name: &str, lang: Lang) -> Result<String, PkgError> {
    let key = format!("add/{}/{}.tmpl", lang.dir(), kind.dir());
    let (_, text) = templates()
        .iter()
        .find(|(p, _)| *p == key)
        .ok_or_else(|| err(format!("template {key} is missing")))?;
    let mut vars = Vars::new();
    vars.set("name", name).set("Name", names::camel(name));
    render(&text.replace("\r\n", "\n"), &vars)
}

fn not_a_skeleton(path: &str, what: &str) -> PkgError {
    err(format!(
        "{path}: {what} was not found. juiz pkg add edits the shape that juiz pkg create writes; add the type by hand (see the existing types in {path})"
    ))
}

fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

fn join(lines: &[String]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// `at` の前に、空行で区切ってコードを入れる
fn insert_block(lines: &mut Vec<String>, at: usize, code: &str) {
    let mut block: Vec<String> = lines_of(code.trim_end_matches('\n'));
    block.push(String::new());
    // 前が空行でなければ空行を足す
    if at > 0 && !lines[at - 1].trim().is_empty() {
        block.insert(0, String::new());
    }
    lines.splice(at..at, block);
}

/// コメント行 (prefix で始まる行) が直前に続いていれば、その先頭まで戻る
fn back_over_comments(lines: &[String], mut at: usize, prefix: &str) -> usize {
    while at > 0 && lines[at - 1].trim_start().starts_with(prefix) {
        at -= 1;
    }
    at
}

// ---- Rust ----

fn edit_rust(
    src: &str,
    path: &str,
    kind: AddKind,
    name: &str,
    code: &str,
) -> Result<String, PkgError> {
    let mut lines = lines_of(src);
    let start = lines
        .iter()
        .position(|l| l.starts_with("juiz_component_manifest!("))
        .ok_or_else(|| not_a_skeleton(path, "juiz_component_manifest!( at the start of a line"))?;
    let end = (start..lines.len())
        .find(|&i| lines[i].trim() == ");")
        .ok_or_else(|| not_a_skeleton(path, "the line ); that closes juiz_component_manifest!"))?;
    match kind {
        AddKind::Process => {
            match (start..end).find(|&i| lines[i].trim_start().starts_with("processes")) {
                Some(i) => {
                    let line = lines[i].trim_end();
                    let (head, inner) = line
                        .strip_suffix(']')
                        .and_then(|l| l.split_once('['))
                        .ok_or_else(|| not_a_skeleton(path, "processes = [ ... ] on one line"))?;
                    let items: Vec<&str> = inner
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    let mut items: Vec<String> = items.into_iter().map(str::to_owned).collect();
                    items.push(name.to_owned());
                    lines[i] = format!("{head}[ {} ]", items.join(", "));
                }
                None => lines.insert(end, format!("    processes = [ {name} ]")),
            }
        }
        AddKind::Container => {
            let entry = format!("        {name} = [ {name}_increment, {name}_get ]");
            match (start..end).find(|&i| lines[i].trim_start().starts_with("containers")) {
                Some(i) => {
                    if !lines[i].trim_end().ends_with('{') {
                        return Err(not_a_skeleton(path, "containers = { at the end of a line"));
                    }
                    let close =
                        (i + 1..end)
                            .find(|&j| lines[j].trim() == "}")
                            .ok_or_else(|| {
                                not_a_skeleton(path, "the line } that closes containers = {")
                            })?;
                    lines.insert(close, entry);
                }
                None => {
                    let after = (start..end)
                        .find(|&i| lines[i].trim_start().starts_with("component_name"))
                        .map(|i| i + 1)
                        .unwrap_or(start + 1);
                    lines.splice(
                        after..after,
                        ["    containers = {".to_owned(), entry, "    }".to_owned()],
                    );
                }
            }
        }
    }
    insert_block(&mut lines, start, code);
    Ok(join(&lines))
}

// ---- Python ----

fn edit_python(src: &str, kind: AddKind, code: &str) -> Result<String, PkgError> {
    let needed: &[&str] = match kind {
        AddKind::Process => &["juiz_process"],
        AddKind::Container => &["juiz_container", "juiz_container_process"],
    };
    let mut lines = lines_of(src);
    let mut code = code.to_owned();
    match lines
        .iter()
        .position(|l| l.starts_with("from juiz import ") && !l.contains('(') && !l.contains('\\'))
    {
        Some(i) => {
            let mut names: Vec<String> = lines[i]["from juiz import ".len()..]
                .split('#')
                .next()
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            for n in needed {
                if !names.iter().any(|x| x == n) {
                    names.push((*n).to_owned());
                }
            }
            names.sort();
            lines[i] = format!("from juiz import {}", names.join(", "));
        }
        None => {
            code = format!("from juiz import {}\n\n\n{code}", needed.join(", "));
        }
    }
    // 末尾の空行を落とし、2 行空けて足す (PEP 8)
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.push(String::new());
    lines.push(String::new());
    lines.extend(lines_of(code.trim_end_matches('\n')));
    Ok(join(&lines))
}

// ---- C++ ----

fn edit_cpp(
    src: &str,
    path: &str,
    kind: AddKind,
    name: &str,
    code: &str,
) -> Result<String, PkgError> {
    let mut lines = lines_of(src);
    let ret = lines
        .iter()
        .position(|l| l.trim_start().starts_with("return ComponentManifest("))
        .ok_or_else(|| {
            not_a_skeleton(
                path,
                "return ComponentManifest(...) of the component manifest function",
            )
        })?;
    let head = (0..ret)
        .rev()
        .find(|&i| lines[i].starts_with("auto ") && lines[i].contains("()"))
        .ok_or_else(|| not_a_skeleton(path, "auto <name>_component_manifest() {"))?;
    let last = (ret..lines.len())
        .find(|&i| lines[i].trim_end().ends_with(';'))
        .ok_or_else(|| not_a_skeleton(path, "the ; that ends return ComponentManifest(...)"))?;
    let body = lines[last].trim_end().strip_suffix(';').unwrap().to_owned();
    lines[last] = body;
    let added: Vec<String> = match kind {
        AddKind::Process => vec![format!("        .add_process({name}_manifest());")],
        AddKind::Container => vec![
            format!("        .add_container({name}_manifest()"),
            format!("            .add_process({name}_increment_manifest())"),
            format!("            .add_process({name}_get_manifest()));"),
        ],
    };
    lines.splice(last + 1..last + 1, added);
    let at = back_over_comments(&lines, head, "//");
    insert_block(&mut lines, at, code);
    Ok(join(&lines))
}

// ---- Haskell ----

fn edit_haskell(
    src: &str,
    path: &str,
    kind: AddKind,
    name: &str,
    code: &str,
) -> Result<String, PkgError> {
    let mut lines = lines_of(src);
    let start = lines
        .iter()
        .position(|l| l.starts_with("juizComponent "))
        .ok_or_else(|| not_a_skeleton(path, "juizComponent at the start of a line"))?;
    let close = (start + 1..lines.len())
        .find(|&i| lines[i].trim() == "]")
        .ok_or_else(|| not_a_skeleton(path, "the line ] that closes the list of juizComponent"))?;
    if !(start + 1..close).any(|i| lines[i].trim_start().starts_with('[')) {
        return Err(not_a_skeleton(
            path,
            "the list [ ... ] of juizComponent, one entry per line",
        ));
    }
    let added: Vec<String> = match kind {
        AddKind::Process => vec![format!("  , componentProcess '{name} '{name}_manifest")],
        AddKind::Container => vec![
            format!("  , componentContainer '{name}_create '{name}_manifest"),
            format!("  , componentContainerProcess '{name}_increment '{name}_increment_manifest"),
            format!("  , componentContainerProcess '{name}_get '{name}_get_manifest"),
        ],
    };
    lines.splice(close..close, added);
    insert_block(&mut lines, start, code);
    Ok(join(&lines))
}

// ---- app/app.conf ----

/// 最上位のキーの行か (`"processes":` / `processes:`)。値が同じ行にあればそれも返す
fn top_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let rest = line
        .strip_prefix(&format!("\"{key}\""))
        .or_else(|| line.strip_prefix(&format!("'{key}'")))
        .or_else(|| line.strip_prefix(key))?;
    let rest = rest.trim_start().strip_prefix(':')?;
    Some(rest.split('#').next().unwrap_or("").trim())
}

/// 最上位のブロック (key の行の次から、次の最上位の行の前まで) の終わり。末尾の空行は含めない
fn block_end(lines: &[String], key_line: usize) -> usize {
    let next = (key_line + 1..lines.len())
        .find(|&i| {
            let l = &lines[i];
            !l.trim().is_empty() && !l.starts_with(' ') && !l.starts_with('\t')
        })
        .unwrap_or(lines.len());
    let mut end = next;
    while end > key_line + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    // 次のキーの直前の最上位のコメント (# で始まる行) は next に含まれるので、次のキーのものになる
    end
}

fn conf_entries(kind: AddKind, name: &str, indent: &str) -> Vec<String> {
    let i = indent;
    match kind {
        AddKind::Process => vec![
            format!("{i}- \"type_name\": \"{name}\""),
            format!("{i}  \"name\": \"{name}0\""),
        ],
        AddKind::Container => vec![
            format!("{i}- \"type_name\": \"{name}\""),
            format!("{i}  \"name\": \"{name}0\""),
            format!("{i}  \"initial_value\": 0"),
            format!("{i}  \"processes\":"),
            format!("{i}    - \"type_name\": \"{name}_increment\""),
            format!("{i}      \"name\": \"{name}_increment0\""),
            format!("{i}    - \"type_name\": \"{name}_get\""),
            format!("{i}      \"name\": \"{name}_get0\""),
        ],
    }
}

fn edit_conf(conf: &str, kind: AddKind, name: &str) -> Result<String, PkgError> {
    let key = match kind {
        AddKind::Process => "processes",
        AddKind::Container => "containers",
    };
    // §5.1 の順: processes の後ろは containers・ecs、containers の後ろは ecs
    let later: &[&str] = match kind {
        AddKind::Process => &["containers", "ecs"],
        AddKind::Container => &["ecs"],
    };
    let mut lines = lines_of(conf);
    match lines.iter().position(|l| top_key(l, key).is_some()) {
        Some(i) => {
            let value = top_key(&lines[i], key).unwrap().to_owned();
            if value == "[]" {
                lines[i] = lines[i].replacen("[]", "", 1).trim_end().to_owned();
            } else if !value.is_empty() {
                return Err(not_a_skeleton(
                    "app/app.conf",
                    &format!("a block list under {key}: (one \"- type_name: ...\" per entry)"),
                ));
            }
            let end = block_end(&lines, i);
            let indent = (i + 1..end)
                .find_map(|j| {
                    let l = &lines[j];
                    let t = l.trim_start();
                    t.starts_with("- ")
                        .then(|| l[..l.len() - t.len()].to_owned())
                })
                .unwrap_or_else(|| "  ".to_owned());
            lines.splice(end..end, conf_entries(kind, name, &indent));
        }
        None => {
            let at = lines
                .iter()
                .position(|l| later.iter().any(|k| top_key(l, k).is_some()))
                .map(|i| back_over_comments(&lines, i, "#"))
                .unwrap_or_else(|| {
                    let mut n = lines.len();
                    while n > 0 && lines[n - 1].trim().is_empty() {
                        n -= 1;
                    }
                    n
                });
            let mut block = vec![format!("\"{key}\":")];
            block.extend(conf_entries(kind, name, "  "));
            lines.splice(at..at, block);
        }
    }
    Ok(join(&lines))
}

// ---- まとめ ----

/// 書き換えた後の中身を作る (ファイルはまだ書かない)
pub fn plan(dir: &Path, kind: AddKind, name: &str) -> Result<(Package, Vec<EditedFile>), PkgError> {
    let conf = read(dir, "app/app.conf").map_err(|e| {
        err(format!(
            "{e}\njuiz pkg add runs in a skeleton made by juiz pkg create (give its directory with -C <dir>)"
        ))
    })?;
    let pkg = Package::from_conf(&conf)?;
    names::validate(name, pkg.lang)?;

    let src_path = pkg.source_path();
    let src = read(dir, &src_path)?;

    for t in type_names(kind, name) {
        if t.len() > names::MAX_NAME_LEN {
            return Err(err(format!(
                "{t}: a type name is at most {} characters",
                names::MAX_NAME_LEN
            )));
        }
        if pkg
            .names_in_conf
            .iter()
            .any(|n| *n == t || *n == format!("{t}0"))
        {
            return Err(err(format!(
                "{t} is already in app/app.conf; choose another name"
            )));
        }
    }
    for ident in new_identifiers(kind, name, pkg.lang) {
        if contains_word(&src, &ident) {
            return Err(err(format!(
                "{ident} is already used in {src_path}; choose another name"
            )));
        }
    }

    let code = snippet(kind, name, pkg.lang)?;
    let new_src = match pkg.lang {
        Lang::Rust => edit_rust(&src, &src_path, kind, name, &code)?,
        Lang::Python => edit_python(&src, kind, &code)?,
        Lang::Cpp => edit_cpp(&src, &src_path, kind, name, &code)?,
        Lang::Haskell => edit_haskell(&src, &src_path, kind, name, &code)?,
    };
    let new_conf = edit_conf(&conf, kind, name)?;

    // 書き換えた conf が読めて、足した型が入っていることを確かめる
    let check = Package::from_conf(&new_conf)?;
    for t in type_names(kind, name) {
        if !check.names_in_conf.contains(&t) {
            return Err(not_a_skeleton(
                "app/app.conf",
                &format!("a place to add {t}"),
            ));
        }
    }

    Ok((
        pkg,
        vec![
            EditedFile {
                path: src_path,
                before: src,
                after: new_src,
            },
            EditedFile {
                path: "app/app.conf".to_owned(),
                before: conf,
                after: new_conf,
            },
        ],
    ))
}

/// 書く。途中で失敗したら、書いたものを元に戻す
pub(crate) fn write_edits(
    dir: &Path,
    files: &[EditedFile],
    fail_at: Option<usize>,
) -> Result<(), PkgError> {
    let mut written: Vec<&EditedFile> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let r = if fail_at == Some(i) {
            Err(std::io::Error::other("injected failure"))
        } else {
            std::fs::write(dir.join(&f.path), f.after.as_bytes())
        };
        if let Err(e) = r {
            for w in written {
                let _ = std::fs::write(dir.join(&w.path), w.before.as_bytes());
            }
            return Err(err(format!(
                "failed to write {}: {e} (nothing was changed)",
                f.path
            )));
        }
        written.push(f);
    }
    Ok(())
}

fn call_examples(kind: AddKind, name: &str) -> Vec<String> {
    match kind {
        AddKind::Process => vec![format!(
            "juiz process call \"http://127.0.0.1:8000/process/{name}0::{name}\" '{{\"a\": 1, \"b\": 2}}' -p"
        )],
        AddKind::Container => vec![
            format!(
                "juiz container-process call \"http://127.0.0.1:8000/container_process/{name}_increment0::{name}_increment:{name}0\" '{{\"step\": 1}}' -p"
            ),
            format!(
                "juiz container-process call \"http://127.0.0.1:8000/container_process/{name}_get0::{name}_get:{name}0\" '{{}}' -p"
            ),
        ],
    }
}

/// 成功したときの表示
pub fn summary(
    pkg: &Package,
    kind: AddKind,
    name: &str,
    files: &[EditedFile],
    windows: bool,
) -> String {
    let types = type_names(kind, name).join(", ");
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut s = format!(
        "Added {} {name} to {} ({}): {types}\nEdited {}\n\nNext steps:\n",
        kind.dir(),
        pkg.component,
        pkg.lang.plugin_language(),
        paths.join(", ")
    );
    if pkg.lang.has_build() {
        let dev = if windows && pkg.lang.has_ps1() {
            "scripts\\dev.ps1"
        } else {
            "scripts/dev.sh"
        };
        s.push_str(&format!("  {dev:<30}  # rebuild the plugin\n"));
    }
    s.push_str(&format!(
        "  {:<30}  # restart juiz\n",
        "juiz -f app/app.conf -d"
    ));
    s.push_str("\n  # in another terminal\n");
    for c in call_examples(kind, name) {
        s.push_str(&format!("  {c}\n"));
    }
    s.push_str("\n  (README.md is not updated: add the new types to its table if you keep it)\n");
    s
}

/// 型を足す
pub fn add(args: &AddArgs) -> Result<Added, PkgError> {
    let dir: PathBuf = args.dir.clone();
    let (pkg, files) = plan(&dir, args.kind, &args.name)?;
    write_edits(&dir, &files, None)?;
    let summary = summary(&pkg, args.kind, &args.name, &files, cfg!(windows));
    Ok(Added { files, summary })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::CreateArgs;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: crate::cli::PkgArgs,
    }

    fn parse(args: &[&str]) -> crate::cli::PkgCommand {
        let mut v = vec!["juiz-pkg"];
        v.extend_from_slice(args);
        Cli::try_parse_from(v).unwrap().args.command
    }

    fn create_args(args: &[&str]) -> CreateArgs {
        match parse(args) {
            crate::cli::PkgCommand::Create(c) => c,
            _ => panic!(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_pkg_add_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// 雛形を作る (ファイルの中身だけ。ツールは要らない)
    fn skeleton(lang: &str, template: &str, tag: &str) -> PathBuf {
        let d = temp(&format!("{lang}_{template}_{tag}"));
        let o = d.to_string_lossy().into_owned();
        let args = create_args(&["create", "my_robot", "-b", lang, "-t", template, "-o", &o]);
        let (plan, _) = crate::create::Plan::from_args_with(&args, || None, false).unwrap();
        crate::create::write_files(&d, &plan.files().unwrap(), None).unwrap();
        d
    }

    fn conf_types(dir: &Path) -> Vec<String> {
        Package::from_conf(&std::fs::read_to_string(dir.join("app/app.conf")).unwrap())
            .unwrap()
            .names_in_conf
    }

    #[test]
    fn cli() {
        match parse(&["add", "process", "my_mul", "-C", "x"]) {
            crate::cli::PkgCommand::Add(a) => {
                assert_eq!(a.kind, AddKind::Process);
                assert_eq!(a.name, "my_mul");
                assert_eq!(a.dir, PathBuf::from("x"));
            }
            _ => panic!(),
        }
        match parse(&["add", "container", "arm"]) {
            crate::cli::PkgCommand::Add(a) => {
                assert_eq!(a.kind, AddKind::Container);
                assert_eq!(a.dir, PathBuf::from("."));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn reads_the_conf() {
        let d = skeleton("cpp", "component", "conf");
        let p =
            Package::from_conf(&std::fs::read_to_string(d.join("app/app.conf")).unwrap()).unwrap();
        assert_eq!(p.component, "my_robot");
        assert_eq!(p.lang, Lang::Cpp);
        assert_eq!(p.source_path(), "src/my_robot.cpp");
        assert!(p.names_in_conf.contains(&"my_robot_get0".to_owned()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn words() {
        assert!(contains_word("fn arm(", "arm"));
        assert!(!contains_word("fn arm_get(", "arm"));
        assert!(!contains_word("fn farm(", "arm"));
        assert!(contains_word("x farm arm", "arm"));
    }

    /// 全言語・全種類の雛形に process と container を足せる。足した後は {{ が残らず、元の型も残る
    #[test]
    fn adds_to_every_skeleton() {
        for lang in ["rust", "python", "cpp", "haskell"] {
            for template in ["component", "process", "container"] {
                let d = skeleton(lang, template, "all");
                let before = conf_types(&d);
                let (pkg, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
                write_edits(&d, &files, None).unwrap();
                let (_, files) = plan(&d, AddKind::Container, "arm").unwrap();
                write_edits(&d, &files, None).unwrap();
                let after = conf_types(&d);
                for t in before.iter().chain(
                    [
                        "my_mul",
                        "my_mul0",
                        "arm",
                        "arm0",
                        "arm_increment0",
                        "arm_get0",
                    ]
                    .map(String::from)
                    .iter(),
                ) {
                    assert!(after.contains(t), "{lang} {template}: {t} not in {after:?}");
                }
                let src = std::fs::read_to_string(d.join(pkg.source_path())).unwrap();
                assert!(!src.contains("{{"), "{lang} {template}");
                for w in ["my_mul", "arm", "arm_increment", "arm_get"] {
                    assert!(contains_word(&src, w), "{lang} {template}: {w}\n{src}");
                }
                let _ = std::fs::remove_dir_all(&d);
            }
        }
    }

    #[test]
    fn rust_manifest() {
        let d = skeleton("rust", "process", "rm");
        let (_, files) = plan(&d, AddKind::Container, "arm").unwrap();
        write_edits(&d, &files, None).unwrap();
        let (_, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
        write_edits(&d, &files, None).unwrap();
        let src = std::fs::read_to_string(d.join("src/lib.rs")).unwrap();
        assert!(
            src.contains(
                "juiz_component_manifest!(\n    component_name = \"my_robot\"\n    containers = {\n        arm = [ arm_increment, arm_get ]\n    }\n    processes = [ my_robot_add, my_mul ]\n);"
            ),
            "{src}"
        );
        // 足したコードは juiz_component_manifest! の前
        assert!(src.find("fn my_mul(").unwrap() < src.find("juiz_component_manifest!(").unwrap());
        assert!(src.contains("pub struct Arm {"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rust_container_only_gets_processes() {
        let d = skeleton("rust", "container", "rc");
        let (_, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
        let src = &files[0].after;
        assert!(src.contains("    processes = [ my_mul ]\n);"), "{src}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn python_imports() {
        let d = skeleton("python", "process", "pi");
        let (_, files) = plan(&d, AddKind::Container, "arm").unwrap();
        let src = &files[0].after;
        assert!(
            src.contains(
                "\nfrom juiz import juiz_container, juiz_container_process, juiz_process\n"
            ),
            "{src}"
        );
        assert!(src.ends_with("    return container.value\n"));
        assert!(src.contains("    return a + b\n\n\nclass Arm:\n"), "{src}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cpp_manifest() {
        let d = skeleton("cpp", "container", "cm");
        let (_, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
        let src = &files[0].after;
        assert!(
            src.contains("            .add_process(my_robot_get_manifest()))\n        .add_process(my_mul_manifest());\n}"),
            "{src}"
        );
        // 足したコードは Component のマニフェストの関数 (とその前のコメント) の前
        assert!(src.contains("COMPONENT_PROCESS_FACTORY(my_mul_manif, my_mul);\n\n// The manifest of the whole Component"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn haskell_list() {
        let d = skeleton("haskell", "process", "hl");
        let (_, files) = plan(&d, AddKind::Container, "arm").unwrap();
        let src = &files[0].after;
        assert!(
            src.contains(
                "  [ componentProcess 'my_robot_add 'my_robot_add_manifest\n  , componentContainer 'arm_create 'arm_manifest\n  , componentContainerProcess 'arm_increment 'arm_increment_manifest\n  , componentContainerProcess 'arm_get 'arm_get_manifest\n  ]"
            ),
            "{src}"
        );
        assert!(src.contains("newtype ArmState = ArmState (IORef Int64)"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn conf_blocks_in_order() {
        // process の雛形に container を足すと、containers は processes の後ろ・ecs の前 (無ければ末尾)
        let d = skeleton("rust", "process", "cb");
        let (_, files) = plan(&d, AddKind::Container, "arm").unwrap();
        let conf = &files[1].after;
        let p = conf
            .find("\"processes\":\n  - \"type_name\": \"my_robot_add\"")
            .unwrap();
        let c = conf
            .find("\"containers\":\n  - \"type_name\": \"arm\"")
            .unwrap();
        assert!(p < c, "{conf}");
        assert!(conf.ends_with("        \"name\": \"arm_get0\"\n"), "{conf}");
        let _ = std::fs::remove_dir_all(&d);

        // container の雛形に process を足すと、processes は containers の前
        let d = skeleton("rust", "container", "cb2");
        let (_, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
        let conf = &files[1].after;
        let p = conf.find("\"processes\":\n  - \"type_name\": \"my_mul\"\n    \"name\": \"my_mul0\"\n\"containers\":").unwrap();
        assert!(p > 0, "{conf}");
        let _ = std::fs::remove_dir_all(&d);

        // component の雛形の containers は ecs の前 (ecs のコメントも ecs のもの)
        let d = skeleton("python", "component", "cb3");
        let (_, files) = plan(&d, AddKind::Container, "arm").unwrap();
        let conf = &files[1].after;
        assert!(
            conf.contains("        \"name\": \"arm_get0\"\n\"ecs\":\n"),
            "{conf}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn edits_flow_style_empty_lists() {
        let conf = "\"name\": \"x\"\n\"plugins\":\n  \"components\":\n    \"x\":\n      \"language\": \"rust\"\n\"processes\": []\n";
        let out = edit_conf(conf, AddKind::Process, "my_mul").unwrap();
        assert!(
            out.ends_with(
                "\"processes\":\n  - \"type_name\": \"my_mul\"\n    \"name\": \"my_mul0\"\n"
            ),
            "{out}"
        );
        assert!(edit_conf("\"processes\": [ {type_name: a} ]\n", AddKind::Process, "b").is_err());
    }

    #[test]
    fn errors() {
        let d = skeleton("rust", "component", "err");
        let e = |kind, name: &str| plan(&d, kind, name).unwrap_err().to_string();
        assert!(e(AddKind::Process, "my_robot_add").contains("already in app/app.conf"));
        assert!(e(AddKind::Process, "my_robot").contains("already in app/app.conf"));
        assert!(e(AddKind::Container, "my_robot_add").contains("already in app/app.conf"));
        assert!(e(AddKind::Process, "My").contains("My"));
        assert!(e(AddKind::Process, "type").contains("type"));
        // ソースにある識別子 (Rust の構造体 MyRobot) と衝突する
        std::fs::write(
            d.join("src/lib.rs"),
            std::fs::read_to_string(d.join("src/lib.rs")).unwrap() + "\nfn helper() {}\n",
        )
        .unwrap();
        assert!(e(AddKind::Process, "helper").contains("already used in src/lib.rs"));
        // マクロが無い
        std::fs::write(d.join("src/lib.rs"), "fn x() {}\n").unwrap();
        assert!(e(AddKind::Process, "my_mul").contains("juiz_component_manifest!"));
        let _ = std::fs::remove_dir_all(&d);

        // 雛形でない
        let d = temp("not_a_skeleton");
        std::fs::create_dir_all(&d).unwrap();
        assert!(plan(&d, AddKind::Process, "my_mul")
            .unwrap_err()
            .to_string()
            .contains("app/app.conf"));
        std::fs::create_dir_all(d.join("app")).unwrap();
        std::fs::write(d.join("app/app.conf"), "\"name\": \"x\"\n").unwrap();
        assert!(plan(&d, AddKind::Process, "my_mul")
            .unwrap_err()
            .to_string()
            .contains("plugins.components"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rollback_on_failure() {
        let d = skeleton("python", "component", "rb");
        let before: Vec<String> = ["app/python/my_robot.py", "app/app.conf"]
            .iter()
            .map(|p| std::fs::read_to_string(d.join(p)).unwrap())
            .collect();
        let (_, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
        assert!(write_edits(&d, &files, Some(1)).is_err());
        let after: Vec<String> = ["app/python/my_robot.py", "app/app.conf"]
            .iter()
            .map(|p| std::fs::read_to_string(d.join(p)).unwrap())
            .collect();
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn summary_lines() {
        let d = skeleton("cpp", "component", "sum");
        let (pkg, files) = plan(&d, AddKind::Process, "my_mul").unwrap();
        let s = summary(&pkg, AddKind::Process, "my_mul", &files, false);
        assert!(s.starts_with("Added process my_mul to my_robot (c++): my_mul\nEdited src/my_robot.cpp, app/app.conf\n"), "{s}");
        assert!(s.contains("scripts/dev.sh"));
        assert!(s.contains("/process/my_mul0::my_mul"));
        let s = summary(&pkg, AddKind::Container, "arm", &files, true);
        assert!(s.contains("scripts\\dev.ps1"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
