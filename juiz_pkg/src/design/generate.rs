//! 設計文書から雛形を作る (docs/spec/design.md §6・§7・§8.2、設計文書 Step 2)
//!
//! `juiz pkg create -t`(プリセット)と `juiz pkg create -d`(文書のファイル)の両方が通る。
//! 文書を juiz_idl のシグネチャにし、juiz_idl の設計文書用の雛形 (`rust_design_items`・`python_design_items`)
//! で Component のソースを作る。-t は本体の断片 (templates/design/fragments/) を入れ、-d は未実装の印のまま。
//! Rust・Python は Step 2 (#595)、C++・Haskell・Zig は Step 3 (#596) (`cpp_design_items`・`haskell_design_items`・
//! `zig_design_items`)。

use std::path::{Path, PathBuf};

use juiz_idl::{DesignSkeleton, ExternNamespace, GenerateOptions, OutputKind, ProcessSignature};
use juiz_types::DataType;
use serde_json::{Map, Value};

use super::check::{check_and_resolve, Resolved};
use super::{parse, preset, to_yaml, Design, FILE_NAME};
use crate::render::{render, Vars};
use crate::{names, templates, Lang, PkgError, Template};

/// juiz_std の型の名前空間 (juiz_std_types のものを使う、datatype.md §24.2)
const JUIZ_STD: &str = "juiz_std";
const JUIZ_STD_RUST_PATH: &str = "::juiz_std_types::juiz_std";

/// 文書がどこから来たか
#[derive(Clone, Debug)]
pub enum Origin {
    /// `-t` のプリセット (§7)
    Preset(Template),
    /// `-d` の文書のファイル (§8.2)
    File {
        /// 表示に使うファイル名 (`juizdesign.yaml`)
        label: String,
        /// 文書のあるディレクトリ
        dir: PathBuf,
        /// 文書のファイル (`-d` に渡したもの)
        path: PathBuf,
        /// 雛形に写す文書の中身 (language の行を書き換えたもの)
        text: String,
    },
}

/// 文書から雛形を作るための、検査の済んだもの
#[derive(Clone, Debug)]
pub struct DesignPlan {
    pub design: Design,
    pub origin: Origin,
    pub lang: Lang,
    pub resolved: Resolved,
    /// 雛形に写す .jidl (文書のディレクトリからの / 区切りの相対パス、中身)。組み込みのものは写さない
    pub idl_files: Vec<(String, String)>,
    /// build.rs で読む .jidl (`idl:` の組み込みでないもの)
    pub compile: Vec<String>,
    /// `idl:` のすべて (組み込みのものも、先頭の `./` を外したもの)。C++・Haskell・Zig のビルドが型を作るときに読む
    pub entries: Vec<String>,
    /// Haskell の型のモジュール (.cabal の other-modules。scripts/dev.sh が gen/ に作る)
    pub hs_modules: Vec<String>,
    /// Python: 作るときに書く型のコード (雛形のディレクトリからの相対パス、中身)
    pub generated: Vec<(String, String)>,
    /// juiz_std の型を使うか (Rust は juiz_std_types に依存し、extern にする)
    pub uses_juiz_std: bool,
}

fn is_juiz_std(namespace: &str) -> bool {
    namespace == JUIZ_STD || namespace.starts_with("juiz_std.")
}

/// 本体の断片 (templates/design/fragments/<lang>/<役割>.<ext>.tmpl) を置換して読む
fn fragment(lang: &str, file: &str, name: &str) -> Option<String> {
    let key = format!("design/fragments/{lang}/{file}.tmpl");
    let (_, text) = templates().iter().find(|(p, _)| *p == key)?;
    let mut vars = Vars::new();
    vars.set("name", name).set("Name", names::camel(name));
    Some(render(&text.replace("\r\n", "\n"), &vars).expect("the fragment renders"))
}

impl DesignPlan {
    /// `-t` のプリセットの文書 (§7.1) に雛形の言語を書いたもの
    pub fn preset(name: &str, template: Template, lang: Lang) -> Result<DesignPlan, PkgError> {
        let mut design = preset(name, template);
        design.component.language = Some(lang);
        let (report, resolved) = check_and_resolve(&design, Some(lang), Path::new("."));
        let resolved = resolved.ok_or_else(|| {
            PkgError::new(format!("the preset design is not valid:\n{}", report.error_text(FILE_NAME)))
        })?;
        Self::build(design, Origin::Preset(template), lang, resolved, Path::new("."))
    }

