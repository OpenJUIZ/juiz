//! Linux(freedesktop)の関連付け(docs/spec/app-package.md §17.4、#396)
//!
//! - `<データ>/mime/packages/juiz-juizapp.xml`: `*.juizapp` → `application/x-juizapp`
//! - `<データ>/applications/juiz.desktop`: ダブルクリックで `juiz run --launched`
//! - `<データ>/applications/juiz-install.desktop`: 「このアプリで開く」の `juiz install --launched`
//! - `<設定>/mimeapps.list`: 既定のアプリを juiz.desktop に、両方を「関連付け」に足す
//!
//! `<データ>` は `$XDG_DATA_HOME`(無ければ `~/.local/share`)、`<設定>` は `$XDG_CONFIG_HOME`(無ければ `~/.config`)。

use super::tools::{warning, Tools};
use super::Associated;
use juiz_core::anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

pub(crate) const MIME_TYPE: &str = "application/x-juizapp";
pub(crate) const RUN_DESKTOP: &str = "juiz.desktop";
pub(crate) const INSTALL_DESKTOP: &str = "juiz-install.desktop";
pub(crate) const MIME_PACKAGE: &str = "juiz-juizapp.xml";

/// 書く場所
#[derive(Debug, Clone)]
pub(crate) struct Dirs {
    /// `$XDG_DATA_HOME`
    pub data: PathBuf,
    /// `$XDG_CONFIG_HOME`
    pub config: PathBuf,
}

impl Dirs {
    /// 環境変数から求める。XDG の値が相対パスなら(仕様に従い)無視する
    pub(crate) fn from_env() -> Result<Dirs> {
        let xdg = |name: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let home = || home::home_dir().ok_or_else(|| anyhow!("cannot find the home directory"));
        let data = match xdg("XDG_DATA_HOME") {
            Some(p) => p,
            None => home()?.join(".local").join("share"),
        };
        let config = match xdg("XDG_CONFIG_HOME") {
            Some(p) => p,
            None => home()?.join(".config"),
        };
        Ok(Dirs { data, config })
    }

    fn applications(&self) -> PathBuf {
        self.data.join("applications")
    }
    fn mime(&self) -> PathBuf {
        self.data.join("mime")
    }
    fn mimeapps(&self) -> PathBuf {
        self.config.join("mimeapps.list")
    }
}

/// .desktop の Exec の 1 引数として引用符で囲む(Desktop Entry Specification の Exec と文字列の規則)
///
/// 引用符の中では `"` `` ` `` `$` `\` の前に `\` を置き、`%` は `%%` にする。
/// そのうえで値の文字列として `\` を `\\` にする。
/// 囲む必要のある文字が無ければ囲まない(xdg-mime の generic の処理は引用符を外さずに実行ファイルを探すため)。
pub(crate) fn exec_quote(arg: &str) -> Result<String> {
    if arg.chars().any(|c| c.is_control()) {
        bail!("the path of juiz contains a control character: {arg:?}");
    }
    const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
    if !arg.is_empty() && !arg.chars().any(|c| RESERVED.contains(c)) {
        return Ok(arg.replace('%', "%%"));
    }
    let mut q = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                q.push('\\');
                q.push(c);
            }
            '%' => q.push_str("%%"),
            _ => q.push(c),
        }
    }
    q.push('"');
    Ok(q.replace('\\', "\\\\"))
}

/// [`exec_quote`] の逆: Exec の値の先頭の引数(実行ファイル)を取り出す
pub(crate) fn exec_executable(exec: &str) -> Option<String> {
    // 値の文字列としてのエスケープを戻す
    let mut value = String::new();
    let mut chars = exec.trim_start().chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => value.push(' '),
                Some('n') => value.push('\n'),
                Some('t') => value.push('\t'),
                Some('r') => value.push('\r'),
                Some(o) => value.push(o),
                None => value.push('\\'),
            }
        } else {
            value.push(c);
        }
    }
    let value = value.replace("%%", "%");
    let mut chars = value.chars();
    match chars.next()? {
        '"' => {
            let mut out = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => out.push(chars.next()?),
                    '"' => return Some(out),
                    _ => out.push(c),
                }
            }
            None
        }
        first => {
            let mut out = String::from(first);
            out.extend(chars.take_while(|c| !c.is_whitespace()));
            Some(out)
        }
    }
}

