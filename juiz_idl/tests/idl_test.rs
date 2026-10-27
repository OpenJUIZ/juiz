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
            "juiz_idl.rs",
            "a.b.skeleton.rs",
            "a/__init__.py",
            "a/b/__init__.py",
            "a.b.h",
            "juiz_idl.h",
            "A/B.hs",
            "juiz_idl.ts",
            "a.b.ts"
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

/// process / container 宣言の Rust の雛形 (仕様 §18)
#[test]
fn rust_skeleton() {
    let model = load(
        r#"namespace a.b;
        record P { long x = -1; }
        /** 説明 */
        process P make(long x = -1, string? note = null);
        container robot_arm { container_process void stop(); }"#,
    );
    let files = generate(&model, &[OutputKind::RustSkeleton]).unwrap();
    assert_eq!(files.len(), 1);
    let code = &files[0].contents;
    assert!(code.contains("pub fn make(x: i64, note: ::std::option::Option<::std::string::String>) -> JuizResult<crate::a::b::P>"), "{code}");
    assert!(code.contains("typed = \"all\""));
    assert!(
        code.contains(r#"default = "{\"note\":null,\"x\":-1}""#),
        "{code}"
    );
    assert!(code.contains("pub struct RobotArm {}"));
    assert!(
        code.contains("#[juiz_component_container_process(\n    container_type = \"robot_arm\"")
    );
    assert!(code.contains("robot_arm = [stop]"));
    assert!(code.contains("processes = [make]"));

    // 型だけで宣言のない名前空間は書き出さない。Rust のキーワードの名前はエラー
    assert!(generate(
        &load("namespace a; record R { long x; }"),
        &[OutputKind::RustSkeleton]
    )
    .unwrap()
    .is_empty());
    let keyword = load("namespace a; process long `loop`(long x);");
    let e = generate(&keyword, &[OutputKind::RustSkeleton])
        .unwrap_err()
        .to_string();
    assert!(e.contains("keyword"), "{e}");
}

/// Python の型と client stub (仕様 §20)
#[test]
fn python_code() {
    let model = load(
        r#"namespace a.b;
        record P { long x = -1; bytes data = ""; string `from` = "f"; long to_value = 0; }
        enum E { X, None, mro } = X;
        /** 作る */
        process P make(long x = -1, P base, E? e = null);
        container robot_arm { container_process void stop(P self); }"#,
    );
    let files = generate(&model, &[OutputKind::Python]).unwrap();
    let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
    assert_eq!(paths, vec!["a/__init__.py", "a/b/__init__.py"]);
    assert!(files[0].contents.starts_with("# @generated by juiz-idl"));
    let code = &files[1].contents;
    assert!(
        code.contains("@_dataclasses.dataclass(kw_only=True)\nclass P:"),
        "{code}"
    );
    // 直接書ける既定値はリテラル、それ以外は default_factory
    assert!(code.contains("    x: int = -1\n"), "{code}");
    assert!(code.contains("data: bytes = _dataclasses.field(default_factory=lambda: _jt.field_default(P, \"data\"))"), "{code}");
    // キーワードとメソッドの名前は末尾に _ (辞書のキーは元の名前)
    assert!(code.contains("    from_: str = \"f\"\n"), "{code}");
    assert!(
        code.contains("(\"to_value\", \"to_value_\", _jt.Long, 0)"),
        "{code}"
    );
    assert!(
        code.contains("    None_ = \"None\"\n    mro_ = \"mro\"\n"),
        "{code}"
    );
    // 型参照は .avsc と同じ指紋
    let fp = model.get_type("a.b.P").unwrap().fingerprint();
    assert!(
        code.contains(&format!("\"fingerprint\": \"{fp}\"")),
        "{code}"
    );
    // client stub。既定値のある引数より後ろはキーワード引数
    assert!(code.contains("class MakeClient:"), "{code}");
    assert!(
        code.contains("def __call__(self, x: int = -1, *, base: P, e: E | None = None) -> P:"),
        "{code}"
    );
    assert!(code.contains("class RobotArmClient:"), "{code}");
    assert!(code.contains("def stop(self, self_: P) -> None:"), "{code}");
    assert!(
        code.contains("(\"self\", self_, _jt.Record(lambda: P))"),
        "{code}"
    );

    // 名前空間をまたぐ参照は別名の import
    let model = load(
        r#"namespace c; record Q { long y; }
        @namespace("d") record R { c.Q q; }"#,
    );
    let files = generate(&model, &[OutputKind::Python]).unwrap();
    let d = files
        .iter()
        .find(|f| f.path.to_str() == Some("d/__init__.py"))
        .unwrap();
    assert!(d.contents.contains("import c as _ns_c\n"), "{}", d.contents);
    assert!(d.contents.contains("    q: _ns_c.Q\n"), "{}", d.contents);

    // 書き出せない名前
    let model = load("namespace a; record None { long x; }");
    assert!(generate(&model, &[OutputKind::Python]).is_err());
    let model = load("namespace a; container c { container_process void proxy(); }");
    assert!(generate(&model, &[OutputKind::Python]).is_err());
    let model = load("namespace a; record MakeClient { long x; } process MakeClient make();");
    assert!(generate(&model, &[OutputKind::Python]).is_err());
}

/// bindings/pyjuiz/test/gen は test_package/idl/geometry.jidl から作った Python のコード。
/// pyjuiz のテストと juiz_core の Python のテストが使う。生成器を変えたら
/// `JUIZ_IDL_UPDATE_PYTHON_GEN=1 cargo test -p juiz_idl --test idl_test python_gen_is_up_to_date` で作り直す
#[test]
fn python_gen_is_up_to_date() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let model = load_files(&[root.join("test_package/idl/geometry.jidl")]).unwrap();
    let files = generate(&model, &[OutputKind::Python]).unwrap();
    let gen = root.join("../bindings/pyjuiz/test/gen");
    if std::env::var_os("JUIZ_IDL_UPDATE_PYTHON_GEN").is_some() {
        let _ = std::fs::remove_dir_all(&gen);
        juiz_idl::write_files(&files, &gen).unwrap();
    }
    for f in files.iter() {
        let path = gen.join(&f.path);
        // Windows の git は改行を CRLF にして取り出すことがあるので揃えて比べる
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            on_disk == f.contents,
            "{} is out of date; regenerate with JUIZ_IDL_UPDATE_PYTHON_GEN=1",
            path.display()
        );
    }
}

