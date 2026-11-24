//! プラグインとの突き合わせ・プラグインから文書を起こすテスト (docs/spec/design.md §12 の Step 5)
//!
//! プラグインの manifest は、文書のシグネチャから §6.1 の表のとおりに作ったもの (文書から作った雛形のプラグインの
//! manifest と同じ形) を JSON で書き換えて使う。本物のプラグインでの確かめは juiz_app の pkg_cli_test にある。

use std::path::{Path, PathBuf};

use juiz_idl::{ArgumentSignature, ProcessSignature, Signatures};
use juiz_sdk::prelude::ComponentManifest;
use serde_json::{json, Value};

use super::plugin::{compare, from_manifest, plugin_lang};
use super::*;
use crate::Lang;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("juiz_pkg_plugin_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const JIDL: &str = "namespace my_robot.types;\nimport idl \"juiz_std/geometry.jidl\";\n\n/** a goal */\nrecord Goal { juiz_std.geometry.Pose2D pose; double tolerance = 0.1; }\nenum Mode { slow, fast } = slow;\n";

/// 型を一通り使う文書 (record・enum・juiz_std・T?・既定値・readonly・Container の引数と状態)
const DOC: &str = r#"format: 1
component:
  name: my_robot
  description: "A robot."
  language: rust
idl:
  - idl/types.jidl
  - juiz_std/geometry.jidl
processes:
  - name: my_robot_go
    description: "Goes to the goal."
    arguments:
      - { name: goal, type: my_robot.types.Goal, description: "where to go" }
      - { name: mode, type: my_robot.types.Mode, default: "fast" }
      - { name: speed, type: double, default: 2 }
      - { name: note, type: "string?" }
    returns: juiz_std.geometry.Pose2D
containers:
  - name: my_robot_arm
    arguments:
      - { name: start, type: long, default: 10 }
    state:
      - { name: count, type: long, init: start }
    processes:
      - name: my_robot_arm_add
        arguments:
          - { name: step, type: long, default: 1 }
        returns: long
      - name: my_robot_arm_get
        returns: long
        readonly: true
      - name: my_robot_arm_reset
"#;

/// 文書と .jidl を置いたディレクトリ
fn doc_dir(name: &str) -> PathBuf {
    let d = temp(name);
    std::fs::create_dir_all(d.join("idl")).unwrap();
    std::fs::write(d.join("idl/types.jidl"), JIDL).unwrap();
    std::fs::write(d.join(FILE_NAME), DOC).unwrap();
    d
}

/// 読んで検査を通した文書とそのシグネチャ
fn resolved(text: &str, base: &Path) -> (Design, Signatures) {
    let d = parse(text).unwrap();
    let (r, res) = check_and_resolve(&d, d.component.language, base);
    assert!(r.is_ok(), "{}", r.error_text("doc"));
    (d, res.unwrap().signatures)
}

fn args_json(args: &[ArgumentSignature]) -> Value {
    Value::Array(
        args.iter()
            .map(|a| {
                json!({
                    "type_name": a.type_name,
                    "name": a.name,
                    "description": a.description.clone().unwrap_or_default(),
                    "default": a.default.clone().unwrap_or(Value::Null),
                    "schema": serde_json::to_value(&a.schema).unwrap(),
                })
            })
            .collect(),
    )
}

fn process_json(p: &ProcessSignature) -> Value {
    json!({
        "type_name": p.type_name,
        "description": p.description.clone().unwrap_or_default(),
        "arguments": args_json(&p.arguments),
        "language": "rust",
        "output_schema": serde_json::to_value(&p.output_schema).unwrap(),
        "readonly": p.readonly,
    })
}

/// 文書から作った雛形のプラグインの manifest (§6.1)
fn manifest_json(s: &Signatures, description: &str) -> Value {
    json!({
        "type_name": s.namespace,
        "description": description,
        "language": "rust",
        "processes": s.processes.iter().map(process_json).collect::<Vec<_>>(),
        "containers": s.containers.iter().map(|k| json!({
            "type_name": k.type_name,
            "description": k.description.clone().unwrap_or_default(),
            "language": "rust",
            "arguments": args_json(&k.arguments),
            "processes": k.container_processes.iter().map(process_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn manifest(v: &Value) -> ComponentManifest {
    serde_json::from_value(v.clone()).unwrap()
}

/// (道筋, 文) のエラーと警告
fn diags(r: &Report) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let f = |v: &Vec<Diag>| v.iter().map(|d| (d.path.clone(), d.message.clone())).collect();
    (f(&r.errors), f(&r.warnings))
}

#[test]
fn compare_matches_the_manifest_made_from_the_document() {
    let dir = doc_dir("same");
    let (d, s) = resolved(DOC, &dir);
    let r = compare(&d, &s, &manifest(&manifest_json(&s, "A robot.")));
    assert_eq!(diags(&r), (vec![], vec![]));
    // Rust のマクロは Component の説明を持てないので、プラグインが空なら比べない (§8.2)
    let r = compare(&d, &s, &manifest(&manifest_json(&s, "")));
    assert_eq!(diags(&r), (vec![], vec![]));
    // 既定値の数は値で比べる (2 と 2.0)
    let mut m = manifest_json(&s, "A robot.");
    m["processes"][0]["arguments"][2]["default"] = json!(2.0);
    assert!(compare(&d, &s, &manifest(&m)).is_ok());
}

/// 1 つだけ変えた manifest で、エラーが 1 つ (道筋, 文の一部)
#[track_caller]
fn one_error(edit: impl Fn(&mut Value), path: &str, needle: &str) {
    let dir = doc_dir("one");
    let (d, s) = resolved(DOC, &dir);
    let mut m = manifest_json(&s, "A robot.");
    edit(&mut m);
    let (e, _) = diags(&compare(&d, &s, &manifest(&m)));
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(e[0].0, path, "{e:#?}");
    assert!(e[0].1.contains(needle), "'{needle}' is not in: {}", e[0].1);
}

#[test]
fn compare_errors() {
    // P1
    one_error(|m| m["type_name"] = json!("other"), "component.name", "the plugin's is 'other'");
    // P2: 文書にだけある型・プラグインにだけある型
    one_error(
        |m| {
            m["processes"] = json!([]);
        },
        "processes[0]",
        "no process 'my_robot_go'",
    );
    one_error(
        |m| {
            let p = m["processes"][0].clone();
            let mut q = p;
            q["type_name"] = json!("my_robot_extra");
            m["processes"].as_array_mut().unwrap().push(q);
        },
        "",
        "the process 'my_robot_extra', which is not in the document",
    );
    one_error(|m| m["containers"] = json!([]), "containers[0]", "no container 'my_robot_arm'");
    one_error(
        |m| {
            m["containers"][0]["processes"].as_array_mut().unwrap().remove(2);
        },
        "containers[0].processes[2]",
        "no container process 'my_robot_arm_reset'",
    );
    one_error(
        |m| {
            let mut q = m["containers"][0]["processes"][2].clone();
            q["type_name"] = json!("my_robot_arm_more");
            m["containers"][0]["processes"].as_array_mut().unwrap().push(q);
        },
        "containers[0].processes",
        "the container process 'my_robot_arm_more', which is not in the document",
    );
    // P3: 引数
    one_error(
        |m| {
            m["processes"][0]["arguments"].as_array_mut().unwrap().remove(3);
        },
        "processes[0].arguments[3]",
        "has no argument 'note'",
    );
    one_error(
        |m| {
            let mut a = m["containers"][0]["arguments"][0].clone();
            a["name"] = json!("stop");
            m["containers"][0]["arguments"].as_array_mut().unwrap().push(a);
        },
        "containers[0].arguments",
        "the argument 'stop', which is not in the document",
    );
    let long = serde_json::to_value(juiz_types::TypeSchema::from_data_type(&juiz_types::DataType::Long).unwrap()).unwrap();
    one_error(
        |m| m["processes"][0]["arguments"][2]["schema"] = long.clone(),
        "processes[0].arguments[2].type",
        "the type is double, but the plugin's is long",
    );
    // 同じ名前で定義が違う record
    one_error(
        |m| {
            let s = &mut m["processes"][0]["arguments"][0]["schema"];
            let other = juiz_types::TypeSchema::from_avro(&json!({
                "type": "record", "name": "Goal", "namespace": "my_robot.types",
                "fields": [{ "name": "x", "type": "double" }]
            }))
            .unwrap();
            *s = serde_json::to_value(other).unwrap();
        },
        "processes[0].arguments[0].type",
        "the same name, but the definitions differ",
    );
    one_error(
        |m| m["processes"][0]["arguments"][1]["default"] = json!("slow"),
        "processes[0].arguments[1].default",
        "the default is \"fast\", but the plugin's is \"slow\"",
    );
    one_error(
        |m| m["containers"][0]["arguments"][0]["default"] = Value::Null,
        "containers[0].arguments[0].default",
        "the default is 10, but the plugin's is none",
    );
    // 型の無い引数の type_name が違う
    one_error(
        |m| {
            let a = &mut m["containers"][0]["processes"][0]["arguments"][0];
            a.as_object_mut().unwrap().remove("schema");
            a["type_name"] = json!("String");
        },
        "containers[0].processes[0].arguments[0].type",
        "has type_name String (and no schema)",
    );
    // P4
    one_error(
        |m| {
            let int = juiz_types::TypeSchema::from_data_type(&juiz_types::DataType::Int).unwrap();
            m["containers"][0]["processes"][0]["output_schema"] = serde_json::to_value(int).unwrap();
        },
        "containers[0].processes[0].returns",
        "the return type is long, but the plugin's is int",
    );
    one_error(
        |m| m["containers"][0]["processes"][2]["output_schema"] = long.clone(),
        "containers[0].processes[2].returns",
        "the return type is void, but the plugin's is long",
    );
    // P5
    one_error(
        |m| m["containers"][0]["processes"][1]["readonly"] = json!(false),
        "containers[0].processes[1].readonly",
        "readonly is true, but the plugin's is false",
    );
}

#[test]
fn compare_warnings() {
    let dir = doc_dir("warn");
    let (d, s) = resolved(DOC, &dir);
    let mut m = manifest_json(&s, "Another robot.");
    // P6: 説明
    m["processes"][0]["description"] = json!("Runs.");
    m["processes"][0]["arguments"][0]["description"] = json!("");
    // P3: 型の無い引数 (type_name だけ比べる) と並び
    m["containers"][0]["processes"][0]["arguments"][0]
        .as_object_mut()
        .unwrap()
        .remove("schema");
    m["processes"][0]["arguments"].as_array_mut().unwrap().swap(2, 3);
    // P4: 戻り値の型が無い
    m["containers"][0]["processes"][1]
        .as_object_mut()
        .unwrap()
        .remove("output_schema");
    // P7
    m["language"] = json!("python");
    // Topic は文書に書けない
    m["processes"][0]["publishes"] = json!([{ "name": "out" }]);
    let r = compare(&d, &s, &manifest(&m));
    let (e, w) = diags(&r);
    assert!(e.is_empty(), "{e:#?}");
    let paths: Vec<&str> = w.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "component.language",
            "component.description",
            "processes[0].arguments[0].description",
            "processes[0].arguments",
            "processes[0].description",
            "processes[0]",
            "containers[0].processes[0].arguments[0].type",
            "containers[0].processes[1].returns",
        ],
        "{w:#?}"
    );
    assert!(w[0].1.contains("the plugin's is python"), "{w:#?}");
    assert!(w[1].1.contains("\"Another robot.\""), "{w:#?}");
    assert!(w[2].1.contains("(none)"), "{w:#?}");
    assert!(w[3].1.contains("(goal, mode, note, speed)"), "{w:#?}");
    assert!(w[5].1.contains("Topics"), "{w:#?}");
    assert!(w[6].1.contains("only its type_name Int was compared"), "{w:#?}");
    assert!(w[7].1.contains("no type for the return value"), "{w:#?}");
    // 位置は文書のキーの位置
    assert_eq!(r.warnings[6].pos.map(|p| p.line), Some(27));
}

#[test]
fn from_manifest_round_trip() {
    let dir = doc_dir("round");
    let (d, s) = resolved(DOC, &dir);
    let m = manifest(&manifest_json(&s, "A robot."));
    let made = from_manifest(&m, None, Lang::Rust, "idl/my_robot.jidl").unwrap();
    assert!(made.warnings.is_empty(), "{:#?}", made.warnings);
    // record・enum は 1 つの .jidl に、juiz_std はファイルの名前で idl: に
    let jidl = made.jidl.clone().unwrap();
    assert!(jidl.starts_with("namespace my_robot.types;\n\nimport idl \"juiz_std/geometry.jidl\";\n"), "{jidl}");
    assert!(jidl.contains("/** a goal */\nrecord Goal {") && jidl.contains("enum Mode { slow, fast } = slow;"), "{jidl}");
    let doc = &made.design;
    assert_eq!(doc.idl, ["idl/my_robot.jidl", "juiz_std/geometry.jidl"]);
    assert_eq!(doc.component.description.as_deref(), Some("A robot."));
    assert_eq!(doc.component.language, Some(Lang::Rust));
    // 文書と同じ型になる (状態はプラグインに無い)
    let mut expected = d.clone();
    expected.positions.clear();
    expected.idl = doc.idl.clone();
    expected.containers[0].state.clear();
    // 書いた既定値 2 は manifest では 2 (辞書の表し方のまま)、`T?` の既定値なしは書かない
    assert_eq!(*doc, expected);

    // 書いた文書は検査を通り、元の manifest と突き合わせても合う
    let out = temp("round_out");
    std::fs::create_dir_all(out.join("idl")).unwrap();
    std::fs::write(out.join("idl/my_robot.jidl"), &jidl).unwrap();
    let text = to_yaml(doc);
    let (d2, s2) = resolved(&text, &out);
    assert!(compare(&d2, &s2, &m).is_ok());
    // 名前を変えて起こす
    let made = from_manifest(&m, Some("other"), Lang::Rust, "idl/other.jidl").unwrap();
    assert_eq!(made.design.component.name, "other");
}

#[test]
fn from_manifest_untyped() {
    // 型の無いプラグイン (今の多くのサンプル): type_name から型を決めて警告、戻り値は any
    let m: ComponentManifest = serde_json::from_value(json!({
        "type_name": "old_robot",
        "description": "",
        "language": "rust",
        "processes": [{
            "type_name": "old_robot_add",
            "arguments": [
                { "type_name": "Int", "name": "a", "default": 1 },
                { "type_name": "Float", "name": "b", "default": 0.5 },
                { "type_name": "Bool", "name": "c", "default": true },
                { "type_name": "String", "name": "d", "default": "x" },
                { "type_name": "Array", "name": "e", "default": [1, 2] },
                { "type_name": "Object", "name": "f", "default": { "k": 1 } },
                { "type_name": "Image", "name": "g", "default": null }
            ],
            "publishes": [{ "name": "out" }]
        }],
        "containers": [{
            "type_name": "old_robot_arm",
            "processes": [{ "type_name": "old_robot_arm_get", "readonly": true }]
        }]
    }))
    .unwrap();
    let made = from_manifest(&m, None, Lang::Rust, "idl/old_robot.jidl").unwrap();
    assert!(made.jidl.is_none());
    let d = &made.design;
    assert!(d.idl.is_empty());
    let types: Vec<&str> = d.processes[0].arguments.iter().map(|a| a.type_expr.as_str()).collect();
    assert_eq!(types, ["long", "double", "boolean", "string", "array<any>", "any", "image"]);
    assert_eq!(d.processes[0].returns.as_deref(), Some("any"));
    assert_eq!(d.processes[0].arguments[6].default, None);
    assert!(d.containers[0].processes[0].readonly);
    // 引数 7 つ + 戻り値 2 つ + Topic 1 つ
    assert_eq!(made.warnings.len(), 10, "{:#?}", made.warnings);
    assert!(made.warnings[0].starts_with("processes[0].arguments[0]: the plugin gives no type"), "{:#?}", made.warnings);
    assert!(made.warnings.iter().any(|w| w.starts_with("processes[0]: ") && w.contains("Topics")), "{:#?}", made.warnings);
    let r = check(d, Some(Lang::Rust), Path::new("."));
    assert!(r.is_ok(), "{}", r.error_text("doc"));
}

#[test]
fn cpp_container_arguments() {
    // C++ の SDK は Container の引数を manifest に出せないので、比べずに警告する
    let dir = doc_dir("cpp");
    let (d, s) = resolved(DOC, &dir);
    let mut m = manifest_json(&s, "A robot.");
    m["containers"][0]["arguments"] = json!([]);
    let r = compare(&d, &s, &manifest(&m));
    assert_eq!(r.errors.len(), 1, "{:#?}", r.errors);
    m["containers"][0]["language"] = json!("c++");
    let (e, w) = diags(&compare(&d, &s, &manifest(&m)));
    assert!(e.is_empty(), "{e:#?}");
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].0, "containers[0].arguments");
    assert!(w[0].1.contains("C++ SDK"), "{w:#?}");
    let made = from_manifest(&manifest(&m), None, Lang::Cpp, "idl/my_robot.jidl").unwrap();
    assert!(made.warnings.iter().any(|w| w.starts_with("containers[0].arguments: the C++ SDK")), "{:#?}", made.warnings);
}

#[test]
fn plugin_language() {
    assert_eq!(plugin_lang(Path::new("a.so"), Some(Lang::Cpp)).unwrap(), Lang::Cpp);
    assert_eq!(plugin_lang(Path::new("app/python/a.py"), None).unwrap(), Lang::Python);
    let e = plugin_lang(Path::new("liba.so"), None).unwrap_err().to_string();
    assert!(e.contains("give -b"), "{e}");
}

// ---- コマンド (design check --plugin・design --from-plugin) ----

/// 文書から作った manifest を返す偽の loader
fn fake_loader(m: Value) -> impl Fn(&Path, Lang) -> Result<ComponentManifest, String> {
    move |p: &Path, l: Lang| {
        assert_eq!(l, Lang::Rust, "{}", p.display());
        Ok(serde_json::from_value(m.clone()).unwrap())
    }
}

fn check_args(file: &Path, plugin: Option<&str>) -> crate::cli::DesignCheckArgs {
    crate::cli::DesignCheckArgs {
        file: file.to_path_buf(),
        lang: None,
        plugin: plugin.map(PathBuf::from),
    }
}

#[test]
fn check_command_with_plugin() {
    let dir = doc_dir("check_cmd");
    let (_, s) = resolved(DOC, &dir);
    let file = dir.join(FILE_NAME);
    let good = manifest_json(&s, "A robot.");
    let c = command::check_file(&check_args(&file, Some("libmy_robot.so")), &fake_loader(good.clone())).unwrap();
    assert!(c.lines.is_empty(), "{:#?}", c.lines);
    assert!(c.ok_line.unwrap().ends_with("matches the plugin libmy_robot.so"));

    let mut bad = good;
    bad["containers"][0]["processes"][1]["readonly"] = json!(false);
    let c = command::check_file(&check_args(&file, Some("libmy_robot.so")), &fake_loader(bad)).unwrap();
    assert!(c.ok_line.is_none());
    assert_eq!(c.lines.len(), 1, "{:#?}", c.lines);
    assert!(
        c.lines[0].contains(":31:9: error: containers[0].processes[1].readonly: readonly is true, but the plugin's is false"),
        "{:#?}",
        c.lines
    );

    // 文書が検査を通らなければプラグインは開かない
    let broken = dir.join("broken.yaml");
    std::fs::write(&broken, DOC.replace("returns: long\n        readonly", "returns: lon\n        readonly")).unwrap();
    let c = command::check_file(&check_args(&broken, Some("libmy_robot.so")), &|_: &Path, _: Lang| -> Result<ComponentManifest, String> {
        panic!("opened")
    })
    .unwrap();
    assert_eq!(c.report.errors.len(), 1);

    // juiz-pkg 単体では開けない
    let e = command::check_file(&check_args(&file, Some("libmy_robot.so")), &plugin::no_loader).unwrap_err().to_string();
    assert!(e.contains("juiz-pkg cannot open plugins"), "{e}");
}

fn design_args(args: &[&str]) -> crate::cli::DesignArgs {
    use clap::Parser;
    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        args: crate::cli::PkgArgs,
    }
    let mut v = vec!["juiz-pkg", "design"];
    v.extend_from_slice(args);
    match Cli::try_parse_from(v).unwrap().args.command {
        crate::cli::PkgCommand::Design(d) => d,
        _ => unreachable!(),
    }
}

#[test]
fn design_command_from_plugin() {
    let dir = doc_dir("from_cmd");
    let (_, s) = resolved(DOC, &dir);
    let m = manifest_json(&s, "A robot.");
    let out = temp("from_cmd_out").join("sub").join(FILE_NAME);
    let o = out.to_str().unwrap();
    let args = design_args(&["--from-plugin", "libmy_robot.so", "-b", "rust", "-o", o]);
    assert!(args.name.is_none());
    let w = command::design(None, &args, &fake_loader(m.clone())).unwrap();
    assert!(w.warnings.is_empty(), "{:#?}", w.warnings);
    assert!(w.summary.starts_with(&format!("Wrote {o} (component my_robot: 1 process, 1 container, 3 container processes) from the plugin libmy_robot.so\n")), "{}", w.summary);
    assert!(w.summary.contains("state: is empty"), "{}", w.summary);
    assert!(w.summary.contains(&format!("juiz pkg design check {o} --plugin libmy_robot.so")), "{}", w.summary);
    let jidl = out.parent().unwrap().join("idl/my_robot.jidl");
    assert!(jidl.is_file());
    assert_eq!(std::fs::read_to_string(&out).unwrap(), w.content);
    // 書いた文書を check --plugin すると合う
    let c = command::check_file(&check_args(&out, Some("libmy_robot.so")), &fake_loader(m.clone())).unwrap();
    assert!(c.ok_line.is_some(), "{:#?}", c.lines);

    // 上書きしない (文書も .jidl も)
    let e = command::design(None, &args, &fake_loader(m.clone())).unwrap_err().to_string();
    assert!(e.contains("already exists"), "{e}");
    let out2 = out.parent().unwrap().join("again.yaml");
    let args = design_args(&["--from-plugin", "libmy_robot.so", "-b", "rust", "-o", out2.to_str().unwrap()]);
    let e = command::design(None, &args, &fake_loader(m.clone())).unwrap_err().to_string();
    assert!(e.contains("my_robot.jidl already exists"), "{e}");
    assert!(!out2.exists());

    // 検査を通らない (Component の名前が使えない) ときは .jidl も消し、名前を渡すよう案内する
    let mut bad = m.clone();
    bad["type_name"] = json!("My-Robot");
    let out3 = temp("from_cmd_bad").join(FILE_NAME);
    let args = design_args(&["--from-plugin", "libmy_robot.so", "-b", "rust", "-o", out3.to_str().unwrap()]);
    let e = command::design(None, &args, &fake_loader(bad.clone())).unwrap_err().to_string();
    assert!(e.contains("component.name") && e.contains("juiz pkg design <name> --from-plugin"), "{e}");
    assert!(!out3.exists() && !out3.parent().unwrap().join("idl").exists());
    // 名前を渡せば通る
    let w = command::design(Some("my_robot"), &args, &fake_loader(bad)).unwrap();
    assert!(w.content.contains("  name: my_robot\n"), "{}", w.content);

    // -b が無く .py でもなければエラー、-t・--idl とは一緒に使えない
    let out4 = out3.with_file_name("other.yaml");
    let args = design_args(&["--from-plugin", "libmy_robot.so", "-o", out4.to_str().unwrap()]);
    let e = command::design(None, &args, &fake_loader(m)).unwrap_err().to_string();
    assert!(e.contains("give -b"), "{e}");
    use clap::Parser;
    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        args: crate::cli::PkgArgs,
    }
    assert!(Cli::try_parse_from(["juiz-pkg", "design", "--from-plugin", "a.so", "-t", "process"]).is_err());
    assert!(Cli::try_parse_from(["juiz-pkg", "design", "--from-plugin", "a.so", "--idl", "a.jidl"]).is_err());
    assert!(Cli::try_parse_from(["juiz-pkg", "design", "-b", "rust"]).is_err());
}