/// 書くファイル(`<データ>` からの相対パスと中身)
pub(crate) fn files(exe: &Path) -> Result<Vec<(PathBuf, String)>> {
    let exe = exe
        .to_str()
        .ok_or_else(|| anyhow!("the path of juiz is not UTF-8: {}", exe.display()))?;
    let q = exec_quote(exe)?;
    let mime = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Written by juiz associate; juiz associate with the remove option deletes it. XML comments cannot contain a double hyphen. -->
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="{MIME_TYPE}">
    <comment>JUIZ application</comment>
    <sub-class-of type="application/zip"/>
    <glob pattern="*.juizapp" weight="60"/>
  </mime-type>
</mime-info>
"#
    );
    let run = format!(
        "[Desktop Entry]
# Written by `juiz associate`; removed by `juiz associate --remove`.
Type=Application
Name=JUIZ
Comment=Run a JUIZ application (.juizapp)
Exec={q} run --launched %f
Terminal=true
NoDisplay=true
MimeType={MIME_TYPE};
"
    );
    // 「このアプリで開く」に出るよう NoDisplay にしない(GNOME は NoDisplay のアプリを一覧から外す)
    let install = format!(
        "[Desktop Entry]
# Written by `juiz associate`; removed by `juiz associate --remove`.
Type=Application
Name=Install with juiz
Comment=Install a JUIZ application (.juizapp)
Exec={q} install --launched %f
Terminal=true
MimeType={MIME_TYPE};
Categories=Development;
"
    );
    Ok(vec![
        (PathBuf::from("mime/packages").join(MIME_PACKAGE), mime),
        (PathBuf::from("applications").join(RUN_DESKTOP), run),
        (PathBuf::from("applications").join(INSTALL_DESKTOP), install),
    ])
}

/// mimeapps.list の `[section]` の `key` の値を書き換える(None なら消す)。変えたら true
fn edit_mimeapps(text: &str, section: &str, key: &str, value: Option<&str>) -> (String, bool) {
    let header = format!("[{section}]");
    let mut out: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut found_section = false;
    let mut done = false;
    let mut changed = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            // 前の節の終わりに足す
            if in_section && !done {
                if let Some(v) = value {
                    insert_before_blank(&mut out, format!("{key}={v}"));
                    changed = true;
                }
                done = true;
            }
            in_section = trimmed == header;
            found_section |= in_section;
            out.push(line.to_owned());
            continue;
        }
        if in_section && !done {
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == key {
                    done = true;
                    match value {
                        Some(new) if v.trim() == new => out.push(line.to_owned()),
                        Some(new) => {
                            out.push(format!("{key}={new}"));
                            changed = true;
                        }
                        None => changed = true,
                    }
                    continue;
                }
            }
        }
        out.push(line.to_owned());
    }
    if !done {
        if let Some(v) = value {
            if !found_section {
                if out.last().is_some_and(|l| !l.trim().is_empty()) {
                    out.push(String::new());
                }
                out.push(header);
            }
            insert_before_blank(&mut out, format!("{key}={v}"));
            changed = true;
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    (s, changed)
}

/// 末尾の空行の前に足す(節の間の空行を保つ)
fn insert_before_blank(out: &mut Vec<String>, line: String) {
    let mut at = out.len();
    while at > 0 && out[at - 1].trim().is_empty() {
        at -= 1;
    }
    out.insert(at, line);
}

/// mimeapps.list の `[section]` の `key` の値
fn mimeapps_value(text: &str, section: &str, key: &str) -> Option<String> {
    let header = format!("[{section}]");
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == header;
        } else if in_section {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_owned());
                }
            }
        }
    }
    None
}

const DEFAULT_SECTION: &str = "Default Applications";
const ADDED_SECTION: &str = "Added Associations";

fn default_value() -> String {
    format!("{RUN_DESKTOP};")
}
fn added_value() -> String {
    format!("{RUN_DESKTOP};{INSTALL_DESKTOP};")
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| anyhow!("cannot write {}: {e}", path.display()))
}