#[test]
fn cpp_code() {
    let model = load(
        r#"namespace a.b;
        record Q { long y = -9223372036854775808; }
        record P { Q q; array<Q> qs = []; long? x = null; string `class` = "k\"あ"; double d = 2; }
        enum E { X, delete } = delete;"#,
    );
    let files = generate(&model, &[OutputKind::Cpp]).unwrap();
    let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
    assert_eq!(paths, vec!["a.b.h", "juiz_idl.h"]);
    let code = &files[0].contents;
    assert!(code.starts_with("// @generated by juiz-idl"));
    // 参照される Q が先に来る
    let q = code.find("struct Q {").unwrap();
    let p = code.find("struct P {").unwrap();
    assert!(q < p, "{code}");
    assert!(
        code.contains("int64_t y{(-INT64_C(9223372036854775807) - 1)};"),
        "{code}"
    );
    assert!(code.contains("::a::b::Q q{};"), "{code}");
    assert!(
        code.contains("std::optional<int64_t> x{std::nullopt};"),
        "{code}"
    );
    // キーワードの名前は末尾に _。辞書のキーは元の名前
    assert!(
        code.contains("std::string class_{\"k\\\"\\343\\201\\202\"};"),
        "{code}"
    );
    assert!(
        code.contains("m.emplace(\"class\", type_traits<std::string>::to_value(v.class_));"),
        "{code}"
    );
    assert!(code.contains("double d{2.0};"), "{code}");
    assert!(
        code.contains("enum class E {\n    X,\n    delete_,\n};"),
        "{code}"
    );
    assert!(code.contains("return ::a::b::E::delete_;"), "{code}");
    assert!(code.contains("static constexpr bool native = true;"));
    assert!(code.contains(&format!("juiz-idl-{}", env!("CARGO_PKG_VERSION"))));
    assert_eq!(files[1].contents.lines().last(), Some("#include \"a.b.h\""));

    // 名前空間をまたぐ参照はヘッダを読み込む。互いに参照するとヘッダが書けない
    let model = load(
        r#"namespace a; record A { long x; }
        @namespace("b") record B { a.A a; }"#,
    );
    let files = generate(&model, &[OutputKind::Cpp]).unwrap();
    let b = files
        .iter()
        .find(|f| f.path.to_str() == Some("b.h"))
        .unwrap();
    assert!(b.contents.contains("#include \"a.h\""), "{}", b.contents);
    let model = load(
        r#"namespace a; record A { b.B? b = null; }
        @namespace("b") record B { a.C c; }
        record C { long x; }"#,
    );
    let e = generate(&model, &[OutputKind::Cpp])
        .unwrap_err()
        .to_string();
    assert!(e.contains("refer to each other"), "{e}");
    // 型の名前にキーワードは使えない
    let e = generate(
        &load("namespace a; record union_ { long x; } @namespace(\"class\") record R { long x; }"),
        &[OutputKind::Cpp],
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("'class'"), "{e}");
}