    /// `-d` の文書を読んで検査する (§8.2)。言語は `lang`、無ければ文書の `component.language`
    pub fn from_file(path: &Path, lang: Option<Lang>) -> Result<(DesignPlan, Vec<String>), PkgError> {
        let label = path.display().to_string();
        let text = std::fs::read_to_string(path)
            .map_err(|e| PkgError::new(format!("-d {label}: cannot read it: {e}")))?;
        let text = text.replace("\r\n", "\n");
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();
        let design = parse(&text).map_err(|r| {
            PkgError::new(format!("{label} is not a valid design document:\n{}", r.error_text(&label)))
        })?;
        let mut warnings = Vec::new();
        let lang = match (lang, design.component.language) {
            (Some(l), Some(d)) if l != d => {
                // W1
                warnings.push(format!(
                    "{label}: -b {} is not component.language ({}) of the document; using -b {}",
                    l.dir(),
                    d.dir(),
                    l.dir()
                ));
                l
            }
            (Some(l), _) => l,
            (None, Some(d)) => d,
            (None, None) => {
                return Err(PkgError::new(format!(
                    "-d {label}: give the language with -b <rust|python|cpp|haskell|zig> (the document has no component.language)"
                )))
            }
        };
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let (report, resolved) = check_and_resolve(&design, Some(lang), &dir);
        for w in &report.warnings {
            warnings.push(w.format(&label, "warning").replacen(&format!("{label}: warning: "), &format!("{label}: "), 1));
        }
        let resolved = match resolved {
            Some(r) => r,
            None => {
                return Err(PkgError::new(format!(
                    "{label} is not a valid design document:\n{}",
                    report.error_text(&label)
                )))
            }
        };
        // 雛形に写す文書: -b と language が違えば language の行を書き換える (§8.2)
        let copy = match design.component.language {
            Some(d) if d != lang => match rewrite_language(&text, lang) {
                Some(t) => t,
                None => {
                    warnings.push(format!(
                        "{label}: the line 'language: {}' under component: was not found, so juizdesign.yaml of the skeleton still says {}; change it by hand",
                        d.dir(),
                        d.dir()
                    ));
                    text.clone()
                }
            },
            _ => text.clone(),
        };
        let origin = Origin::File {
            label: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| FILE_NAME.to_owned()),
            dir: dir.clone(),
            path: path.to_path_buf(),
            text: copy,
        };
        let plan = Self::build(design, origin, lang, resolved, &dir)?;
        Ok((plan, warnings))
    }

    fn build(design: Design, origin: Origin, lang: Lang, resolved: Resolved, dir: &Path) -> Result<DesignPlan, PkgError> {
        let model = &resolved.model;
        let uses_juiz_std = model.types.iter().any(|t| match t {
            DataType::Record(r) => is_juiz_std(&r.namespace),
            DataType::Enum(e) => is_juiz_std(&e.namespace),
            _ => false,
        });
        // 写す .jidl: 文書のディレクトリの下のもの (検査で外のものはエラーにしてある)。組み込みのものは写さない
        let mut idl_files = Vec::new();
        if !design.idl.is_empty() {
            let base = dir.canonicalize().map_err(|e| PkgError::new(format!("{}: {e}", dir.display())))?;
            for f in &model.files {
                let Ok(abs) = f.canonicalize() else { continue };
                let Ok(rel) = abs.strip_prefix(&base) else { continue };
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let text = std::fs::read_to_string(&abs)
                    .map_err(|e| PkgError::new(format!("{}: {e}", abs.display())))?;
                idl_files.push((rel, text.replace("\r\n", "\n")));
            }
        }
        let compile: Vec<String> = design
            .idl
            .iter()
            .filter(|p| idl_files.iter().any(|(f, _)| f == p.trim_start_matches("./")))
            .map(|p| p.trim_start_matches("./").to_owned())
            .collect();
        // Python の型のコード: 作るときに app/python/ に書く (juiz_std は pyjuiz のもの、pkg.md §6.5 と同じ)
        let mut generated = Vec::new();
        if lang == Lang::Python && !model.types.is_empty() {
            let options = GenerateOptions {
                externs: vec![ExternNamespace {
                    namespace: JUIZ_STD.to_owned(),
                    rust_path: None,
                }],
            };
            let files = juiz_idl::generate_with(model, &[OutputKind::Python], &options)
                .map_err(|e| PkgError::new(e.to_string()))?;
            for f in files {
                generated.push((
                    format!("app/python/{}", f.path.to_string_lossy().replace('\\', "/")),
                    f.contents,
                ));
            }
        }
        // `idl:` のすべて (C++ の juiz_generate_types・Haskell の dev.sh・Zig の addTypes が読む)
        let entries: Vec<String> = design.idl.iter().map(|p| p.trim_start_matches("./").to_owned()).collect();
        // Haskell の型のモジュール (.cabal の other-modules)
        let mut hs_modules = Vec::new();
        if lang == Lang::Haskell && !model.types.is_empty() {
            let files = juiz_idl::generate(model, &[OutputKind::Haskell]).map_err(|e| PkgError::new(e.to_string()))?;
            for f in files {
                let p = f.path.to_string_lossy().replace('\\', "/");
                if let Some(m) = p.strip_suffix(".hs") {
                    hs_modules.push(m.replace('/', "."));
                }
            }
        }
        Ok(DesignPlan {
            design,
            origin,
            lang,
            resolved,
            idl_files,
            compile,
            entries,
            hs_modules,
            generated,
            uses_juiz_std,
        })
    }

    pub fn name(&self) -> &str {
        &self.design.component.name
    }

    pub fn preset_template(&self) -> Option<Template> {
        match self.origin {
            Origin::Preset(t) => Some(t),
            Origin::File { .. } => None,
        }
    }

    /// build.rs を置くか (Rust で、写す .jidl があるとき)
    pub fn has_build_rs(&self) -> bool {
        self.lang == Lang::Rust && !self.compile.is_empty()
    }

    fn options(&self) -> GenerateOptions {
        GenerateOptions {
            externs: if self.uses_juiz_std {
                vec![ExternNamespace {
                    namespace: JUIZ_STD.to_owned(),
                    rust_path: Some(JUIZ_STD_RUST_PATH.to_owned()),
                }]
            } else {
                Vec::new()
            },
        }
    }

    /// 本体の断片 (§7.2)。-t のプリセットだけ。鍵は型の役割 (`<name>_add` の `add` など) で、型名ではない。
    /// C++・Haskell・Zig のファイルの先頭の説明と Component の説明もここに入れる
    fn skeleton(&self) -> DesignSkeleton {
        let mut s = DesignSkeleton {
            header: self.header(),
            description: self.design.component.description.clone(),
            types: !self.entries.is_empty(),
            ..Default::default()
        };
        let Some(template) = self.preset_template() else {
            return s;
        };
        let name = self.name();
        let (dir, ext) = lang_fragments(self.lang);
        let mut roles = Vec::new();
        if template.has_process() {
            roles.push((format!("{name}_add"), "add"));
        }
        if template.has_container() {
            roles.push((format!("{name}_increment"), "increment"));
            roles.push((format!("{name}_get"), "get"));
        }
        for (type_name, role) in roles {
            if let Some(b) = fragment(dir, &format!("{role}.{ext}"), name) {
                s.bodies.insert(type_name, b);
            }
        }
        // Rust のテスト (Process `<name>_add` を呼ぶので、Process のあるプリセットだけ)
        if self.lang == Lang::Rust && template.has_process() {
            s.tail = fragment(dir, "tests.rs", name);
        }
        s
    }

    /// Component のソース (雛形のディレクトリからの相対パス、中身)
    pub fn source(&self) -> Result<(String, String), PkgError> {
        let e = |e: juiz_idl::IdlError| PkgError::new(e.to_string());
        let name = self.name();
        let sigs = &self.resolved.signatures;
        let skeleton = self.skeleton();
        let from = match &self.origin {
            Origin::File { label, .. } => label.clone(),
            Origin::Preset(_) => FILE_NAME.to_owned(),
        };
        Ok(match self.lang {
            Lang::Rust => {
                let items = juiz_idl::rust_design_items(sigs, name, &self.options(), &skeleton).map_err(e)?;
                let head = match self.origin {
                    Origin::Preset(_) => fragment("rust", "header.rs", name).unwrap_or_default(),
                    Origin::File { .. } => format!(
                        "//! {name}: a juiz component made from {from} (juiz pkg create -d).\n\
                         //!\n\
                         //! Write the body of each function (replace the `let _ = …;` and `todo!()` lines).\n\
                         //! The arguments and the return values are typed by {from}: when you change it,\n\
                         //! change the functions here to match.\n"
                    ),
                };
                let include = if self.has_build_rs() {
                    "\n// The Rust types (record, enum) of the .jidl files, generated by build.rs\n\
                     include!(concat!(env!(\"OUT_DIR\"), \"/juiz_idl.rs\"));\n\n"
                } else if matches!(self.origin, Origin::File { .. }) {
                    "\n"
                } else {
                    ""
                };
                ("src/lib.rs".to_owned(), format!("{head}{include}{items}"))
            }
            Lang::Python => {
                let items = juiz_idl::python_design_items(sigs, &skeleton).map_err(e)?;
                let head = match self.origin {
                    Origin::Preset(_) => fragment("python", "header.py", name).unwrap_or_default(),
                    Origin::File { .. } => {
                        let types = if self.generated.is_empty() {
                            String::new()
                        } else {
                            "The record and enum types are in the packages next to this file, generated from the .jidl\n\
                             files of idl: (see README.md).\n"
                                .to_owned()
                        };
                        format!(
                            "\"\"\"{name}: a juiz component made from {from} (juiz pkg create -d).\n\n\
                             Write the body of each function (replace the `raise NotImplementedError(...)` lines).\n\
                             The arguments and the return values are typed by {from}: when you change it,\n\
                             change the functions here to match.\n{types}\"\"\"\n"
                        )
                    }
                };
                (format!("app/python/{name}.py"), format!("{head}{items}"))
            }
            Lang::Cpp => {
                let text = juiz_idl::cpp_design_items(sigs, name, &skeleton).map_err(e)?;
                (format!("src/{name}.cpp"), text)
            }
            Lang::Haskell => {
                let module = names::camel(name);
                if let Some(m) = self.hs_modules.iter().find(|m| **m == module) {
                    return Err(PkgError::new(format!(
                        "the Haskell module of a namespace of the types is {m}, the same as the module of the component; use another component name or namespace"
                    )));
                }
                let text = juiz_idl::haskell_design_items(sigs, name, &module, &skeleton).map_err(e)?;
                (format!("src/{module}.hs"), text)
            }
            Lang::Zig => {
                let text = juiz_idl::zig_design_items(sigs, name, &skeleton).map_err(e)?;
                (format!("src/{name}.zig"), text)
            }
        })
    }

    /// C++・Haskell・Zig のファイルの先頭の説明。-t は断片の `header`、-d は文書から作ったことと本体の書き方
    fn header(&self) -> String {
        let name = self.name();
        let (dir, ext) = lang_fragments(self.lang);
        if let Origin::Preset(_) = self.origin {
            return fragment(dir, &format!("header.{ext}"), name).unwrap_or_default();
        }
        let from = match &self.origin {
            Origin::File { label, .. } => label.as_str(),
            Origin::Preset(_) => FILE_NAME,
        };
        let types = self.design_idl_label();
        match self.lang {
            Lang::Cpp => {
                let mut h = format!(
                    "// {name}: a juiz component made from {from} (juiz pkg create -d).\n\
                     //\n\
                     // Write the body of each function (replace the `(void)…;` and `throw` lines).\n\
                     // The arguments and the return values are typed by {from}: when you change it,\n\
                     // change the functions and the manifests here to match.\n"
                );
                if let Some(t) = types {
                    h.push_str(&format!(
                        "// The record and enum types are in juiz_idl.h, which CMake generates from {} (juiz_generate_types).\n",
                        t.replace('`', "")
                    ));
                }
                h
            }
            Lang::Haskell => {
                let mut h = format!(
                    "-- | {name}: a juiz component made from {from} (juiz pkg create -d).\n\
                     --\n\
                     -- Write the body of each function (replace the @error \"write the body of …\"@ lines).\n\
                     -- The arguments and the return values are typed by {from}: when you change it,\n\
                     -- change the functions and the manifests here to match.\n"
                );
                if let Some(t) = types {
                    h.push_str(&format!(
                        "-- The record and enum types are in the modules that scripts/dev.sh generates in gen/ from {}.\n",
                        t.replace('`', "")
                    ));
                }
                h
            }
            Lang::Zig => {
                let mut h = format!(
                    "//! {name}: a juiz component made from {from} (juiz pkg create -d).\n\
                     //!\n\
                     //! Write the body of each function (replace the `return error.NotImplemented` lines).\n\
                     //! The arguments and the return values are typed by {from}: when you change it,\n\
                     //! change the functions and the parts here to match.\n"
                );
                if let Some(t) = types {
                    h.push_str(&format!(
                        "//! The record and enum types are in the module juiz_types, which build.zig generates from {} (juiz.addTypes).\n",
                        t.replace('`', "")
                    ));
                }
                h
            }
            Lang::Rust | Lang::Python => String::new(),
        }
    }

    /// 雛形の直下に置く文書 (pkg.md §2.2、design.md §8.2)
    pub fn doc_file(&self) -> (String, String) {
        let text = match &self.origin {
            Origin::Preset(_) => to_yaml(&self.design),
            Origin::File { text, .. } => text.clone(),
        };
        (FILE_NAME.to_owned(), text)
    }

    /// 生成後の表示の 1 行目の種類 (pkg.md §2.5)
    pub fn kind_label(&self) -> String {
        match &self.origin {
            Origin::Preset(t) => t.dir().to_owned(),
            Origin::File { label, .. } => format!("from {label}"),
        }
    }

    /// app.conf の processes・containers (と、プリセットの ecs) (§6.3)
    pub fn conf_types(&self) -> String {
        let sigs = &self.resolved.signatures;
        let mut s = String::new();
        if !sigs.processes.is_empty() {
            s.push_str("\"processes\":\n");
            for p in &sigs.processes {
                s.push_str(&format!("  - \"type_name\": \"{0}\"\n    \"name\": \"{0}0\"\n", p.type_name));
            }
        }
        if !sigs.containers.is_empty() {
            s.push_str("\"containers\":\n");
            for c in &sigs.containers {
                s.push_str(&format!("  - \"type_name\": \"{0}\"\n    \"name\": \"{0}0\"\n", c.type_name));
                for a in &c.arguments {
                    let v = match &a.default {
                        Some(d) => d.clone(),
                        None => zero(&a.schema),
                    };
                    s.push_str(&format!("    \"{}\": {}\n", a.name, serde_json::to_string(&v).unwrap()));
                }
                if !c.container_processes.is_empty() {
                    s.push_str("    \"processes\":\n");
                    for p in &c.container_processes {
                        s.push_str(&format!(
                            "      - \"type_name\": \"{0}\"\n        \"name\": \"{0}0\"\n",
                            p.type_name
                        ));
                    }
                }
            }
        }
        // プリセットは今の -t と同じ ecs を書く (§6.3)
        if let Some(t) = self.preset_template() {
            if t.has_container() {
                if let Some(ecs) = fragment("common", "ecs.conf", self.name()) {
                    s.push_str(&ecs);
                }
            }
        }
        s.trim_end().to_owned()
    }

    /// README の型の表 (§6.4)
    pub fn types_table(&self) -> String {
        let mut s = String::from(
            "| Type | Kind | Arguments (default) | Returns | What it does |\n| --- | --- | --- | --- | --- |\n",
        );
        let sigs = &self.resolved.signatures;
        let row = |s: &mut String, kind: &str, p: &ProcessSignature, design: Option<&super::ProcessDesign>| {
            let returns = match design.and_then(|d| d.returns.as_deref()) {
                Some(r) => format!("`{r}`"),
                None => "-".to_owned(),
            };
            let kind = if p.readonly { format!("{kind} (readonly)") } else { kind.to_owned() };
            s.push_str(&format!(
                "| `{}` | {kind} | {} | {returns} | {} |\n",
                p.type_name,
                arguments_cell(design.map(|d| d.arguments.as_slice()).unwrap_or(&[]), &p.arguments),
                cell(p.description.as_deref())
            ));
        };
        for (p, d) in sigs.processes.iter().zip(&self.design.processes) {
            row(&mut s, "Process", p, Some(d));
        }
        for (c, d) in sigs.containers.iter().zip(&self.design.containers) {
            s.push_str(&format!(
                "| `{}` | Container | {} | - | {} |\n",
                c.type_name,
                arguments_cell(&d.arguments, &c.arguments),
                cell(c.description.as_deref())
            ));
            for (p, pd) in c.container_processes.iter().zip(&d.processes) {
                row(&mut s, "ContainerProcess", p, Some(pd));
            }
        }
        s.trim_end().to_owned()
    }

    /// 呼び出しの例 (§6.4、pkg.md §9)。引数は既定値、無ければ型の 0 値 (`image` は null)
    pub fn call_examples(&self) -> Vec<String> {
        let sigs = &self.resolved.signatures;
        let mut v = Vec::new();
        for p in &sigs.processes {
            v.push(format!(
                "juiz process call \"http://127.0.0.1:8000/process/{0}0::{0}\" '{1}' -p",
                p.type_name,
                example_arguments(p)
            ));
        }
        for c in &sigs.containers {
            for p in &c.container_processes {
                v.push(format!(
                    "juiz container-process call \"http://127.0.0.1:8000/container_process/{0}0::{0}:{1}0\" '{2}' -p",
                    p.type_name,
                    c.type_name,
                    example_arguments(p)
                ));
            }
        }
        v
    }

    /// 本体を書く関数のあるファイル
    pub fn source_path(&self) -> String {
        let name = self.name();
        match self.lang {
            Lang::Python => format!("app/python/{name}.py"),
            Lang::Rust => "src/lib.rs".to_owned(),
            Lang::Cpp => format!("src/{name}.cpp"),
            Lang::Haskell => format!("src/{}.hs", names::camel(name)),
            Lang::Zig => format!("src/{name}.zig"),
        }
    }

    /// C++・Haskell・Zig の README に書く `idl:` の並び (`` `idl/a.jidl`, `juiz_std/geometry.jidl` ``)。無ければ None
    pub fn design_idl_label(&self) -> Option<String> {
        (!self.entries.is_empty())
            .then(|| self.entries.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", "))
    }

    /// C++ の juiz_generate_types の IDL・Haskell の dev.sh の `juiz idl` に渡す `idl:` (空白区切り)。無ければ None
    pub fn idl_list(&self) -> Option<String> {
        (!self.entries.is_empty()).then(|| self.entries.join(" "))
    }

    /// C++ の juiz_generate_types の DEPENDS (写した .jidl のうち `idl:` に無いもの = import した先)。無ければ None
    pub fn idl_depends(&self) -> Option<String> {
        let v: Vec<&str> = self
            .idl_files
            .iter()
            .map(|(p, _)| p.as_str())
            .filter(|p| !self.entries.iter().any(|e| e == p))
            .collect();
        (!v.is_empty()).then(|| v.join(" "))
    }

    /// Zig の build.zig の addTypes: .files (写した .jidl のうち `idl:` にあるもの)、
    /// .depends (import した先)、.builtins (組み込みのもの、無ければ None)。`&.{ b.path("…"), … }` の形
    pub fn zig_types(&self) -> (String, String, Option<String>) {
        let (files, depends, builtins) = self.zig_type_items();
        (zig_list(&files), zig_list(&depends), builtins.map(|b| zig_list(&b)))
    }

    fn zig_type_items(&self) -> (String, String, Option<String>) {
        let copied = |p: &str| self.idl_files.iter().any(|(f, _)| f == p);
        let files = self
            .entries
            .iter()
            .filter(|p| copied(p))
            .map(|p| format!("b.path(\"{p}\")"))
            .collect::<Vec<_>>()
            .join(", ");
        let depends = self
            .idl_files
            .iter()
            .map(|(p, _)| p.as_str())
            .filter(|p| !self.entries.iter().any(|e| e == p))
            .map(|p| format!("b.path(\"{p}\")"))
            .collect::<Vec<_>>()
            .join(", ");
        let builtins: Vec<String> = self.entries.iter().filter(|p| !copied(p)).map(|p| format!("\"{p}\"")).collect();
        (files, depends, (!builtins.is_empty()).then(|| builtins.join(", ")))
    }

    /// Haskell の .cabal の other-modules に足す型のモジュール (`, A.B` を並べたもの)
    pub fn hs_other_modules(&self) -> String {
        self.hs_modules.iter().map(|m| format!("\n                    , {m}")).collect()
    }

    /// build.rs の compile の並び (`"idl/a.jidl", "idl/b.jidl"`)
    pub fn compile_list(&self) -> String {
        self.compile.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>().join(", ")
    }

    /// README に書く .jidl の並び (`` `idl/a.jidl`, `idl/b.jidl` ``)。写すものが無ければ None
    pub fn idl_label(&self) -> Option<String> {
        (!self.compile.is_empty()).then(|| {
            self.compile.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", ")
        })
    }

    /// build.rs の extern_path の行 (juiz_std を使うときだけ)
    pub fn extern_line(&self) -> Option<String> {
        self.uses_juiz_std
            .then(|| format!("        .extern_path({JUIZ_STD:?}, {JUIZ_STD_RUST_PATH:?})"))
    }

    /// Python の README の「.jidl を変えたら型を作り直す」段落 (型のコードを書いたときだけ)
    pub fn python_idl_note(&self) -> Option<String> {
        (self.lang == Lang::Python && !self.generated.is_empty() && !self.compile.is_empty()).then(|| {
            format!(
                "When you change the `.jidl` files, generate their Python types again (the `juiz_std` types come with pyjuiz, so they are not generated):\n\n```sh\njuiz idl -g python --extern juiz_std -o app/python {}\n```",
                self.compile.join(" ")
            )
        })
    }
}

