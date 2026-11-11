//! `juiz pkg create --idl` (docs/spec/pkg.md §6.5、issue #460・#468)
//!
//! .jidl の `process` / `container` 宣言から、型付きの関数 (本体は未実装) の Component を作る。
//! 関数は juiz_idl の各言語の雛形 (datatype.md §18・§20.6・§21.6・§22.4) をそのまま使い、
//! コンポーネントの名前だけ `<name>` にする。

use std::path::{Component, Path};

use juiz_idl::{
    ExternNamespace, GenerateOptions, IdlModel, OutputKind, ProcessSignature, Signatures,
};
use juiz_types::DataType;
use serde_json::{Map, Value};

use crate::{names, Lang, PkgError};

/// juiz_std の型の名前空間 (juiz_std_types のものを使う、datatype.md §24.2)
const JUIZ_STD: &str = "juiz_std";
const JUIZ_STD_RUST_PATH: &str = "::juiz_std_types::juiz_std";

/// .jidl を読んで決めたもの
#[derive(Debug, Clone)]
pub struct IdlPlan {
    pub lang: Lang,
    /// 作るときに書く型のコード (雛形のディレクトリからの相対パス、中身)。Python の app/python/<a>/<b>/__init__.py
    pub generated: Vec<(String, String)>,
    /// Haskell の型のモジュール (.cabal の other-modules。scripts/dev.sh が gen/ に作る)
    pub hs_modules: Vec<String>,
    /// 雛形の idl/ に写すファイル (雛形のディレクトリからの / 区切りの相対パス、中身)
    pub files: Vec<(String, String)>,
    /// build.rs に書く .jidl (`idl/<ファイル名>`)
    pub main: String,
    /// 宣言のある名前空間の宣言
    pub signatures: Signatures,
    /// juiz_std の型を使うか (juiz_std_types に依存し、extern にする)
    pub uses_juiz_std: bool,
    /// 名前付き型 (呼び出しの例の値を作るため)
    pub types: Vec<DataType>,
}

fn err(path: &Path, msg: impl std::fmt::Display) -> PkgError {
    PkgError::new(format!("--idl {}: {msg}", path.display()))
}