#[test]
fn haskell_code() {
    let model = load(
        r#"namespace a.b;
        /** 点 */
        record Q { long y = -9223372036854775808; /** 名前 */ string frame_id = "k\"あ"; }
        record P { Q q; array<Q> qs = []; long? x = null; map<double?> m = {}; bytes data = ""; any extra = null; }
        enum E { X, delete } = delete;"#,
    );
    let files = generate(&model, &[OutputKind::Haskell]).unwrap();
    let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
    assert_eq!(paths, vec!["A/B.hs"]);
    let code = &files[0].contents;
    assert!(code.contains("-- @generated by juiz-idl"), "{code}");
    assert!(
        code.contains("module A.B\n  ( Q (..)\n  , defaultQ\n  , P (..)\n  , E (..)\n  ) where"),
        "{code}"
    );
    // フィールドの関数名は型名 + フィールド名 (camelCase)
    assert!(
        code.contains("  { qY :: HI.Int64\n  , qFrameId :: HT.Text\n    -- ^ 名前\n  }"),
        "{code}"
    );
    assert!(code.contains("  , pQs :: [Q]\n  , pX :: Maybe HI.Int64\n  , pM :: HM.Map HT.Text (Maybe Double)\n  , pData :: HB.ByteString\n  , pExtra :: JV.Value\n"), "{code}");
    // 既定値は辞書の表し方。文字列の ASCII 以外はエスケープする
    assert!(
        code.contains("<$> JT.field o \"y\" (Just (JV.JInt (-9223372036854775808)))"),
        "{code}"
    );
    assert!(
        code.contains("<*> JT.field o \"frame_id\" (Just (JV.JString \"k\\\"\\12354\\&\"))"),
        "{code}"
    );
    assert!(code.contains("<$> JT.field o \"q\" Nothing"), "{code}");
    // 列挙は型名 + 記号の構築子。知らない記号は既定値
    assert!(code.contains("data E\n  = EX\n  | EDelete\n"), "{code}");
    assert!(
        code.contains("[(\"X\", EX), (\"delete\", EDelete)] (Just EDelete)"),
        "{code}"
    );
    assert!(
        code.contains("instance JT.JuizType P where\n  juizType _ = typeInfoP"),
        "{code}"
    );
    // すべてのフィールドに既定値がある record だけ default<型名> を作る
    assert!(code.contains("defaultQ :: Q"), "{code}");
    assert!(!code.contains("defaultP"), "{code}");

    // 名前空間をまたぐ参照は import qualified。互いに参照するとモジュールが書けない
    let model = load(
        r#"namespace a; record A { long x; }
        @namespace("b") record B { a.A a; }"#,
    );
    let files = generate(&model, &[OutputKind::Haskell]).unwrap();
    let b = files
        .iter()
        .find(|f| f.path.to_str() == Some("B.hs"))
        .unwrap();
    assert!(
        b.contents.contains("import qualified A\n"),
        "{}",
        b.contents
    );
    assert!(b.contents.contains("bA :: A.A"), "{}", b.contents);
    let model = load(
        r#"namespace a; record A { b.B? b = null; }
        @namespace("b") record B { a.C c; }
        record C { long x; }"#,
    );
    let e = generate(&model, &[OutputKind::Haskell])
        .unwrap_err()
        .to_string();
    assert!(e.contains("refer to each other"), "{e}");
    // Haskell で同じ名前になるものはエラー
    let e = generate(
        &load("namespace a; record R { long frame_id; long frameId; }"),
        &[OutputKind::Haskell],
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("'rFrameId'"), "{e}");
    let e = generate(
        &load("namespace a; enum Maybe { X } = X;"),
        &[OutputKind::Haskell],
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("'Maybe'"), "{e}");
    let e = generate(
        &load("namespace a._b; record R { long x; }"),
        &[OutputKind::Haskell],
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("'_b'"), "{e}");
}

