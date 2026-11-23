//! 設計文書のテスト (docs/spec/design.md §12 の Step 1)

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::*;
use crate::{Lang, Template};

const LANGS: [Lang; 5] = [Lang::Rust, Lang::Python, Lang::Cpp, Lang::Haskell, Lang::Zig];
const TEMPLATES: [Template; 3] = [Template::Component, Template::Process, Template::Container];

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("juiz_pkg_design_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 読んで検査する。読めなければその Report
fn check_text(text: &str, lang: Option<Lang>, base: &Path) -> Report {
    match parse(text) {
        Ok(d) => check(&d, lang, base),
        Err(r) => r,
    }
}

/// エラーの (道筋, 行) と文
fn errors(text: &str, lang: Option<Lang>) -> Vec<(String, Option<usize>, String)> {
    check_text(text, lang, Path::new("."))
        .errors
        .into_iter()
        .map(|d| (d.path, d.pos.map(|p| p.line), d.message))
        .collect()
}

/// エラーがちょうど 1 つで、道筋が `path`、文に `needle` を含む
#[track_caller]
fn one_error(text: &str, lang: Option<Lang>, path: &str, needle: &str) -> Option<usize> {
    let e = errors(text, lang);
    assert_eq!(e.len(), 1, "{text}\n{e:#?}");
    assert_eq!(e[0].0, path, "{e:#?}");
    assert!(e[0].2.contains(needle), "'{needle}' is not in: {}", e[0].2);
    e[0].1
}

/// 型を 1 つだけ持つ文書 (Process `p`、引数は `args` の flow 形式の行)
fn process_doc(args: &[&str], returns: &str) -> String {
    let mut s = String::from("format: 1\ncomponent:\n  name: my_robot\nprocesses:\n  - name: p\n");
    if !args.is_empty() {
        s.push_str("    arguments:\n");
        for a in args {
            s.push_str(&format!("      - {a}\n"));
        }
    }
    if !returns.is_empty() {
        s.push_str(&format!("    returns: {returns}\n"));
    }
    s
}

/// Container `c` だけを持つ文書
fn container_doc(args: &[&str], state: &[&str]) -> String {
    let mut s = String::from("format: 1\ncomponent:\n  name: my_robot\ncontainers:\n  - name: c\n");
    if !args.is_empty() {
        s.push_str("    arguments:\n");
        for a in args {
            s.push_str(&format!("      - {a}\n"));
        }
    }
    if !state.is_empty() {
        s.push_str("    state:\n");
        for a in state {
            s.push_str(&format!("      - {a}\n"));
        }
    }
    s
}

/// §3.1 の 2 つめの例
const ROVER: &str = r#"format: 1
component:
  name: rover
  description: "Drives a two-wheeled rover."
  language: rust
idl:
  - idl/rover.jidl                 # namespace rover.types; record Velocity { double v; double w; }
  - juiz_std/geometry.jidl         # built-in types
processes:
  - name: rover_stop_command
    returns: rover.types.Velocity
containers:
  - name: rover_base
    arguments:
      - { name: wheel_base, type: double, default: 0.3, description: "distance between the wheels [m]" }
    state:
      - { name: wheel_base, type: double, init: wheel_base }
      - { name: last_command, type: "rover.types.Velocity?" }
      - { name: pose, type: juiz_std.geometry.Pose2D }
    processes:
      - name: rover_base_drive
        arguments:
          - { name: command, type: rover.types.Velocity }
        returns: juiz_std.geometry.Pose2D
      - name: rover_base_pose
        readonly: true
        returns: juiz_std.geometry.Pose2D
"#;

const ROVER_JIDL: &str = "namespace rover.types;\n\nrecord Velocity {\n  double v;\n  double w;\n}\n";

fn rover_dir(name: &str) -> PathBuf {
    let dir = temp(name);
    std::fs::create_dir_all(dir.join("idl")).unwrap();
    std::fs::write(dir.join("idl/rover.jidl"), ROVER_JIDL).unwrap();
    std::fs::write(dir.join(FILE_NAME), ROVER).unwrap();
    dir
}

fn without_positions(d: Design) -> Design {
    Design {
        positions: Vec::new(),
        ..d
    }
}

// ---- プリセットと読み書き ----

#[test]
fn presets_check_and_round_trip() {
    for t in TEMPLATES {
        let text = preset_text("my_robot", t);
        assert!(!text.contains("{{"), "{text}");
        let d = parse(&text).unwrap();
        // juiz が書いた文書は、読んで書き直すと同じ文字列になる (§9)
        assert_eq!(to_yaml(&d), text, "{}", t.dir());
        let r = check(&d, None, Path::new("."));
        assert!(r.is_ok(), "{:#?}", r.errors);
        // 言語が決まっていないので W3 だけ
        assert_eq!(r.warnings.len(), 1, "{:#?}", r.warnings);
        for lang in LANGS {
            let mut d = d.clone();
            d.component.language = Some(lang);
            let r = check(&d, Some(lang), Path::new("."));
            assert!(r.is_ok() && r.warnings.is_empty(), "{} {lang:?}: {:#?}", t.dir(), r);
            let written = to_yaml(&d);
            assert!(written.contains(&format!("  language: {}\n", lang.dir())));
            assert_eq!(to_yaml(&parse(&written).unwrap()), written);
        }
    }
    let d = preset("my_robot", Template::Component);
    assert_eq!(d.counts(), "component my_robot: 1 process, 1 container, 2 container processes");
    assert_eq!(d.component.description.as_deref(), Some("my_robot: a juiz component"));
    assert_eq!(d.processes[0].arguments[0].type_expr, "long");
    assert_eq!(d.processes[0].arguments[0].default, Some(json!(0)));
    let get = &d.containers[0].processes[1];
    assert_eq!((get.name.as_str(), get.readonly, get.returns.as_deref()), ("my_robot_get", true, Some("long")));
    assert_eq!(d.containers[0].state[0].init.as_deref(), Some("initial_value"));
    assert_eq!(preset("arm", Template::Process).counts(), "component arm: 1 process, 0 containers, 0 container processes");
    assert_eq!(preset("arm", Template::Container).counts(), "component arm: 0 processes, 1 container, 2 container processes");
    // 1 行目は JSON Schema の場所 (その版のタグ)
    assert!(preset_text("arm", Template::Process).starts_with(&format!(
        "# yaml-language-server: $schema=https://raw.githubusercontent.com/OpenJUIZ/juiz/v{}/juiz_pkg/schema/juizdesign.schema.json\n",
        crate::JUIZ_VERSION
    )));
}

#[test]
fn rover_example() {
    let dir = rover_dir("rover");
    for lang in LANGS {
        let r = check_text(ROVER, Some(lang), &dir);
        assert!(r.is_ok() && r.warnings.is_empty(), "{lang:?}: {:#?}", r);
    }
    let d = parse(ROVER).unwrap();
    assert_eq!(d.idl, vec!["idl/rover.jidl", "juiz_std/geometry.jidl"]);
    assert_eq!(d.containers[0].state[1].type_expr, "rover.types.Velocity?");
    assert_eq!(d.containers[0].arguments[0].default, Some(json!(0.3)));
    // 書き直したものを読むと同じ中身 (コメントは残らない)
    let written = to_yaml(&d);
    assert!(written.contains("      - { name: last_command, type: \"rover.types.Velocity?\" }\n"), "{written}");
    assert!(written.contains("      - { name: wheel_base, type: double, default: 0.3, description: \"distance between the wheels [m]\" }\n"), "{written}");
    assert_eq!(without_positions(parse(&written).unwrap()), without_positions(d));
    assert_eq!(to_yaml(&parse(&written).unwrap()), written);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reading_forms() {
    // 空の配列・null は省いたのと同じ、x- のキーは無視、block 形式・説明の null も読む
    let text = "format: 1\nx-tool: { a: 1 }\ncomponent:\n  name: my_robot\n  description: null\n  language: C++\n  x-note: hi\nidl: []\nprocesses:\n  - name: p\n    x-hint: 1\n    arguments:\n      - name: a\n        type: long?\n        default: null\ncontainers: null\n";
    let d = parse(text).unwrap();
    assert_eq!(d.component.language, Some(Lang::Cpp));
    assert_eq!(d.component.description, None);
    assert!(d.idl.is_empty() && d.containers.is_empty());
    assert_eq!(d.processes[0].arguments[0].type_expr, "long?");
    assert_eq!(d.processes[0].arguments[0].default, Some(Value::Null));
    assert!(check(&d, None, Path::new(".")).is_ok());
    // 位置: 行と桁
    assert_eq!(d.position("processes[0].arguments[0].type"), Some(Pos { line: 14, column: 9 }));
    // 道筋に無いものは親の位置
    assert_eq!(d.position("processes[0].arguments[0].type.x"), Some(Pos { line: 14, column: 9 }));
}

// ---- §5.1 形 ----

#[test]
fn rule_f1_f2() {
    assert!(one_error("- a\n", None, "", "top level must be a mapping").is_some());
    one_error("a: [\n", None, "", "not a YAML document");
    assert_eq!(one_error("", None, "", "empty"), None);
    one_error("component: { name: a }\n", None, "format", "'format' is required");
    let line = one_error("component: { name: a }\nformat: 2\n", None, "format", "newer");
    assert_eq!(line, Some(2));
    one_error("format: \"1\"\n", None, "format", "format must be 1");
    // 読めないときと format が違うときはそこで止める (ほかの誤りは出さない)
    one_error("format: 3\nfoo: 1\n", None, "format", "newer");
}

#[test]
fn rule_f3() {
    let line = one_error(
        "format: 1\ncomponent:\n  name: my_robot\nprocesses:\n  - name: p\n    retruns: long\n",
        None,
        "processes[0].retruns",
        "unknown key 'retruns'",
    );
    assert_eq!(line, Some(6));
    one_error(
        "format: 1\ncomponent:\n  name: my_robot\nprocesses:\n  - name: p\n    readonly: true\n",
        None,
        "processes[0].readonly",
        "only for a container process",
    );
    one_error("format: 1\ncomponent: {}\nprocesses: [{ name: p }]\n", None, "component.name", "'name' is required");
    one_error("format: 1\nprocesses: [{ name: p }]\n", None, "component", "'component' is required");
    one_error(
        "format: 1\ncomponent: { name: my_robot, language: java }\nprocesses: [{ name: p }]\n",
        None,
        "component.language",
        "unknown language",
    );
    one_error(&process_doc(&["{ name: a }"], ""), None, "processes[0].arguments[0].type", "'type' is required");
    one_error(&process_doc(&["{ name: a, type: 1 }"], ""), None, "processes[0].arguments[0].type", "must be a string");
    one_error(&process_doc(&["{ name: a, type: \"\" }"], ""), None, "processes[0].arguments[0].type", "must not be empty");
    one_error("format: 1\ncomponent: { name: my_robot }\nprocesses: { name: p }\n", None, "processes", "must be a sequence");
    one_error(
        "format: 1\ncomponent: { name: my_robot }\ncontainers:\n  - name: c\n    processes:\n      - { name: g, readonly: yes }\n",
        None,
        "containers[0].processes[0].readonly",
        "true or false",
    );
    one_error("format: 1\ncomponent: { name: my_robot }\nidl: [1]\nprocesses: [{ name: p }]\n", None, "idl[0]", "must be a string");
    // F3 の誤りはすべて並べる
    let e = errors("format: 1\ncomponent: { name: my_robot, foo: 1 }\nprocesses:\n  - { name: p, bar: 2 }\n  - { baz: 3 }\n", None);
    let paths: Vec<&str> = e.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(paths, vec!["component.foo", "processes[0].bar", "processes[1].baz", "processes[1].name"]);
}

#[test]
fn rule_f4() {
    one_error("format: 1\ncomponent:\n  name: my_robot\nprocesses: []\n", None, "", "no type");
}

// ---- §5.2 名前 ----

#[test]
fn rules_n() {
    // N1
    one_error("format: 1\ncomponent: { name: MyRobot }\nprocesses: [{ name: p }]\n", None, "component.name", "lowercase");
    one_error("format: 1\ncomponent: { name: arm_2 }\nprocesses: [{ name: p }]\n", Some(Lang::Haskell), "component.name", "Haskell");
    assert!(errors("format: 1\ncomponent: { name: arm_2 }\nprocesses: [{ name: p }]\n", Some(Lang::Rust)).is_empty());
    one_error(
        &format!("format: 1\ncomponent: {{ name: {} }}\nprocesses: [{{ name: p }}]\n", "a".repeat(33)),
        Some(Lang::Zig),
        "component.name",
        "at most 32",
    );
    // N2
    one_error("format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: my-add }]\n", None, "processes[0].name", "not a valid type name");
    one_error("format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: 1add }]\n", None, "processes[0].name", "not a valid type name");
    // N3: Process・Container・ContainerProcess を通して重ならない。component.name とは同じでよい
    one_error(
        "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: x }]\ncontainers:\n  - name: my_robot\n    processes: [{ name: x }]\n",
        None,
        "containers[0].processes[0].name",
        "already used by processes[0]",
    );
    // N4: Container arm の arm_create と Process arm_create
    one_error(
        "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: arm_create }]\ncontainers: [{ name: arm }]\n",
        None,
        "containers[0].name",
        "'arm_create'",
    );
    // Container の状態の型 <Name>・<Name>State
    one_error(
        "format: 1\ncomponent: { name: my_robot }\ncontainers: [{ name: arm }, { name: ArmState }]\n",
        None,
        "containers[1].name",
        "'ArmState'",
    );
    // N5
    one_error(&process_doc(&["{ name: a, type: long }", "{ name: a, type: long }"], ""), None, "processes[0].arguments[1].name", "written twice");
    one_error(&process_doc(&["{ name: a-b, type: long }"], ""), None, "processes[0].arguments[0].name", "not a valid argument name");
    one_error(&container_doc(&[], &["{ name: v, type: long }", "{ name: v, type: long }"]), None, "containers[0].state[1].name", "written twice");
}

#[test]
fn rule_n6() {
    // 型名・引数名がその言語のキーワード
    let doc = "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: fn }]\n";
    assert!(errors(doc, None).is_empty());
    one_error(doc, Some(Lang::Rust), "processes[0]", "Rust");
    let doc = process_doc(&["{ name: class, type: long }"], "");
    one_error(&doc, Some(Lang::Python), "processes[0]", "Python");
    assert!(errors(&doc, Some(Lang::Rust)).is_empty());
    let doc = "format: 1\ncomponent: { name: my_robot }\ncontainers:\n  - name: c\n    processes: [{ name: delete }]\n";
    one_error(doc, Some(Lang::Cpp), "containers[0].processes[0]", "C++");
    // Container の引数と状態の名前
    let doc = container_doc(&["{ name: lambda, type: long }"], &["{ name: def, type: long }"]);
    let e = errors(&doc, Some(Lang::Python));
    let paths: Vec<&str> = e.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(paths, vec!["containers[0].arguments[0].name", "containers[0].state[0].name"], "{e:#?}");
    assert!(e[1].2.contains("'def' cannot be used as a name in Python"), "{e:#?}");
    // Haskell: 型のモジュール名が Component のモジュール名と同じ
    let dir = temp("n6_haskell");
    std::fs::write(dir.join("t.jidl"), "namespace myRobot;\nrecord P { long x; }\n").unwrap();
    let doc = "format: 1\ncomponent: { name: my_robot }\nidl: [t.jidl]\nprocesses: [{ name: p, returns: myRobot.P }]\n";
    assert!(check_text(doc, Some(Lang::Rust), &dir).is_ok());
    let r = check_text(doc, Some(Lang::Haskell), &dir);
    assert_eq!(r.errors.len(), 1, "{r:#?}");
    assert_eq!(r.errors[0].path, "idl");
    assert!(r.errors[0].message.contains("MyRobot"), "{r:#?}");
    let _ = std::fs::remove_dir_all(dir);
}

// ---- §5.3 型と値 ----

#[test]
fn rules_t1_t2() {
    // T1: 読めない、知らない型
    one_error(&process_doc(&["{ name: a, type: \"array<\" }"], ""), None, "processes[0].arguments[0].type", "cannot read the type 'array<'");
    one_error(&process_doc(&["{ name: a, type: \"long; process long q\" }"], ""), None, "processes[0].arguments[0].type", "cannot read the type");
    one_error(&process_doc(&[], "Velocity"), None, "processes[0].returns", "record and enum types are read from");
    // 型の式いろいろ
    let ok = process_doc(
        &[
            "{ name: a, type: boolean }",
            "{ name: b, type: \"array<map<double>>\" }",
            "{ name: c, type: \"union { null, string }\" }",
            "{ name: d, type: any }",
            "{ name: e, type: image }",
            "{ name: f, type: bytes, default: \"aGk=\" }",
            "{ name: g, type: int }",
        ],
        "void",
    );
    assert!(errors(&ok, None).is_empty(), "{:#?}", errors(&ok, None));
    // 短い名前には完全名の候補を出す
    let dir = rover_dir("t1");
    let r = check_text(&process_doc(&[], "Velocity"), None, &dir);
    let doc = format!("format: 1\ncomponent: {{ name: my_robot }}\nidl: [idl/rover.jidl]\n{}", process_doc(&[], "Velocity").split_once("processes:").map(|(_, p)| format!("processes:{p}")).unwrap());
    assert!(!r.is_ok());
    let r = check_text(&doc, None, &dir);
    assert_eq!(r.errors.len(), 1, "{r:#?}");
    assert!(r.errors[0].message.contains("write the full name (rover.types.Velocity)"), "{r:#?}");
    let _ = std::fs::remove_dir_all(dir);
    // T2
    one_error(&process_doc(&["{ name: a, type: \"null\" }"], ""), None, "processes[0].arguments[0].type", "null alone");
    one_error(&process_doc(&["{ name: a, type: void }"], ""), None, "processes[0].arguments[0].type", "only for returns");
    one_error(&process_doc(&["{ name: a, type: \"image?\" }"], ""), None, "processes[0].arguments[0].type", "image can be used only");
    one_error(&process_doc(&[], "array<image>"), None, "processes[0].returns", "image can be used only");
    one_error(&container_doc(&["{ name: a, type: image }"], &[]), None, "containers[0].arguments[0].type", "container argument");
    one_error(&container_doc(&[], &["{ name: a, type: image }"]), None, "containers[0].state[0].type", "state");
}

#[test]
fn rules_t3_t4() {
    // T3
    one_error(&process_doc(&["{ name: a, type: long, default: \"x\" }"], ""), None, "processes[0].arguments[0].default", "does not match the type long");
    one_error(&container_doc(&[], &["{ name: s, type: string, default: 1 }"]), None, "containers[0].state[0].default", "does not match");
    one_error(&process_doc(&["{ name: a, type: \"array<double>\", default: [1.0, \"x\"] }"], ""), None, "processes[0].arguments[0].default", "does not match");
    // null は「既定値なし」、T? なら null の既定値
    assert!(errors(&process_doc(&["{ name: a, type: long, default: null }", "{ name: b, type: \"long?\", default: null }"], ""), None).is_empty());
    // T4
    one_error(&container_doc(&["{ name: a, type: long }"], &["{ name: s, type: long, init: b }"]), None, "containers[0].state[0].init", "'b' is not an argument");
    one_error(&container_doc(&["{ name: a, type: long }"], &["{ name: s, type: double, init: a }"]), None, "containers[0].state[0].init", "the argument 'a' is long, but the state 's' is double");
    one_error(&container_doc(&["{ name: a, type: long }"], &["{ name: s, type: long, init: a, default: 1 }"]), None, "containers[0].state[0].init", "not both");
    // 状態が T? で引数が T はよい
    assert!(errors(&container_doc(&["{ name: a, type: long }"], &["{ name: s, type: \"long?\", init: a }"]), None).is_empty());
}

#[test]
fn rule_t5() {
    let dir = rover_dir("t5");
    let doc = |idl: &str| format!("format: 1\ncomponent: {{ name: my_robot }}\nidl: [{idl}]\nprocesses: [{{ name: p }}]\n");
    let err = |idl: &str| {
        let r = check_text(&doc(idl), None, &dir);
        assert_eq!(r.errors.len(), 1, "{idl}: {r:#?}");
        (r.errors[0].path.clone(), r.errors[0].message.clone())
    };
    assert!(check_text(&doc("idl/rover.jidl"), None, &dir).is_ok());
    assert!(check_text(&doc("./idl/rover.jidl"), None, &dir).is_ok());
    assert!(check_text(&doc("juiz_std/geometry.jidl"), None, &dir).is_ok());
    assert!(err("../x.jidl").1.contains("without '..'"));
    assert!(err("/tmp/x.jidl").1.contains("relative"));
    assert!(err("\"idl\\\\rover.jidl\"").1.contains("separator"));
    assert!(err("x.avsc").1.contains("import schema"));
    assert!(err("x.txt").1.contains("not a .jidl"));
    assert_eq!(err("idl/none.jidl"), ("idl[0]".to_owned(), "'idl/none.jidl' is not found (relative to the directory of the document, or a built-in file such as juiz_std/geometry.jidl)".to_owned()));
    // .jidl のエラーはファイル:行:桁 のまま
    std::fs::write(dir.join("bad.jidl"), "namespace a;\nrecord R { long x }\n").unwrap();
    let (path, message) = err("bad.jidl");
    assert_eq!(path, "idl");
    assert!(message.contains("bad.jidl:2:"), "{message}");
    // 文書のディレクトリの外を import する
    let outside = temp("t5_outside");
    std::fs::write(outside.join("o.jidl"), "namespace o;\nrecord O { long x; }\n").unwrap();
    std::fs::write(dir.join("imp.jidl"), format!("namespace i;\nimport idl \"{}\";\n", outside.join("o.jidl").display())).unwrap();
    let (_, message) = err("imp.jidl");
    assert!(message.contains("outside the directory of the document"), "{message}");
    let _ = std::fs::remove_dir_all(outside);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rule_t6() {
    // Python・C++ は T?・bytes の引数を作れない
    let doc = process_doc(&["{ name: a, type: \"long?\" }"], "");
    assert!(errors(&doc, Some(Lang::Rust)).is_empty());
    one_error(&doc, Some(Lang::Python), "processes[0]", "Python");
    one_error(&doc, Some(Lang::Cpp), "processes[0]", "C++");
    let doc = process_doc(&["{ name: a, type: bytes }"], "");
    one_error(&doc, Some(Lang::Python), "processes[0]", "Python");
    // 状態の T? は引数ではないので、言語によらずよい
    assert!(errors(&container_doc(&[], &["{ name: s, type: \"long?\" }"]), Some(Lang::Python)).is_empty());
}

#[test]
fn warnings() {
    // W2
    let r = check_text(&process_doc(&["{ name: a, type: image, default: 1 }"], ""), Some(Lang::Rust), Path::new("."));
    assert!(r.is_ok(), "{r:#?}");
    assert_eq!(r.warnings.len(), 1);
    assert_eq!(r.warnings[0].path, "processes[0].arguments[0].default");
    // W3: 言語が決まらない
    let r = check_text(&process_doc(&[], ""), None, Path::new("."));
    assert_eq!(r.warnings.len(), 1);
    assert!(r.warnings[0].message.contains("-b <lang>"));
    // 言語の規則で見るのはほかにエラーが無いときだけ
    let e = errors("format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: fn, returns: nope }]\n", Some(Lang::Rust));
    assert_eq!(e.len(), 1, "{e:#?}");
}

#[test]
fn diag_format() {
    let r = check_text(
        "format: 1\ncomponent:\n  name: my_robot\nprocesses:\n  - name: p\n    arguments:\n      - { name: a, type: long, default: x }\n",
        Some(Lang::Rust),
        Path::new("."),
    );
    assert!(
        r.error_text("juizdesign.yaml")
            .starts_with("juizdesign.yaml:7:32: error: processes[0].arguments[0].default: the value does not match the type long: "),
        "{}",
        r.error_text("juizdesign.yaml")
    );
    let d = Diag { path: String::new(), pos: None, message: "m".to_owned() };
    assert_eq!(d.format("f.yaml", "warning"), "f.yaml: warning: m");
}

// ---- コマンド ----

fn design_args(args: &[&str]) -> crate::cli::DesignArgs {
    use clap::Parser;
    #[derive(Parser)]
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
fn design_command() {
    let dir = temp("design_command");
    let out = dir.join("sub/juizdesign.yaml");
    let args = design_args(&["my_robot", "-b", "python", "-t", "container", "-o", out.to_str().unwrap()]);
    assert!(args.command.is_none());
    let w = command::design("my_robot", &args).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(text, w.content);
    assert!(text.contains("  language: python\n"));
    assert!(!text.contains("processes:\n  - name: my_robot_add"));
    assert!(w.summary.starts_with(&format!("Wrote {} (component my_robot: 0 processes, 1 container, 2 container processes)", out.display())), "{}", w.summary);
    // 上書きしない
    let e = command::design("my_robot", &args).unwrap_err();
    assert!(e.to_string().contains("already exists"), "{e}");
    // 言語の名前の規則
    let args = design_args(&["arm_2", "-b", "haskell", "-o", dir.join("h.yaml").to_str().unwrap()]);
    let e = command::design("arm_2", &args).unwrap_err().to_string();
    assert!(e.contains("component.name"), "{e}");
    assert!(!dir.join("h.yaml").exists());
    // 書いた文書は check を通る
    let c = command::check_file(&crate::cli::DesignCheckArgs { file: out.clone(), lang: None }).unwrap();
    assert_eq!(c.ok_line.as_deref(), Some(format!("ok: {} (component my_robot: 0 processes, 1 container, 2 container processes)", out.display()).as_str()));
    assert!(c.lines.is_empty(), "{:?}", c.lines);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn check_command() {
    let parsed = design_args(&["check", "x.yaml", "-b", "zig"]);
    match parsed.command {
        Some(crate::cli::DesignCommand::Check(c)) => {
            assert_eq!(c.file, PathBuf::from("x.yaml"));
            assert_eq!(c.lang, Some(Lang::Zig));
        }
        other => panic!("{other:?}"),
    }
    let dir = rover_dir("check_command");
    let file = dir.join(FILE_NAME);
    let c = command::check_file(&crate::cli::DesignCheckArgs { file: file.clone(), lang: None }).unwrap();
    assert!(c.ok_line.as_deref().unwrap().ends_with("(component rover: 1 process, 1 container, 2 container processes)"));
    // エラーはすべて <file>:<行>:<桁>: error: の形で
    std::fs::write(&file, ROVER.replace("type: double, init", "type: string, init").replace("returns: juiz_std.geometry.Pose2D\n      - name", "returns: Pose2D\n      - name")).unwrap();
    let c = command::check_file(&crate::cli::DesignCheckArgs { file: file.clone(), lang: Some(Lang::Rust) }).unwrap();
    assert_eq!(c.ok_line, None);
    assert_eq!(c.lines.len(), 2, "{:#?}", c.lines);
    let f = file.display().to_string();
    assert!(c.lines[0].starts_with(&format!("{f}:17:")) && c.lines[0].contains(": error: containers[0].state[0].init: "), "{:#?}", c.lines);
    assert!(c.lines[1].starts_with(&format!("{f}:24:")) && c.lines[1].contains(": error: containers[0].processes[0].returns: unknown type 'Pose2D'; write the full name (juiz_std.geometry.Pose2D)"), "{:#?}", c.lines);
    assert!(command::check_file(&crate::cli::DesignCheckArgs { file: dir.join("none.yaml"), lang: None }).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

// ---- JSON Schema (§10) ----

/// juizdesign.schema.json が使う JSON Schema の言葉だけを解く小さな検査器
fn schema_ok(schema: &Value, root: &Value, v: &Value) -> bool {
    let s = match schema.get("$ref").and_then(Value::as_str) {
        Some(r) => {
            let name = r.strip_prefix("#/$defs/").expect("local ref");
            let mut merged = root["$defs"][name].clone();
            // $ref と並べた description などは検査に関係しない
            if let (Some(m), Some(o)) = (merged.as_object_mut(), schema.as_object()) {
                for (k, x) in o {
                    if k != "$ref" {
                        m.insert(k.clone(), x.clone());
                    }
                }
            }
            return schema_ok(&merged, root, v);
        }
        None => schema,
    };
    if let Value::Bool(b) = s {
        return *b;
    }
    let type_ok = |t: &str| match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        _ => panic!("unknown type {t}"),
    };
    match s.get("type") {
        Some(Value::String(t)) if !type_ok(t) => return false,
        Some(Value::Array(ts)) if !ts.iter().any(|t| type_ok(t.as_str().unwrap())) => return false,
        _ => {}
    }
    if let Some(c) = s.get("const") {
        if c != v {
            return false;
        }
    }
    if let Some(Value::Array(e)) = s.get("enum") {
        if !e.contains(v) {
            return false;
        }
    }
    if let Some(n) = s.get("not") {
        if schema_ok(n, root, v) {
            return false;
        }
    }
    if let Value::String(text) = v {
        if let Some(p) = s.get("pattern").and_then(Value::as_str) {
            if !regex::Regex::new(p).unwrap().is_match(text) {
                return false;
            }
        }
        if s.get("minLength").and_then(Value::as_u64).is_some_and(|n| (text.chars().count() as u64) < n) {
            return false;
        }
        if s.get("maxLength").and_then(Value::as_u64).is_some_and(|n| (text.chars().count() as u64) > n) {
            return false;
        }
    }
    if let (Value::Array(items), Some(is)) = (v, s.get("items")) {
        if !items.iter().all(|i| schema_ok(is, root, i)) {
            return false;
        }
    }
    if let Value::Object(m) = v {
        if let Some(Value::Array(req)) = s.get("required") {
            if !req.iter().all(|k| m.contains_key(k.as_str().unwrap())) {
                return false;
            }
        }
        let props = s.get("properties").and_then(Value::as_object);
        let pattern_props = s.get("patternProperties").and_then(Value::as_object);
        for (k, x) in m {
            let mut matched = false;
            if let Some(ps) = props.and_then(|p| p.get(k)) {
                matched = true;
                if !schema_ok(ps, root, x) {
                    return false;
                }
            }
            for (pat, ps) in pattern_props.into_iter().flatten() {
                if regex::Regex::new(pat).unwrap().is_match(k) {
                    matched = true;
                    if !schema_ok(ps, root, x) {
                        return false;
                    }
                }
            }
            if !matched && s.get("additionalProperties") == Some(&Value::Bool(false)) {
                return false;
            }
        }
    }
    true
}

fn schema() -> Value {
    serde_json::from_str(include_str!("../../schema/juizdesign.schema.json")).unwrap()
}

fn yaml_json(text: &str) -> Value {
    yaml::parse(text).unwrap().unwrap().to_json()
}

#[test]
fn json_schema_agrees() {
    let schema = schema();
    // 知らない言葉を使っていない (この検査器で解けるものだけ)
    fn words(v: &Value, out: &mut Vec<String>) {
        if let Value::Object(m) = v {
            for (k, x) in m {
                out.push(k.clone());
                if !matches!(k.as_str(), "properties" | "$defs" | "patternProperties") {
                    words(x, out);
                } else if let Value::Object(inner) = x {
                    for y in inner.values() {
                        words(y, out);
                    }
                }
            }
        } else if let Value::Array(a) = v {
            a.iter().for_each(|x| words(x, out));
        }
    }
    let mut w = Vec::new();
    words(&schema, &mut w);
    for k in w {
        assert!(
            [
                "$schema", "$id", "title", "description", "type", "required", "properties", "patternProperties",
                "additionalProperties", "$defs", "const", "items", "pattern", "$ref", "minLength", "maxLength", "enum", "not",
                "default"
            ]
            .contains(&k.as_str()),
            "the test validator does not know '{k}'"
        );
    }
    // プリセットと §3.1 の例は通る
    for t in TEMPLATES {
        for lang in [None, Some(Lang::Cpp)] {
            let mut d = preset("my_robot", t);
            d.component.language = lang;
            assert!(schema_ok(&schema, &schema, &yaml_json(&to_yaml(&d))), "{}", t.dir());
        }
    }
    assert!(schema_ok(&schema, &schema, &yaml_json(ROVER)));
    // F3 に反するものは JSON Schema でも落ちる (juiz_pkg でも F3 のエラー)
    let bad = [
        "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: p, retruns: long }]\n",
        "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: p, readonly: true }]\n",
        "format: 1\ncomponent: {}\nprocesses: [{ name: p }]\n",
        "format: 1\ncomponent: { name: my_robot, language: java }\nprocesses: [{ name: p }]\n",
        "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: p, arguments: [{ name: a }] }]\n",
        "format: 1\ncomponent: { name: my_robot }\nprocesses: [{ name: p, arguments: [{ name: a, type: \"\" }] }]\n",
        "format: 1\ncomponent: { name: my_robot }\nprocesses: { name: p }\n",
        "format: 1\ncomponent: { name: my_robot }\ncontainers: [{ name: c, processes: [{ name: g, readonly: 1 }] }]\n",
        "format: 1\ncomponent: { name: my_robot }\nfoo: 1\n",
        "format: 2\ncomponent: { name: my_robot }\nprocesses: [{ name: p }]\n",
        "component: { name: my_robot }\nprocesses: [{ name: p }]\n",
    ];
    for text in bad {
        assert!(!schema_ok(&schema, &schema, &yaml_json(text)), "the schema passes: {text}");
        assert!(parse(text).is_err(), "juiz_pkg passes: {text}");
    }
    // x- のキーはどちらも通す
    let x = "format: 1\nx-a: 1\ncomponent: { name: my_robot, x-b: 2 }\nprocesses: [{ name: p, x-c: 3 }]\n";
    assert!(schema_ok(&schema, &schema, &yaml_json(x)));
    assert!(parse(x).is_ok());
}
