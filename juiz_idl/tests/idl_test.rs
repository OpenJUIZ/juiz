//! juiz-idl のパーサ・名前の解決・書き出しのテスト (issue #191)

use juiz_idl::{generate, load_files, load_str, IdlError, OutputKind};
use juiz_types::{DataType, RecordField, RecordType, TypeSchema};
use serde_json::{json, Value};

fn load(src: &str) -> juiz_idl::IdlModel {
    load_str(src, "test.jidl").unwrap()
}

fn load_err(src: &str) -> String {
    load_str(src, "test.jidl").unwrap_err().to_string()
}

#[test]
fn fingerprint_matches_juiz_types_builder() {
    let model = load(
        r#"
        namespace robot.geometry;
        /** 平面上の点 */
        record Point2D { double x; double y; }
        record Pose2D {
          Point2D position;
          double theta = 0.0;
          string frame = "map";
          @aliases(["note"]) string? comment = null;
        }
        "#,
    );
    let point = DataType::Record(
        RecordType::new("robot.geometry", "Point2D")
            .doc("平面上の点")
            .field(RecordField::new("x", DataType::Double))
            .field(RecordField::new("y", DataType::Double)),
    );
    let pose = DataType::Record(
        RecordType::new("robot.geometry", "Pose2D")
            .field(RecordField::new("position", point))
            .field(
                RecordField::new("theta", DataType::Double)
                    .default_value(json!(0.0))
                    .unwrap(),
            )
            .field(
                RecordField::new("frame", DataType::String)
                    .default_value(json!("map"))
                    .unwrap(),
            )
            .field(
                RecordField::new("comment", DataType::optional(DataType::String))
                    .alias("note")
                    .default_value(Value::Null)
                    .unwrap(),
            ),
    );
    assert_eq!(model.get_type("robot.geometry.Pose2D"), Some(&pose));
    assert_eq!(model.types.len(), 2);
}

#[test]
fn declarations_can_refer_to_later_types() {
    let model = load(
        "namespace a; record A { B b; C c = \"Y\"; } record B { long x = 1; } enum C { X, Y };",
    );
    let a = model.get_type("a.A").unwrap();
    assert_eq!(
        a.canonical_form(),
        r#"{"name":"a.A","type":"record","fields":[{"name":"b","type":{"name":"a.B","type":"record","fields":[{"name":"x","type":"long"}]}},{"name":"c","type":{"name":"a.C","type":"enum","symbols":["X","Y"]}}]}"#
    );
}