/// Zig のスライスのリテラル (zig fmt の書き方: 空なら `&.{}`、1 つなら `&.{a}`、2 つ以上なら `&.{ a, b }`)
fn zig_list(items: &str) -> String {
    if items.is_empty() {
        "&.{}".to_owned()
    } else if items.contains(", ") {
        format!("&.{{ {items} }}")
    } else {
        format!("&.{{{items}}}")
    }
}

/// 言語ごとの本体の断片のディレクトリと拡張子 (templates/design/fragments/<dir>/<役割>.<ext>.tmpl)
fn lang_fragments(lang: Lang) -> (&'static str, &'static str) {
    match lang {
        Lang::Rust => ("rust", "rs"),
        Lang::Python => ("python", "py"),
        Lang::Cpp => ("cpp", "cpp"),
        Lang::Haskell => ("haskell", "hs"),
        Lang::Zig => ("zig", "zig"),
    }
}

/// 型の 0 値 (§4.4)
fn zero(schema: &juiz_types::TypeSchema) -> Value {
    schema
        .data_type()
        .map(|t| crate::idl::example_value(&t))
        .unwrap_or(Value::Null)
}

fn example_arguments(p: &ProcessSignature) -> String {
    let mut m = Map::new();
    for a in &p.arguments {
        let v = match &a.default {
            Some(d) => d.clone(),
            None => zero(&a.schema),
        };
        m.insert(a.name.clone(), v);
    }
    // ' は shell の '…' の中に書けないので逃がす
    serde_json::to_string(&Value::Object(m))
        .unwrap()
        .replace('\'', "'\\''")
}

