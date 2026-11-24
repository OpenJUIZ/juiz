//! `juiz pkg create --idl` の .jidl を読む (docs/spec/pkg.md §6.5、design.md §6.2、issue #460・#468・#597)
//!
//! 設計文書 Step 4 (#597) から、`--idl` は「.jidl → 設計文書 → 雛形」の 1 本の道になった。
//! ここでは .jidl を読んで設計文書にする ([`IdlSource`]) だけで、雛形は設計文書から作る (design/generate.rs)。
//! [`IdlPlan`] は、設計文書の検査が juiz_idl の各言語の雛形 (`*_skeleton_items`) で作れるかを確かめるのに使う。

use std::path::{Component, Path, PathBuf};

use juiz_idl::{ExternNamespace, GenerateOptions, IdlModel, ProcessSignature, Signatures};
use juiz_types::DataType;
use serde_json::{Map, Value};

use crate::design::{ArgumentDesign, ComponentDesign, ContainerDesign, Design, ProcessDesign};
use crate::{names, Lang, PkgError};

/// juiz_std の型の名前空間 (juiz_std_types のものを使う、datatype.md §24.2)
const JUIZ_STD: &str = "juiz_std";
const JUIZ_STD_RUST_PATH: &str = "::juiz_std_types::juiz_std";

/// juiz_idl の `*_skeleton_items` で Component のソースを作るためのもの。
/// 設計文書の検査 (design.md §5 の N6・T6) が、その言語で作れるかを確かめるのに使う
#[derive(Debug, Clone)]
pub struct IdlPlan {
    pub lang: Lang,
    /// Haskell の型のモジュール (Component のモジュールとぶつからないかを見る)
    pub hs_modules: Vec<String>,
    /// ソースの先頭の説明に書くファイル
    pub main: String,
    /// 宣言のある名前空間の宣言
    pub signatures: Signatures,
    /// juiz_std の型を使うか (Rust は juiz_std_types のものを extern にする)
    pub uses_juiz_std: bool,
}

/// `--idl` で渡した .jidl を読んだもの。設計文書を作る元 (design.md §6.2・§8.4、設計文書 Step 4)
#[derive(Debug, Clone)]
pub struct IdlSource {
    /// 渡したパス (エラーの表示に使う)
    pub path: PathBuf,
    /// .jidl のあるディレクトリ (正規化したもの)
    pub dir: PathBuf,
    /// 渡した .jidl (`dir` からの / 区切りの相対パス)
    pub main: String,
    /// 渡した .jidl と、それが import するもの (`dir` からの / 区切りの相対パス)。組み込みのものは入らない
    pub files: Vec<String>,
    /// 宣言のある名前空間の宣言
    pub signatures: Signatures,
}

fn err(path: &Path, msg: impl std::fmt::Display) -> PkgError {
    PkgError::new(format!("--idl {}: {msg}", path.display()))
}