#[test]
fn optional_order_follows_default() {
    let model = load(
        r#"namespace a;
        record R {
          long? none;
          long? n = null;
          long? three = 3;
          union { long, null } explicit;
        }"#,
    );
    let avro = model.get_type("a.R").unwrap().to_avro().unwrap();
    let types: Vec<&Value> = avro["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| &f["type"])
        .collect();
    assert_eq!(
        types,
        vec![
            &json!(["null", "long"]),
            &json!(["null", "long"]),
            &json!(["long", "null"]),
            &json!(["long", "null"])
        ]
    );
}

#[test]
fn bytes_default_is_base64_in_jidl_and_latin1_in_avsc() {
    let model = load(r#"namespace a; record R { bytes b = "AAEC"; }"#);
    let avro = model.get_type("a.R").unwrap().to_avro().unwrap();
    assert_eq!(avro["fields"][0]["default"], json!("\u{0}\u{1}\u{2}"));
}

#[test]
fn doc_comments_and_backquoted_names() {
    let model = load(
        r#"namespace a;
        /**
         * 複数行の
         * 説明
         */
        record R {
          /** フィールドの説明 */ long `record`;
          // ただのコメント
          /* ブロックコメント */ long y;
        }
        enum `E` { `enum`, B };"#,
    );
    let r = match model.get_type("a.R").unwrap() {
        DataType::Record(r) => r.clone(),
        _ => unreachable!(),
    };
    assert_eq!(r.doc.as_deref(), Some("複数行の\n説明"));
    assert_eq!(r.fields[0].name, "record");
    assert_eq!(r.fields[0].doc.as_deref(), Some("フィールドの説明"));
    assert_eq!(r.fields[1].doc, None);
    assert!(model.get_type("a.E").is_some());
}

#[test]
fn namespace_annotation_changes_type_namespace() {
    let model =
        load(r#"namespace a; @namespace("b.c") record R { long x; } record S { b.c.R r; }"#);
    assert!(model.get_type("b.c.R").is_some());
    assert!(model.get_type("a.S").is_some());
}

#[test]
fn process_and_container_signatures() {
    let model = load(
        r#"namespace a;
        record P { double x = 0.0; }
        /** 動く */
        process P move_to(/** 目標 */ P target, double speed = 1.0, P? hint = null);
        process image gray(image img = null);
        process void tick();
        container motor {
          /** モードを変える */
          container_process boolean set_mode(string mode = "idle");
        }"#,
    );
    assert_eq!(model.signatures.len(), 1);
    let s = serde_json::to_value(&model.signatures[0]).unwrap();
    let p = TypeSchema::from_data_type(model.get_type("a.P").unwrap()).unwrap();
    assert_eq!(
        s["processes"][0],
        json!({
            "type_name": "move_to",
            "description": "動く",
            "arguments": [
                {"name": "target", "type_name": "Object", "description": "目標", "schema": p},
                {"name": "speed", "type_name": "Float", "schema": TypeSchema::from_data_type(&DataType::Double).unwrap(), "default": 1.0},
                {"name": "hint", "type_name": "Object", "schema": TypeSchema::from_data_type(&DataType::optional(model.get_type("a.P").unwrap().clone())).unwrap(), "default": null},
            ],
            "output_schema": p,
        })
    );
    assert_eq!(s["processes"][1]["arguments"][0]["type_name"], "Image");
    assert_eq!(s["processes"][1]["output_schema"]["name"], "image");
    assert_eq!(s["processes"][2]["output_schema"]["name"], "null");
    assert_eq!(s["containers"][0]["type_name"], "motor");
    assert_eq!(
        s["containers"][0]["container_processes"][0]["description"],
        "モードを変える"
    );
}

#[test]
fn errors_have_positions() {
    let e = load_str("namespace a;\nrecord R {\n  long x\n}", "geo.jidl").unwrap_err();
    assert_eq!(
        e,
        IdlError::Syntax {
            file: "geo.jidl".to_owned(),
            line: 4,
            column: 1,
            message: "expected ';' but found '}'".to_owned(),
        }
    );
    assert!(e.to_string().starts_with("geo.jidl:4:1: syntax error"));
}

#[test]
fn invalid_declarations_are_errors() {
    let cases = [
        ("record R { long x; }", "expected 'namespace'"),
        ("namespace juiz.x; record R { long x; }", "reserved"),
        (
            "namespace a; record R { Unknown u; }",
            "unknown type 'Unknown'",
        ),
        (
            "namespace a; record R { S s; } record S { R r; }",
            "recursive",
        ),
        ("namespace a; record R { R r; }", "recursive"),
        ("namespace a; record R { image i; }", "image"),
        ("namespace a; record R { array<image> i; }", "image"),
        (
            "namespace a; record R { long x; long x; }",
            "declared twice",
        ),
        (
            "namespace a; record R { long x; } enum R { A };",
            "declared twice",
        ),
        (
            "namespace a; record R { long x = \"s\"; }",
            "Value does not match",
        ),
        (
            "namespace a; record R { union { null, long } x = 1; }",
            "first type",
        ),
        (
            "namespace a; record R { union { long, string } x; }",
            "only a union of null",
        ),
        ("namespace a; record R { long?? x; }", "expected a name"),
        ("namespace a; enum E { A, B } = C;", "not a symbol"),
        ("namespace a; enum E { A, A };", "declared twice"),
        ("namespace a; process long f(null n);", "null alone"),
        (
            "namespace a; process long f(long n, long n);",
            "declared twice",
        ),
        (
            "namespace a; process long f(long n = \"x\");",
            "Value does not match",
        ),
        ("namespace a; process long f(void v);", "void"),
        ("namespace a; process image? f();", "image"),
        (
            "namespace a; @doc(\"x\") record R { long x; }",
            "unknown annotation",
        ),
        (
            "namespace a; @aliases([\"x\"]) process long f();",
            "annotations cannot be used",
        ),
        (
            "namespace a; record R { @namespace(\"b\") long x; }",
            "only @aliases",
        ),
        ("namespace a; record R { bytes b = \"!!\"; }", "base64"),
        (
            "namespace a; import idl \"x.jidl\"; record R { long x; }",
            "import cannot be used",
        ),
        (
            "namespace a; record R { long x; } namespace b;",
            "only one namespace",
        ),
        (
            "namespace a; record R { long x; } /* 閉じない",
            "unterminated comment",
        ),
    ];
    for (src, expected) in cases.iter() {
        let message = load_err(src);
        assert!(
            message.contains(expected),
            "{src}\n  => {message}\n  expected to contain: {expected}"
        );
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("juiz_idl_test_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn imports_and_avsc_inputs() {
    let dir = temp_dir("imports");
    std::fs::write(
        dir.join("stamp.avsc"),
        r#"{"type":"record","name":"Stamp","namespace":"c","fields":[{"name":"sec","type":"long","default":0}]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("common.jidl"),
        "namespace c;\nimport schema \"stamp.avsc\";\nrecord H { Stamp s; }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.jidl"),
        "namespace m;\nimport idl \"common.jidl\";\nrecord R { c.H h; c.Stamp s; }\n",
    )
    .unwrap();
    let model = load_files(&[dir.join("main.jidl")]).unwrap();
    let names: Vec<String> = model.types.iter().map(|t| t.display_name()).collect();
    assert_eq!(names, vec!["c.Stamp", "c.H", "m.R"]);
    assert_eq!(model.files.len(), 3);

    // .avsc を直接入力にしてもよい (仕様 §9.4)
    let model = load_files(&[dir.join("stamp.avsc")]).unwrap();
    assert_eq!(model.types.len(), 1);

    // import の循環
    std::fs::write(dir.join("x.jidl"), "namespace x;\nimport idl \"y.jidl\";\n").unwrap();
    std::fs::write(dir.join("y.jidl"), "namespace y;\nimport idl \"x.jidl\";\n").unwrap();
    let e = load_files(&[dir.join("x.jidl")]).unwrap_err().to_string();
    assert!(e.contains("circular"), "{e}");

    // 同じ完全名で中身の違う型
    std::fs::write(
        dir.join("dup.jidl"),
        "namespace c;\nimport idl \"common.jidl\";\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("dup2.jidl"),
        "namespace c;\nrecord H { long other; }\n",
    )
    .unwrap();
    let e = load_files(&[dir.join("dup.jidl"), dir.join("dup2.jidl")])
        .unwrap_err()
        .to_string();
    assert!(e.contains("defined twice"), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn generated_files() {
    let model = load(
        r#"namespace a.b;
        /** 点 */
        record P { double x = 0.0; long? n; bytes data = ""; map<array<int>> m = {}; any extra = null; }
        enum E { X, Y } = X;
        process P f(P p);"#,
    );
    let files = generate(&model, &OutputKind::ALL).unwrap();
    let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
    assert_eq!(
        paths,
        vec![
            "a.b.P.avsc",
            "a.b.E.avsc",
            "a.b.P.schema.json",
            "a.b.E.schema.json",
            "a.b.processes.json",
            "a.b.rs",
            "juiz_idl.rs"
        ]
    );
    // .avsc は読み直すと同じ型になる
    let avsc: Value = serde_json::from_str(&files[0].contents).unwrap();
    assert_eq!(
        &DataType::from_avro(&avsc).unwrap(),
        model.get_type("a.b.P").unwrap()
    );
    assert!(files[0]
        .contents
        .starts_with("{\n  \"type\": \"record\",\n  \"name\": \"P\""));
    // JSON Schema
    let schema: Value = serde_json::from_str(&files[2].contents).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["title"], "a.b.P");
    assert_eq!(schema["description"], "点");
    assert_eq!(schema["required"], json!(["n"]));
    assert_eq!(
        schema["properties"]["n"],
        json!({"anyOf": [{"type": "integer", "minimum": i64::MIN, "maximum": i64::MAX}, {"type": "null"}]})
    );
    assert_eq!(
        schema["properties"]["data"],
        json!({"type": "string", "contentEncoding": "base64", "default": ""})
    );
    assert_eq!(
        schema["properties"]["m"]["additionalProperties"]["items"]["maximum"],
        i32::MAX
    );
    assert_eq!(schema["properties"]["extra"], json!({"default": null}));
    assert_eq!(
        schema["x-juiz-fingerprint"],
        json!(model.get_type("a.b.P").unwrap().fingerprint())
    );
    let e: Value = serde_json::from_str(&files[3].contents).unwrap();
    assert_eq!(e["enum"], json!(["X", "Y"]));
    assert_eq!(e["default"], "X");
    // Rust
    assert!(files[5].contents.contains("pub struct P {"));
    assert!(files[5].contents.contains("pub enum E {"));
    assert!(files[6].contents.contains("pub mod a {"));
    assert!(files[6].contents.contains("include!(\"a.b.rs\");"));
}

#[test]
fn cli_writes_files_and_checks() {
    let dir = temp_dir("cli");
    let input = dir.join("t.jidl");
    std::fs::write(
        &input,
        "namespace t;\nrecord R { long x; }\nprocess R f();\n",
    )
    .unwrap();
    let out = dir.join("out");
    let args = juiz_idl::cli::IdlArgs {
        files: vec![input.clone()],
        out_dir: out.clone(),
        generators: vec![],
        check: false,
    };
    juiz_idl::cli::run(&args).unwrap();
    assert!(out.join("t.R.avsc").exists());
    assert!(out.join("t.R.schema.json").exists());
    assert!(out.join("t.processes.json").exists());
    assert!(!out.join("t.rs").exists());
    let args = juiz_idl::cli::IdlArgs {
        generators: vec![juiz_idl::cli::Generator::Rust],
        ..args
    };
    juiz_idl::cli::run(&args).unwrap();
    assert!(out.join("t.rs").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