#[test]
fn typescript_code() {
    let model = load(
        r#"namespace a.b;
        /** 点 */
        record Q { long y = -1; /** 名前 */ string frame_id = "k\"あ"; }
        record P { Q q; array<Q?> qs = []; long? x = null; map<double> m = {}; bytes data = ""; any extra = null; E e = "X"; }
        enum E { X, delete } = delete;"#,
    );
    let files = generate(&model, &[OutputKind::TypeScript]).unwrap();
    let paths: Vec<String> = files.iter().map(|f| f.path.display().to_string()).collect();
    assert_eq!(paths, vec!["juiz_idl.ts", "a.b.ts"]);
    assert!(files[0].contents.contains("export interface TypeSchema {"));
    let code = &files[1].contents;
    assert!(code.contains("// @generated by juiz-idl"), "{code}");
    assert!(
        code.contains("import type { TypeSchema } from \"./juiz_idl.js\";\n"),
        "{code}"
    );
    assert!(
        code.contains(
            "/** 点 */\nexport interface Q {\n  y: number;\n  /** 名前 */\n  frame_id: string;\n}"
        ),
        "{code}"
    );
    assert!(code.contains("  q: Q;\n  qs: (Q | null)[];\n  x: number | null;\n  m: Record<string, number>;\n  data: string;\n  extra: unknown;\n  e: E;\n"), "{code}");
    // 既定値は辞書の表し方。すべてのフィールドに既定値がある record だけ default<型名> を作る
    assert!(
        code.contains("export function defaultQ(): Q {\n  return {\n    y: -1,\n    frame_id: \"k\\\"あ\",\n  };\n}"),
        "{code}"
    );
    assert!(!code.contains("defaultP"), "{code}");
    assert!(
        code.contains("export type E = \"X\" | \"delete\";"),
        "{code}"
    );
    assert!(
        code.contains("export const ESymbols: readonly E[] = [\"X\", \"delete\"];"),
        "{code}"
    );
    // 型参照の指紋は .avsc と同じ
    let fp = model.get_type("a.b.P").unwrap().fingerprint();
    assert!(
        code.contains(&format!(
            "export const PType: TypeSchema = {{\n  name: \"a.b.P\",\n  fingerprint: \"{fp}\","
        )),
        "{code}"
    );

    // 名前空間をまたぐ参照は import type。互いに参照してもよい
    let model = load(
        r#"namespace a; record A { b.B? b = null; }
        @namespace("b") record B { a.C? c = null; }
        record C { long x; }"#,
    );
    let files = generate(&model, &[OutputKind::TypeScript]).unwrap();
    let b = files
        .iter()
        .find(|f| f.path.to_str() == Some("b.ts"))
        .unwrap();
    assert!(
        b.contents.contains("import type * as a from \"./a.js\";\n"),
        "{}",
        b.contents
    );
    assert!(b.contents.contains("  c: a.C | null;\n"), "{}", b.contents);
    // TypeScript で使えない名前・重なる名前はエラー
    let e = generate(
        &load("namespace a; enum string { X } = X;"),
        &[OutputKind::TypeScript],
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("'string'"), "{e}");
    let e = generate(
        &load("namespace a; record ZType { long x = 0; } record defaultZ { long x; }"),
        &[OutputKind::TypeScript],
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("'defaultZType'"), "{e}");
}

#[test]
fn jidl_round_trip() {
    let model = load(
        r#"namespace a.b;
        /** 点 */
        @aliases(["OldQ"]) record Q { long y = -1; /** 名前 */ string `record` = "k\"あ"; }
        record P { Q q; array<Q?> qs = []; long? x = null; double? d = 1.5; array<union { double, null }> ds = []; map<double> m = {};
          bytes data = "AAE="; any extra = null; E e = "delete"; @aliases(["w"]) c.W? w = null; }
        enum E { X, delete } = delete;
        @namespace("c") record W { int `int` = 0; }"#,
    );
    let p = model.get_type("a.b.P").unwrap();
    let text = juiz_idl::to_jidl(p);
    assert!(
        text.starts_with("namespace a.b;\n\nrecord P {\n  Q q;\n"),
        "{text}"
    );
    assert!(
        text.contains("  double? d = 1.5;\n  array<union { double, null }> ds = [];\n"),
        "{text}"
    );
    assert!(
        text.contains("  @aliases([\"w\"]) c.W? w = null;\n"),
        "{text}"
    );
    assert!(
        text.contains("@namespace(\"c\")\nrecord W {\n  int `int` = 0;\n}"),
        "{text}"
    );
    assert!(text.contains("enum E { X, delete } = delete;"), "{text}");
    // 読み直すと同じ型 (指紋・既定値・doc・別名も同じ)
    let again = load(&text);
    assert_eq!(again.get_type("a.b.P").unwrap(), p, "{text}");
    let q = model.get_type("a.b.Q").unwrap();
    assert_eq!(load(&juiz_idl::to_jidl(q)).get_type("a.b.Q").unwrap(), q);
}