/// 表の 1 つのセル (| と改行を逃がす)
fn cell(s: Option<&str>) -> String {
    match s {
        Some(s) if !s.trim().is_empty() => s
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace('|', "\\|"),
        _ => "-".to_owned(),
    }
}

/// 引数のセル。型は文書に書いた型の式
fn arguments_cell(design: &[super::ArgumentDesign], sigs: &[juiz_idl::ArgumentSignature]) -> String {
    if sigs.is_empty() {
        return "-".to_owned();
    }
    sigs.iter()
        .map(|a| {
            let t = design
                .iter()
                .find(|d| d.name == a.name)
                .map(|d| d.type_expr.as_str())
                .unwrap_or(a.schema.name.as_str());
            let d = a
                .default
                .as_ref()
                .map(|d| format!(" ({})", serde_json::to_string(d).unwrap().replace('|', "\\|")))
                .unwrap_or_default();
            format!("`{}: {}`{d}", a.name, t.replace('|', "\\|"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `component:` の下の `language:` の行の値を書き換える (§8.2)。行が無ければ None
pub fn rewrite_language(text: &str, lang: Lang) -> Option<String> {
    let mut in_component = false;
    let mut done = false;
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let top = !body.starts_with([' ', '\t']) && !body.trim().is_empty() && !body.trim_start().starts_with('#');
        if top {
            in_component = body.trim_end() == "component:" || body.starts_with("component:") && body["component:".len()..].trim_start().starts_with('#');
        }
        if in_component && !done && !top {
            let indent = &body[..body.len() - body.trim_start().len()];
            if let Some(rest) = body.trim_start().strip_prefix("language:") {
                // 値だけを書き換え、前後の空白とコメントは残す
                let value = rest.trim_start();
                let lead = &rest[..rest.len() - value.len()];
                let end = value.find(char::is_whitespace).unwrap_or(value.len());
                let after = &value[end..];
                let nl = &line[body.len()..];
                out.push(format!("{indent}language:{lead}{}{after}{nl}", lang.dir()));
                done = true;
                continue;
            }
        }
        out.push(line.to_owned());
    }
    done.then(|| out.concat())
}