/// 今の juiz を .juizapp に関連付ける(§17.1)。警告(見つからなかったコマンドなど)も返す
pub(crate) fn associate(
    tools: &mut impl Tools,
    dirs: &Dirs,
    exe: &Path,
) -> Result<(Associated, Vec<String>)> {
    let wanted = files(exe)?;
    let mimeapps_text = std::fs::read_to_string(dirs.mimeapps()).unwrap_or_default();
    let same_files = wanted.iter().all(|(rel, text)| {
        std::fs::read_to_string(dirs.data.join(rel)).ok().as_deref() == Some(text.as_str())
    });
    let same_default = mimeapps_value(&mimeapps_text, DEFAULT_SECTION, MIME_TYPE).as_deref()
        == Some(default_value().as_str());
    if same_files && same_default {
        return Ok((Associated::Unchanged, Vec::new()));
    }
    let previous = std::fs::read_to_string(dirs.applications().join(RUN_DESKTOP))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("Exec=").map(str::to_owned))
        })
        .and_then(|e| exec_executable(&e))
        .filter(|p| Path::new(p) != exe);

    for (rel, text) in &wanted {
        write(&dirs.data.join(rel), text)?;
    }
    // xdg-mime default が書くのと同じ場所(利用者の mimeapps.list)を直接書く。xdg-utils が無くても効く
    let (text, _) = edit_mimeapps(
        &mimeapps_text,
        DEFAULT_SECTION,
        MIME_TYPE,
        Some(&default_value()),
    );
    let (text, _) = edit_mimeapps(&text, ADDED_SECTION, MIME_TYPE, Some(&added_value()));
    write(&dirs.mimeapps(), &text)?;

    let warnings = refresh(tools, dirs);
    Ok((
        match previous {
            Some(p) => Associated::Replaced(p),
            None => Associated::New,
        },
        warnings,
    ))
}

/// 関連付けを外す(§17.1)。外したものがあれば true
///
/// mimeapps.list の項目は、値が juiz の .desktop のときだけ消す(ほかのアプリの関連付けを消さないため)
pub(crate) fn remove(tools: &mut impl Tools, dirs: &Dirs) -> Result<(bool, Vec<String>)> {
    let mut removed = false;
    for (rel, _) in files(Path::new("juiz"))? {
        let p = dirs.data.join(rel);
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| anyhow!("cannot remove {}: {e}", p.display()))?;
            removed = true;
        }
    }
    if let Ok(text) = std::fs::read_to_string(dirs.mimeapps()) {
        let mut text = text;
        let mut changed = false;
        if mimeapps_value(&text, DEFAULT_SECTION, MIME_TYPE).as_deref()
            == Some(default_value().as_str())
        {
            let (t, c) = edit_mimeapps(&text, DEFAULT_SECTION, MIME_TYPE, None);
            text = t;
            changed |= c;
        }
        if mimeapps_value(&text, ADDED_SECTION, MIME_TYPE).as_deref()
            == Some(added_value().as_str())
        {
            let (t, c) = edit_mimeapps(&text, ADDED_SECTION, MIME_TYPE, None);
            text = t;
            changed |= c;
        }
        if changed {
            write(&dirs.mimeapps(), &text)?;
            removed = true;
        }
    }
    let warnings = if removed {
        refresh(tools, dirs)
    } else {
        Vec::new()
    };
    Ok((removed, warnings))
}