/// `base` からの相対パスにする。`base` の外なら None
fn relative_under(path: &Path, base: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn is_juiz_std(namespace: &str) -> bool {
    namespace == JUIZ_STD || namespace.starts_with("juiz_std.")
}

impl IdlPlan {
    /// .jidl を読んで検査する (ファイルはまだ書かない)
    pub fn load(path: &Path, lang: Lang) -> Result<IdlPlan, PkgError> {
        if !path.is_file() {
            return Err(err(path, "no such file"));
        }
        let model = juiz_idl::load_files(&[path]).map_err(|e| err(path, e))?;
        let signatures = Self::signatures(&model, path)?;
        let uses_juiz_std = model
            .types
            .iter()
            .any(|t| matches!(t, DataType::Record(r) if is_juiz_std(&r.namespace))
                || matches!(t, DataType::Enum(e) if is_juiz_std(&e.namespace)));
        // 写すファイル: 渡した .jidl と、同じディレクトリの下から import したもの (同じ並びで idl/ に置く)
        let main_abs = path
            .canonicalize()
            .map_err(|e| err(path, e))?;
        let base = main_abs.parent().unwrap_or(Path::new("/")).to_path_buf();
        let mut files = Vec::new();
        let mut main = String::new();
        for f in &model.files {
            let abs = f.canonicalize().map_err(|e| err(f, e))?;
            let rel = relative_under(&abs, &base).ok_or_else(|| {
                err(
                    path,
                    format!(
                        "it imports {}, which is not in the directory of the .jidl (only files under {} are copied to idl/)",
                        f.display(),
                        base.display()
                    ),
                )
            })?;
            let text = std::fs::read_to_string(&abs).map_err(|e| err(f, e))?;
            let rel = format!("idl/{rel}");
            if abs == main_abs {
                main = rel.clone();
            }
            files.push((rel, text.replace("\r\n", "\n")));
        }
        // 型のコード: Python は作るときに app/python/ に書き (juiz_std は pyjuiz のもの)、
        // C++・Haskell はビルドのときに作るので、ここでは作れるか (名前空間の循環など) だけ確かめる
        let mut generated = Vec::new();
        let mut hs_modules = Vec::new();
        match lang {
            Lang::Rust => {}
            Lang::Python => {
                let options = GenerateOptions {
                    externs: vec![ExternNamespace {
                        namespace: JUIZ_STD.to_owned(),
                        rust_path: None,
                    }],
                };
                let files = juiz_idl::generate_with(&model, &[OutputKind::Python], &options)
                    .map_err(|e| err(path, e))?;
                for f in files {
                    generated.push((
                        format!("app/python/{}", f.path.to_string_lossy().replace('\\', "/")),
                        f.contents,
                    ));
                }
            }
            Lang::Cpp => {
                juiz_idl::generate(&model, &[OutputKind::Cpp]).map_err(|e| err(path, e))?;
            }
            Lang::Haskell => {
                let files = juiz_idl::generate(&model, &[OutputKind::Haskell])
                    .map_err(|e| err(path, e))?;
                for f in files {
                    let p = f.path.to_string_lossy().replace('\\', "/");
                    if let Some(m) = p.strip_suffix(".hs") {
                        hs_modules.push(m.replace('/', "."));
                    }
                }
            }
        }
        let plan = IdlPlan {
            lang,
            generated,
            hs_modules,
            files,
            main,
            signatures,
            uses_juiz_std,
            types: model.types.clone(),
        };
        // 生成できるかを先に確かめる (その言語のキーワードの名前、使えない型など)
        plan.source("x").map_err(|e| err(path, e))?;
        Ok(plan)
    }

    /// 雛形の Component のソース (雛形のディレクトリからの相対パス、中身)。
    /// juiz_idl の雛形の出力をそのまま入れるので、テンプレートの置換は通さない
    pub fn source(&self, name: &str) -> Result<(String, String), PkgError> {
        let e = |e: juiz_idl::IdlError| PkgError::new(e.to_string());
        Ok(match self.lang {
            Lang::Rust => ("src/lib.rs".to_owned(), self.lib_rs(name)?),
            Lang::Python => {
                let items = juiz_idl::python_skeleton_items(&self.signatures).map_err(e)?;
                (
                    format!("app/python/{name}.py"),
                    format!(
                        "\"\"\"{name}: a juiz component made from {main} (juiz pkg create --idl).\n\n\
                         Write the body of each function (replace the `raise NotImplementedError(...)` lines).\n\
                         The arguments and the return values are typed by the .jidl. The record and enum types\n\
                         are in the packages next to this file, generated from {main}: when you change {main},\n\
                         generate them again (see README.md) and change the functions here to match.\n\
                         \"\"\"\n{items}",
                        main = self.main
                    ),
                )
            }
            Lang::Cpp => {
                let items = juiz_idl::cpp_skeleton_items(&self.signatures, name).map_err(e)?;
                (
                    format!("src/{name}.cpp"),
                    format!(
                        "// {name}: a juiz component made from {main} (juiz pkg create --idl).\n\
                         //\n\
                         // Write the body of each function (replace the `(void)…;` and `throw` lines).\n\
                         // The arguments and the return values are typed by the .jidl: the record and enum types are\n\
                         // in juiz_idl.h, which CMake generates from {main} (juiz_generate_types in CMakeLists.txt).\n\
                         // When you change {main}, change the functions and the manifests here to match.\n\
                         \n{items}",
                        main = self.main
                    ),
                )
            }
            Lang::Haskell => {
                let module = names::camel(name);
                if let Some(m) = self.hs_modules.iter().find(|m| **m == module) {
                    return Err(PkgError::new(format!(
                        "the Haskell module of a namespace of the .jidl is {m}, the same as the module of the component; use another <name> or namespace"
                    )));
                }
                let doc = format!(
                    "-- | {name}: a juiz component made from {main} (juiz pkg create --idl).\n\
                     --\n\
                     -- Write the body of each function (replace the @error \"write the body of …\"@ lines).\n\
                     -- The arguments and the return values are typed by the .jidl: the record and enum types are in\n\
                     -- the modules that scripts/dev.sh generates from {main} in gen/. When you change {main},\n\
                     -- change the functions and the manifests here (and other-modules in the .cabal) to match.\n",
                    main = self.main
                );
                let text = juiz_idl::haskell_skeleton_items(&self.signatures, name, &module, &doc)
                    .map_err(e)?;
                (format!("src/{module}.hs"), text)
            }
        })
    }

    /// C++ の juiz_generate_types の DEPENDS (渡した .jidl のほかの写し、空白区切り)。無ければ None
    pub fn depends(&self) -> Option<String> {
        let v: Vec<&str> = self
            .files
            .iter()
            .map(|(p, _)| p.as_str())
            .filter(|p| *p != self.main)
            .collect();
        (!v.is_empty()).then(|| v.join(" "))
    }

    /// Haskell の .cabal の other-modules に足す型のモジュール (`, A.B` を並べたもの)
    pub fn hs_other_modules(&self) -> String {
        self.hs_modules
            .iter()
            .map(|m| format!("\n                    , {m}"))
            .collect()
    }

    /// 本体を書く関数のあるファイル (生成後の表示の「本体を書く」行)
    pub fn source_path(&self, name: &str) -> String {
        match self.lang {
            Lang::Rust => "src/lib.rs".to_owned(),
            Lang::Python => format!("app/python/{name}.py"),
            Lang::Cpp => format!("src/{name}.cpp"),
            Lang::Haskell => format!("src/{}.hs", names::camel(name)),
        }
    }

    /// 生成後の表示の「本体を書く」行
    pub fn write_bodies_line(&self, name: &str) -> String {
        let what = match self.lang {
            Lang::Rust => "todo!()",
            Lang::Python => "raise NotImplementedError",
            Lang::Cpp => "throw std::runtime_error",
            Lang::Haskell => "error \"…\"",
        };
        format!(
            "# write the bodies of the functions ({what}) in {}, then",
            self.source_path(name)
        )
    }

    /// 宣言のある名前空間は 1 つだけ (1 雛形 = 1 Component、pkg.md §4)
    fn signatures(model: &IdlModel, path: &Path) -> Result<Signatures, PkgError> {
        let with_decls: Vec<&Signatures> = model
            .signatures
            .iter()
            .filter(|s| !s.processes.is_empty() || !s.containers.is_empty())
            .filter(|s| !is_juiz_std(&s.namespace))
            .collect();
        match with_decls.as_slice() {
            [] => Err(err(path, "it has no process or container declaration")),
            [s] => Ok((*s).clone()),
            many => Err(err(
                path,
                format!(
                    "process / container declarations are in {} namespaces ({}); a package is one Component, so put them in one namespace",
                    many.len(),
                    {
                        let mut v: Vec<&str> = many.iter().map(|s| s.namespace.as_str()).collect();
                        v.sort();
                        v.join(", ")
                    }
                ),
            )),
        }
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

    /// src/lib.rs
    pub fn lib_rs(&self, name: &str) -> Result<String, PkgError> {
        let items = juiz_idl::rust_skeleton_items(&self.signatures, name, &self.options())
            .map_err(|e| PkgError::new(e.to_string()))?;
        Ok(format!(
            "//! {name}: a juiz component made from {main} (juiz pkg create --idl).\n\
             //!\n\
             //! Write the body of each function (replace the `let _ = …;` and `todo!()` lines).\n\
             //! The arguments and the return values are typed by the .jidl: when you change {main},\n\
             //! change the signatures here to match (cargo build regenerates the types).\n\
             \n\
             // The Rust types (record, enum) of the .jidl, generated by build.rs\n\
             include!(concat!(env!(\"OUT_DIR\"), \"/juiz_idl.rs\"));\n\
             \n\
             {items}",
            main = self.main
        ))
    }

    /// build.rs の extern_path の行 (juiz_std を使うときだけ)
    pub fn extern_line(&self) -> Option<String> {
        self.uses_juiz_std.then(|| {
            format!("        .extern_path({JUIZ_STD:?}, {JUIZ_STD_RUST_PATH:?})")
        })
    }

    /// app.conf の processes と containers (pkg.md §5.1 と同じキーと順番)
    pub fn conf_types(&self) -> String {
        let mut s = String::new();
        if !self.signatures.processes.is_empty() {
            s.push_str("\"processes\":\n");
            for p in &self.signatures.processes {
                s.push_str(&format!(
                    "  - \"type_name\": \"{0}\"\n    \"name\": \"{0}0\"\n",
                    p.type_name
                ));
            }
        }
        if !self.signatures.containers.is_empty() {
            s.push_str("\"containers\":\n");
            for c in &self.signatures.containers {
                s.push_str(&format!(
                    "  - \"type_name\": \"{0}\"\n    \"name\": \"{0}0\"\n",
                    c.type_name
                ));
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
        s.trim_end().to_owned()
    }

    /// 入っている型の表 (README、pkg.md §4 の表と同じ列)
    pub fn types_table(&self) -> String {
        let mut s = String::from(
            "| Type | Kind | Arguments (default) | Returns | What it does |\n| --- | --- | --- | --- | --- |\n",
        );
        let row = |s: &mut String, kind: &str, p: &ProcessSignature| {
            s.push_str(&format!(
                "| `{}` | {kind} | {} | `{}` | {} |\n",
                p.type_name,
                arguments_cell(p),
                p.output_schema.name,
                cell(p.description.as_deref())
            ));
        };
        for p in &self.signatures.processes {
            row(&mut s, "Process", p);
        }
        for c in &self.signatures.containers {
            s.push_str(&format!(
                "| `{}` | Container | - | - | {} |\n",
                c.type_name,
                cell(c.description.as_deref())
            ));
            for p in &c.container_processes {
                row(&mut s, "ContainerProcess", p);
            }
        }
        s.trim_end().to_owned()
    }

    /// 呼び出しの例 (pkg.md §9)。引数は既定値、無ければ型から作った値
    pub fn call_examples(&self) -> Vec<String> {
        let mut v = Vec::new();
        for p in &self.signatures.processes {
            v.push(format!(
                "juiz process call \"http://127.0.0.1:8000/process/{0}0::{0}\" '{1}' -p",
                p.type_name,
                self.example_arguments(p)
            ));
        }
        for c in &self.signatures.containers {
            for p in &c.container_processes {
                v.push(format!(
                    "juiz container-process call \"http://127.0.0.1:8000/container_process/{0}0::{0}:{1}0\" '{2}' -p",
                    p.type_name,
                    c.type_name,
                    self.example_arguments(p)
                ));
            }
        }
        v
    }

    fn example_arguments(&self, p: &ProcessSignature) -> String {
        let mut m = Map::new();
        for a in &p.arguments {
            let v = match &a.default {
                Some(d) => d.clone(),
                None => a
                    .schema
                    .data_type()
                    .map(|t| example_value(&t))
                    .unwrap_or(Value::Null),
            };
            m.insert(a.name.clone(), v);
        }
        // ' は shell の '…' の中に書けないので逃がす
        serde_json::to_string(&Value::Object(m))
            .unwrap()
            .replace('\'', "'\\''")
    }
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

fn arguments_cell(p: &ProcessSignature) -> String {
    if p.arguments.is_empty() {
        return "-".to_owned();
    }
    p.arguments
        .iter()
        .map(|a| {
            let d = a
                .default
                .as_ref()
                .map(|d| format!(" ({})", serde_json::to_string(d).unwrap().replace('|', "\\|")))
                .unwrap_or_default();
            format!("`{}: {}`{d}", a.name, a.schema.name)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// 型に合う値 (辞書の表し方、datatype.md §4)。record のフィールドは既定値があればそれを使う
pub fn example_value(t: &DataType) -> Value {
    match t {
        DataType::Null | DataType::Any | DataType::Image | DataType::Optional { .. } => Value::Null,
        DataType::Boolean => Value::Bool(false),
        DataType::Int | DataType::Long => Value::from(0),
        DataType::Float | DataType::Double => Value::from(0.0),
        DataType::String | DataType::Bytes => Value::from(""),
        DataType::Array(_) => Value::Array(Vec::new()),
        DataType::Map(_) => Value::Object(Map::new()),
        DataType::Record(r) => Value::Object(
            r.fields
                .iter()
                .map(|f| {
                    (
                        f.name.clone(),
                        f.default_as_dict().unwrap_or_else(|| example_value(&f.data_type)),
                    )
                })
                .collect(),
        ),
        DataType::Enum(e) => Value::from(
            e.default
                .clone()
                .or_else(|| e.symbols.first().cloned())
                .unwrap_or_default(),
        ),
    }
}

/// テスト用: 一時ディレクトリの .jidl
#[cfg(test)]
pub(crate) fn write_jidl(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_pkg_idl_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    const ROBOT: &str = r#"namespace my_robot;
import idl "juiz_std/geometry.jidl";
import idl "common/goal.jidl";
/** Distance | to the goal */
process double distance(my_robot.common.Goal goal, juiz_std.geometry.Pose2D from_pose);
container arm { container_process long move_by(long step = 1, string note = "it's"); }
"#;
    const GOAL: &str = r#"namespace my_robot.common;
import idl "juiz_std/geometry.jidl";
enum Mode { FAST, SLOW } = SLOW;
record Goal { juiz_std.geometry.Pose2D pose; double tolerance = 0.1; Mode mode; long id; }
"#;

    #[test]
    fn loads_declarations_and_imports() {
        let d = temp("load");
        write_jidl(&d.join("common"), "goal.jidl", GOAL);
        let p = write_jidl(&d, "robot.jidl", ROBOT);
        let plan = IdlPlan::load(&p, Lang::Rust).unwrap();
        let paths: Vec<&str> = plan.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["idl/robot.jidl", "idl/common/goal.jidl"]);
        assert_eq!(plan.main, "idl/robot.jidl");
        assert!(plan.uses_juiz_std);
        assert_eq!(plan.signatures.namespace, "my_robot");

        let lib = plan.lib_rs("my_pkg").unwrap();
        assert!(lib.contains("include!(concat!(env!(\"OUT_DIR\"), \"/juiz_idl.rs\"));"), "{lib}");
        assert!(lib.contains("component_name = \"my_pkg\""), "{lib}");
        assert!(
            lib.contains("pub fn distance(goal: crate::my_robot::common::Goal, from_pose: ::juiz_std_types::juiz_std::geometry::Pose2D) -> JuizResult<f64>"),
            "{lib}"
        );
        assert!(lib.contains("container: &mut ContainerImpl<Arm>"), "{lib}");
        assert!(!lib.contains("名前空間"), "{lib}");

        let conf = plan.conf_types();
        assert!(conf.starts_with("\"processes\":\n  - \"type_name\": \"distance\"\n    \"name\": \"distance0\"\n\"containers\":\n"), "{conf}");
        assert!(conf.contains("      - \"type_name\": \"move_by\"\n        \"name\": \"move_by0\""), "{conf}");

        let table = plan.types_table();
        assert!(table.contains("| `distance` | Process | `goal: my_robot.common.Goal`, `from_pose: juiz_std.geometry.Pose2D` | `double` | Distance \\| to the goal |"), "{table}");
        assert!(table.contains("| `move_by` | ContainerProcess | `step: long` (1), `note: string` (\"it's\") | `long` | - |"), "{table}");

        let calls = plan.call_examples();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].contains("/process/distance0::distance"), "{}", calls[0]);
        assert!(calls[0].contains(r#""tolerance":0.1"#) && calls[0].contains(r#""mode":"SLOW""#) && calls[0].contains(r#""theta":0.0"#), "{}", calls[0]);
        assert!(calls[1].contains("/container_process/move_by0::move_by:arm0"), "{}", calls[1]);
        assert!(calls[1].contains(r#"'{"note":"it'\''s","step":1}'"#), "{}", calls[1]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn errors() {
        let d = temp("errors");
        // 4 つの言語で読める
        let p = write_jidl(&d, "a.jidl", "namespace a; process long f(long x);");
        for lang in [Lang::Rust, Lang::Python, Lang::Cpp, Lang::Haskell] {
            assert!(IdlPlan::load(&p, lang).is_ok(), "{lang:?}");
        }
        // 無いファイル・文法の誤り
        assert!(IdlPlan::load(&d.join("none.jidl"), Lang::Rust).unwrap_err().to_string().contains("no such file"));
        let p = write_jidl(&d, "bad.jidl", "namespace a; process long f(");
        assert!(IdlPlan::load(&p, Lang::Rust).unwrap_err().to_string().contains("bad.jidl"));
        // 宣言が無い・名前空間が 2 つ
        let p = write_jidl(&d, "types.jidl", "namespace a; record R { long x; }");
        assert!(IdlPlan::load(&p, Lang::Rust).unwrap_err().to_string().contains("no process or container"));
        write_jidl(&d, "other.jidl", "namespace b; process long g();");
        let p = write_jidl(&d, "two.jidl", "namespace a; import idl \"other.jidl\"; process long f();");
        let e = IdlPlan::load(&p, Lang::Rust).unwrap_err().to_string();
        assert!(e.contains("a, b"), "{e}");
        // ディレクトリの外の import
        write_jidl(&d, "outside.jidl", "namespace o; record O { long x; }");
        let p = write_jidl(&d.join("sub"), "in.jidl", "namespace i; import idl \"../outside.jidl\"; process o.O f();");
        let e = IdlPlan::load(&p, Lang::Rust).unwrap_err().to_string();
        assert!(e.contains("not in the directory"), "{e}");
        // その言語のキーワード (Haskell は関数名に _process などを付けるので書ける)
        let p = write_jidl(&d, "kw.jidl", "namespace a; process long `loop`(long x);");
        assert!(IdlPlan::load(&p, Lang::Rust).unwrap_err().to_string().contains("keyword"));
        let p = write_jidl(&d, "kwpy.jidl", "namespace a; process long `lambda`(long x);");
        assert!(IdlPlan::load(&p, Lang::Python).unwrap_err().to_string().contains("Python keyword"));
        let p = write_jidl(&d, "kwcpp.jidl", "namespace a; process long `delete`(long x);");
        assert!(IdlPlan::load(&p, Lang::Cpp).unwrap_err().to_string().contains("C++ keyword"));
        assert!(IdlPlan::load(&p, Lang::Haskell).is_ok());
        // Python: 既定値のある引数の後ろに、既定値の無い record の引数は書けない
        let p = write_jidl(&d, "order.jidl", "namespace o; record R { long x; } process long f(long a = 1, o.R r);");
        let e = IdlPlan::load(&p, Lang::Python).unwrap_err().to_string();
        assert!(e.contains("argument r has no default but comes after a"), "{e}");
        assert!(IdlPlan::load(&p, Lang::Cpp).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_juiz_std() {
        let d = temp("nostd");
        let p = write_jidl(&d, "a.jidl", "namespace a; process long f(long x);");
        let plan = IdlPlan::load(&p, Lang::Rust).unwrap();
        assert!(!plan.uses_juiz_std);
        assert!(plan.extern_line().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