/// `base` からの相対パスにする。`base` の外なら None
pub(crate) fn relative_under(path: &Path, base: &Path) -> Option<String> {
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

impl IdlSource {
    /// .jidl を読む。宣言のある名前空間が 1 つで、import するものが同じディレクトリの下にあること (pkg.md §6.5)。
    /// その言語で作れるかは、作った設計文書の検査 (design.md §5) で確かめる
    pub fn load(path: &Path) -> Result<IdlSource, PkgError> {
        if !path.is_file() {
            return Err(err(path, "no such file"));
        }
        let model = juiz_idl::load_files(&[path]).map_err(|e| err(path, e))?;
        let signatures = signatures(&model, path)?;
        let main_abs = path.canonicalize().map_err(|e| err(path, e))?;
        let dir = main_abs.parent().unwrap_or(Path::new("/")).to_path_buf();
        let mut files = Vec::new();
        let mut main = String::new();
        for f in &model.files {
            let abs = f.canonicalize().map_err(|e| err(f, e))?;
            let rel = relative_under(&abs, &dir).ok_or_else(|| {
                err(
                    path,
                    format!(
                        "it imports {}, which is not in the directory of the .jidl (only files under {} are copied to idl/)",
                        f.display(),
                        dir.display()
                    ),
                )
            })?;
            if abs == main_abs {
                main = rel.clone();
            }
            files.push(rel);
        }
        Ok(IdlSource {
            path: path.to_path_buf(),
            dir,
            main,
            files,
            signatures,
        })
    }

    /// .jidl の宣言を設計文書にする (design.md §6.2)。型の参照は完全名、`idl:` は `idl_entry` だけ。
    /// Container の引数・状態は空、readonly は書かない
    pub fn design(&self, name: &str, language: Option<Lang>, idl_entry: &str) -> Result<Design, PkgError> {
        let type_of = |schema: &juiz_types::TypeSchema, what: &str| {
            schema
                .data_type()
                .map_err(|e| err(&self.path, format!("{what}: {e}")))
        };
        let process = |p: &ProcessSignature| -> Result<ProcessDesign, PkgError> {
            let mut arguments = Vec::new();
            for a in &p.arguments {
                let t = type_of(&a.schema, &format!("{} argument {}", p.type_name, a.name))?;
                arguments.push(ArgumentDesign {
                    name: a.name.clone(),
                    type_expr: juiz_idl::type_expression(&t),
                    default: a.default.clone(),
                    description: a.description.clone(),
                });
            }
            let returns = match type_of(&p.output_schema, &p.type_name)? {
                DataType::Null => None,
                t => Some(juiz_idl::type_expression(&t)),
            };
            Ok(ProcessDesign {
                name: p.type_name.clone(),
                description: p.description.clone(),
                arguments,
                returns,
                readonly: false,
            })
        };
        let mut processes = Vec::new();
        for p in &self.signatures.processes {
            processes.push(process(p)?);
        }
        let mut containers = Vec::new();
        for c in &self.signatures.containers {
            let mut cps = Vec::new();
            for p in &c.container_processes {
                cps.push(process(p)?);
            }
            containers.push(ContainerDesign {
                name: c.type_name.clone(),
                description: c.description.clone(),
                arguments: Vec::new(),
                state: Vec::new(),
                processes: cps,
            });
        }
        Ok(Design {
            component: ComponentDesign {
                name: name.to_owned(),
                description: None,
                language,
            },
            idl: vec![idl_entry.to_owned()],
            processes,
            containers,
            positions: Vec::new(),
        })
    }
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

impl IdlPlan {
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
            Lang::Zig => {
                let doc = format!(
                    "//! {name}: a juiz component made from {main} (juiz pkg create --idl).\n\
                     //!\n\
                     //! Write the body of each function (replace the `return error.NotImplemented` lines).\n\
                     //! The arguments and the return values are typed by the .jidl: the record and enum types are in\n\
                     //! the module juiz_types, which build.zig generates from {main} (juiz.addTypes).\n\
                     //! When you change {main}, change the functions and the parts here to match.\n",
                    main = self.main
                );
                let text = juiz_idl::zig_skeleton_items(&self.signatures, name, &doc).map_err(e)?;
                (format!("src/{name}.zig"), text)
            }
        })
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
    use crate::design::generate::DesignPlan;
    use crate::design::to_yaml;

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
container arm { container_process long move_by(long step = 1, string note = "it's", long? limit, union { string, null } tag = "x"); container_process void stop(); }
"#;
    const GOAL: &str = r#"namespace my_robot.common;
import idl "juiz_std/geometry.jidl";
enum Mode { FAST, SLOW } = SLOW;
record Goal { juiz_std.geometry.Pose2D pose; double tolerance = 0.1; Mode mode; long id; }
"#;

    /// .jidl → 設計文書 (design.md §6.2)
    #[test]
    fn loads_and_makes_a_design() {
        let d = temp("load");
        write_jidl(&d.join("common"), "goal.jidl", GOAL);
        let p = write_jidl(&d, "robot.jidl", ROBOT);
        let src = IdlSource::load(&p).unwrap();
        assert_eq!(src.main, "robot.jidl");
        assert_eq!(src.files, ["robot.jidl", "common/goal.jidl"]);
        assert_eq!(src.signatures.namespace, "my_robot");
        let design = src.design("my_pkg", Some(Lang::Rust), "idl/robot.jidl").unwrap();
        let yaml = to_yaml(&design);
        assert!(yaml.contains("component:\n  name: my_pkg\n  language: rust\nidl:\n  - idl/robot.jidl\nprocesses:\n  - name: distance\n    description: \"Distance | to the goal\"\n    arguments:\n      - { name: goal, type: my_robot.common.Goal }\n      - { name: from_pose, type: juiz_std.geometry.Pose2D }\n    returns: double\n"), "{yaml}");
        assert!(yaml.contains("containers:\n  - name: arm\n    processes:\n      - name: move_by\n        arguments:\n          - { name: step, type: long, default: 1 }\n          - { name: note, type: string, default: \"it's\" }\n          - { name: limit, type: \"long?\" }\n          - { name: tag, type: \"union { string, null }\", default: \"x\" }\n        returns: long\n      - name: stop\n"), "{yaml}");
        // 書いた文書を読み直すと同じもの
        assert_eq!(crate::design::parse(&yaml).unwrap().processes, design.processes);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `create --idl` の文書: .jidl は雛形の idl/ に置く
    #[test]
    fn design_plan_from_idl() {
        let d = temp("plan");
        write_jidl(&d.join("common"), "goal.jidl", GOAL);
        let p = write_jidl(&d, "robot.jidl", ROBOT);
        let plan = DesignPlan::from_idl(&p, "my_pkg", Lang::Rust).unwrap();
        let paths: Vec<&str> = plan.idl_files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["idl/robot.jidl", "idl/common/goal.jidl"]);
        assert_eq!(plan.design.idl, ["idl/robot.jidl"]);
        assert_eq!(plan.compile, ["idl/robot.jidl"]);
        assert!(plan.uses_juiz_std);
        assert_eq!(plan.kind_label(), "from idl/robot.jidl");
        let (path, doc) = plan.doc_file();
        assert_eq!(path, "juizdesign.yaml");
        assert!(doc.contains("idl:\n  - idl/robot.jidl\n"), "{doc}");
        let (_, lib) = plan.source().unwrap();
        assert!(lib.starts_with("//! my_pkg: a juiz component made from idl/robot.jidl (juiz pkg create --idl).\n"), "{lib}");
        assert!(lib.contains("typed by juizdesign.yaml"), "{lib}");
        assert!(lib.contains("include!(concat!(env!(\"OUT_DIR\"), \"/juiz_idl.rs\"));"), "{lib}");
        assert!(
            lib.contains("goal: crate::my_robot::common::Goal") && lib.contains("from_pose: ::juiz_std_types::juiz_std::geometry::Pose2D"),
            "{lib}"
        );
        let table = plan.types_table();
        assert!(table.contains("| `distance` | Process | `goal: my_robot.common.Goal`, `from_pose: juiz_std.geometry.Pose2D` | `double` | Distance \\| to the goal |"), "{table}");
        let calls = plan.call_examples();
        assert!(calls[0].contains(r#""tolerance":0.1"#) && calls[0].contains(r#""mode":"SLOW""#) && calls[0].contains(r#""theta":0.0"#), "{}", calls[0]);
        assert!(calls[1].contains("/container_process/move_by0::move_by:arm0"), "{}", calls[1]);
        assert!(calls[1].contains(r#""note":"it'\''s""#), "{}", calls[1]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn errors() {
        let d = temp("errors");
        let load = |p: &Path, lang: Lang| DesignPlan::from_idl(p, "my_pkg", lang).map(|_| ()).map_err(|e| e.to_string());
        // 5 つの言語で読める
        let p = write_jidl(&d, "a.jidl", "namespace a; process long f(long x);");
        for lang in [Lang::Rust, Lang::Python, Lang::Cpp, Lang::Haskell, Lang::Zig] {
            assert!(load(&p, lang).is_ok(), "{lang:?}");
        }
        // 無いファイル・文法の誤り
        assert!(load(&d.join("none.jidl"), Lang::Rust).unwrap_err().contains("no such file"));
        let p = write_jidl(&d, "bad.jidl", "namespace a; process long f(");
        assert!(load(&p, Lang::Rust).unwrap_err().contains("bad.jidl"));
        // 宣言が無い・名前空間が 2 つ
        let p = write_jidl(&d, "types.jidl", "namespace a; record R { long x; }");
        assert!(load(&p, Lang::Rust).unwrap_err().contains("no process or container"));
        write_jidl(&d, "other.jidl", "namespace b; process long g();");
        let p = write_jidl(&d, "two.jidl", "namespace a; import idl \"other.jidl\"; process long f();");
        let e = load(&p, Lang::Rust).unwrap_err();
        assert!(e.contains("a, b"), "{e}");
        // ディレクトリの外の import
        write_jidl(&d, "outside.jidl", "namespace o; record O { long x; }");
        let p = write_jidl(&d.join("sub"), "in.jidl", "namespace i; import idl \"../outside.jidl\"; process o.O f();");
        let e = load(&p, Lang::Rust).unwrap_err();
        assert!(e.contains("not in the directory"), "{e}");
        // その言語のキーワード (文書の検査 N6 で見つかる。Haskell は関数名に _process などを付けるので書ける)
        let p = write_jidl(&d, "kw.jidl", "namespace a; process long `loop`(long x);");
        let e = load(&p, Lang::Rust).unwrap_err();
        assert!(e.starts_with(&format!("--idl {}: processes[0]", p.display())) && e.contains("keyword"), "{e}");
        let p = write_jidl(&d, "kwpy.jidl", "namespace a; process long `lambda`(long x);");
        assert!(load(&p, Lang::Python).unwrap_err().contains("Python keyword"));
        let p = write_jidl(&d, "kwcpp.jidl", "namespace a; process long `delete`(long x);");
        assert!(load(&p, Lang::Cpp).unwrap_err().contains("C++ keyword"));
        assert!(load(&p, Lang::Haskell).is_ok());
        // Python: 既定値のある引数の後ろに、既定値の無い record の引数は書けない
        let p = write_jidl(&d, "order.jidl", "namespace o; record R { long x; } process long f(long a = 1, o.R r);");
        let e = load(&p, Lang::Python).unwrap_err();
        assert!(e.contains("argument r has no default but comes after a"), "{e}");
        assert!(load(&p, Lang::Cpp).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn no_juiz_std() {
        let d = temp("nostd");
        let p = write_jidl(&d, "a.jidl", "namespace a; process long f(long x);");
        let plan = DesignPlan::from_idl(&p, "my_pkg", Lang::Rust).unwrap();
        assert!(!plan.uses_juiz_std);
        assert!(plan.extern_line().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