/// MIME の表と .desktop の索引を作り直す。無いコマンドは警告にして続ける
fn refresh(tools: &mut impl Tools, dirs: &Dirs) -> Vec<String> {
    let mime = dirs.mime();
    let apps = dirs.applications();
    let mut warnings = Vec::new();
    let o = tools.run("update-mime-database", &[mime.as_os_str()]);
    warnings.extend(warning(
        "update-mime-database",
        &o,
        "the file manager may not recognize .juizapp until the MIME database is updated (install shared-mime-info)",
    ));
    if apps.exists() {
        let o = tools.run("update-desktop-database", &[apps.as_os_str()]);
        warnings.extend(warning(
            "update-desktop-database",
            &o,
            "the desktop may pick up the change after you log in again (install desktop-file-utils)",
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::super::tools::fake::{temp_dir, FakeTools};
    use super::super::tools::ToolOutcome;
    use super::*;

    fn dirs(name: &str) -> Dirs {
        let d = temp_dir(name);
        Dirs {
            data: d.join("data"),
            config: d.join("config"),
        }
    }

    #[test]
    fn exec_quote_round_trips() {
        for p in [
            "/home/a/.venv/bin/juiz",
            "/home/a b/juiz",
            r#"/odd/"q"/$x/`y`/back\slash/100%/juiz"#,
            "/ジュイズ/juiz",
        ] {
            let q = exec_quote(p).unwrap();
            assert_eq!(
                exec_executable(&format!("{q} run --launched %f")).as_deref(),
                Some(p)
            );
        }
        assert_eq!(exec_quote("/usr/bin/juiz").unwrap(), "/usr/bin/juiz");
        assert_eq!(exec_quote("/a b").unwrap(), r#""/a b""#);
        assert_eq!(exec_quote(r"/a\b").unwrap(), r#""/a\\\\b""#);
        assert_eq!(exec_quote("/100%").unwrap(), "/100%%");
        assert_eq!(exec_quote("/1 00%").unwrap(), r#""/1 00%%""#);
        assert!(exec_quote("/a\nb").is_err());
        assert_eq!(
            exec_executable("/usr/bin/juiz run %f").as_deref(),
            Some("/usr/bin/juiz")
        );
    }

    #[test]
    fn mimeapps_edit_keeps_other_entries() {
        let text = "[Default Applications]\ntext/plain=gedit.desktop;\n\n[Added Associations]\ntext/plain=vim.desktop;\n";
        let (t, c) = edit_mimeapps(text, DEFAULT_SECTION, MIME_TYPE, Some("juiz.desktop;"));
        assert!(c);
        assert_eq!(
            t,
            "[Default Applications]\ntext/plain=gedit.desktop;\napplication/x-juizapp=juiz.desktop;\n\n[Added Associations]\ntext/plain=vim.desktop;\n"
        );
        let (t2, c) = edit_mimeapps(&t, DEFAULT_SECTION, MIME_TYPE, Some("juiz.desktop;"));
        assert!(!c);
        assert_eq!(t2, t);
        let (t3, c) = edit_mimeapps(&t, DEFAULT_SECTION, MIME_TYPE, None);
        assert!(c);
        assert_eq!(t3, text);
        // 節が無ければ作る
        let (t4, _) = edit_mimeapps("", ADDED_SECTION, MIME_TYPE, Some("a;"));
        assert_eq!(t4, "[Added Associations]\napplication/x-juizapp=a;\n");
        let (t5, _) = edit_mimeapps("[X]\nk=v\n", ADDED_SECTION, MIME_TYPE, Some("a;"));
        assert_eq!(
            t5,
            "[X]\nk=v\n\n[Added Associations]\napplication/x-juizapp=a;\n"
        );
    }

    #[test]
    fn associate_writes_files_and_refreshes() {
        let d = dirs("linux_new");
        let mut tools = FakeTools::default();
        let exe = Path::new("/home/a/.venv/bin/juiz");
        let (r, w) = associate(&mut tools, &d, exe).unwrap();
        assert_eq!(r, Associated::New);
        assert!(w.is_empty(), "{w:?}");

        let run = std::fs::read_to_string(d.data.join("applications/juiz.desktop")).unwrap();
        assert!(
            run.contains("Exec=/home/a/.venv/bin/juiz run --launched %f\n"),
            "{run}"
        );
        assert!(run.contains("Terminal=true\n"));
        assert!(run.contains("MimeType=application/x-juizapp;\n"));
        let install =
            std::fs::read_to_string(d.data.join("applications/juiz-install.desktop")).unwrap();
        assert!(
            install.contains("Exec=/home/a/.venv/bin/juiz install --launched %f\n"),
            "{install}"
        );
        let mime = std::fs::read_to_string(d.data.join("mime/packages/juiz-juizapp.xml")).unwrap();
        assert!(mime.contains(r#"<glob pattern="*.juizapp""#));
        // update-mime-database は壊れた XML を黙って飛ばすので、XML として読めることを確かめる
        roxmltree::Document::parse(&mime).unwrap();
        let list = std::fs::read_to_string(d.config.join("mimeapps.list")).unwrap();
        assert!(
            list.contains("[Default Applications]\napplication/x-juizapp=juiz.desktop;\n"),
            "{list}"
        );
        assert!(
            list.contains(
                "[Added Associations]\napplication/x-juizapp=juiz.desktop;juiz-install.desktop;\n"
            ),
            "{list}"
        );

        let mime_dir = d.data.join("mime").display().to_string();
        let apps_dir = d.data.join("applications").display().to_string();
        assert_eq!(
            tools.calls,
            vec![
                vec!["update-mime-database".to_owned(), mime_dir],
                vec!["update-desktop-database".to_owned(), apps_dir],
            ]
        );

        // 同じ juiz ならなにもしない
        let mut tools = FakeTools::default();
        assert_eq!(
            associate(&mut tools, &d, exe).unwrap().0,
            Associated::Unchanged
        );
        assert!(tools.calls.is_empty());

        // 別の juiz なら前のパスを返す
        let (r, _) = associate(&mut tools, &d, Path::new("/opt/juiz/bin/juiz")).unwrap();
        assert_eq!(r, Associated::Replaced("/home/a/.venv/bin/juiz".to_owned()));
        let _ = std::fs::remove_dir_all(d.data.parent().unwrap());
    }

    #[test]
    fn missing_tools_are_warnings() {
        let d = dirs("linux_missing");
        let mut tools = FakeTools::default();
        tools
            .outcomes
            .insert("update-mime-database".into(), ToolOutcome::NotFound);
        tools.outcomes.insert(
            "update-desktop-database".into(),
            ToolOutcome::Failed("exit status: 1".into()),
        );
        let (r, w) = associate(&mut tools, &d, Path::new("/usr/bin/juiz")).unwrap();
        assert_eq!(r, Associated::New);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(
            w[0].starts_with("update-mime-database was not found"),
            "{w:?}"
        );
        assert!(
            w[1].starts_with("update-desktop-database failed (exit status: 1)"),
            "{w:?}"
        );
        let _ = std::fs::remove_dir_all(d.data.parent().unwrap());
    }

    #[test]
    fn remove_keeps_other_apps() {
        let d = dirs("linux_remove");
        let mut tools = FakeTools::default();
        assert!(!remove(&mut tools, &d).unwrap().0);
        assert!(tools.calls.is_empty());

        std::fs::create_dir_all(&d.config).unwrap();
        std::fs::write(
            d.config.join("mimeapps.list"),
            "[Default Applications]\ntext/plain=gedit.desktop;\n",
        )
        .unwrap();
        associate(&mut tools, &d, Path::new("/usr/bin/juiz")).unwrap();
        assert!(remove(&mut tools, &d).unwrap().0);
        assert!(!d.data.join("applications/juiz.desktop").exists());
        assert!(!d.data.join("applications/juiz-install.desktop").exists());
        assert!(!d.data.join("mime/packages/juiz-juizapp.xml").exists());
        let list = std::fs::read_to_string(d.config.join("mimeapps.list")).unwrap();
        assert!(!list.contains("juizapp"), "{list}");
        assert!(list.contains("text/plain=gedit.desktop;"), "{list}");

        // 既定が別のアプリなら mimeapps.list のその項目は残す
        associate(&mut tools, &d, Path::new("/usr/bin/juiz")).unwrap();
        let list = std::fs::read_to_string(d.config.join("mimeapps.list")).unwrap();
        let list = list.replace(
            "application/x-juizapp=juiz.desktop;\n",
            "application/x-juizapp=other.desktop;\n",
        );
        std::fs::write(d.config.join("mimeapps.list"), &list).unwrap();
        assert!(remove(&mut tools, &d).unwrap().0);
        let list = std::fs::read_to_string(d.config.join("mimeapps.list")).unwrap();
        assert!(
            list.contains("application/x-juizapp=other.desktop;"),
            "{list}"
        );
        assert!(!list.contains("juiz-install.desktop"), "{list}");
        let _ = std::fs::remove_dir_all(d.data.parent().unwrap());
    }
}
