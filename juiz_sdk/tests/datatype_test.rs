//! データ型 (issue #190、仕様 docs/spec/datatype.md §3〜§8) のテスト

use juiz_sdk::prelude::*;
use juiz_sdk::types::*;

const POSE2D: &str = r#"{
  "type": "record", "name": "Pose2D", "namespace": "robot.geometry",
  "doc": "姿勢",
  "fields": [
    {"name": "position", "type": {"type": "record", "name": "Point2D", "fields": [
      {"name": "x", "type": "double"}, {"name": "y", "type": "double"}]}},
    {"name": "theta", "type": "double", "default": 0.0},
    {"name": "frame", "type": "string", "default": "map"},
    {"name": "comment", "type": ["null", "string"], "default": null, "aliases": ["note"]},
    {"name": "goal", "type": "Point2D"}
  ]
}"#;

fn schema(avro: &str) -> TypeSchema {
    TypeSchema::from_avro_str(avro).unwrap()
}

fn data_type(avro: &str) -> DataType {
    DataType::from_avro_str(avro).unwrap()
}

fn error_of<T: std::fmt::Debug>(r: JuizResult<T>) -> JuizError {
    r.unwrap_err().downcast::<JuizError>().unwrap()
}

fn assert_schema_invalid(avro: &str) {
    match error_of(DataType::from_avro_str(avro)) {
        JuizError::TypeSchemaInvalidError { .. } => {}
        e => panic!("unexpected error {e:?} for {avro}"),
    }
}

// ---- §5 指紋 ----

/// PCF は Avro の仕様どおりでなければならない。
///
/// 期待値は Avro の Rust 実装 (apache-avro 0.19 の `Schema::canonical_form()`) で作ったもの。
/// apache-avro は serde_json の `arbitrary_precision` を有効にし、Value の形が変わって
/// プラグインと食い違うので、dev-dependencies には入れない。
#[test]
fn canonical_form_matches_apache_avro() {
    let cases = [
        (
            POSE2D,
            r#"{"name":"robot.geometry.Pose2D","type":"record","fields":[{"name":"position","type":{"name":"robot.geometry.Point2D","type":"record","fields":[{"name":"x","type":"double"},{"name":"y","type":"double"}]}},{"name":"theta","type":"double"},{"name":"frame","type":"string"},{"name":"comment","type":["null","string"]},{"name":"goal","type":"robot.geometry.Point2D"}]}"#,
        ),
        (r#""long""#, r#""long""#),
        (
            r#"{"type": "array", "items": {"type": "map", "values": ["null", "bytes"]}}"#,
            r#"{"type":"array","items":{"type":"map","values":["null","bytes"]}}"#,
        ),
        (
            r#"{"type": "enum", "name": "MotorMode", "namespace": "robot", "symbols": ["Idle", "Velocity"], "default": "Idle", "doc": "mode"}"#,
            r#"{"name":"robot.MotorMode","type":"enum","symbols":["Idle","Velocity"]}"#,
        ),
        (
            r#"{"type": "record", "name": "a.b.R", "fields": [
            {"name": "e", "type": ["long", "null"], "default": 3},
            {"name": "any1", "type": {"type": "record", "name": "Any", "namespace": "juiz", "fields": []}},
            {"name": "any2", "type": "juiz.Any"},
            {"name": "m", "type": {"type": "enum", "name": "M", "symbols": ["A"]}},
            {"name": "ms", "type": {"type": "array", "items": "M"}}
        ]}"#,
            r#"{"name":"a.b.R","type":"record","fields":[{"name":"e","type":["long","null"]},{"name":"any1","type":{"name":"juiz.Any","type":"record","fields":[]}},{"name":"any2","type":"juiz.Any"},{"name":"m","type":{"name":"a.b.M","type":"enum","symbols":["A"]}},{"name":"ms","type":{"type":"array","items":"a.b.M"}}]}"#,
        ),
    ];
    for (schema, expected) in cases.iter() {
        assert_eq!(
            data_type(schema).canonical_form(),
            *expected,
            "PCF differs for {schema}"
        );
    }
}

#[test]
fn fingerprint_is_sha256_of_canonical_form() {
    let t = data_type(r#""long""#);
    assert_eq!(t.canonical_form(), r#""long""#);
    // printf '%s' '"long"' | sha256sum
    assert_eq!(
        t.fingerprint(),
        "sha256:c32c497df6730c97fa07362aa5023f37d49a027ec452360778114cf427965add"
    );
    assert_eq!(fingerprint_of_canonical_form("\"long\""), t.fingerprint());
}

#[test]
fn fingerprint_ignores_doc_default_aliases() {
    let a = data_type(POSE2D);
    let b = data_type(
        &POSE2D
            .replace("\"default\": 0.0", "\"default\": 1.5")
            .replace("\"doc\": \"姿勢\",", "")
            .replace(", \"aliases\": [\"note\"]", ""),
    );
    assert_eq!(a.fingerprint(), b.fingerprint());
    // フィールドの順番や名前空間を変えると変わる
    let c = data_type(&POSE2D.replace("robot.geometry", "robot.geo"));
    assert_ne!(a.fingerprint(), c.fingerprint());
}

#[test]
fn optional_order_is_part_of_fingerprint() {
    let a = data_type(r#"["null", "long"]"#);
    let b = data_type(r#"["long", "null"]"#);
    assert_eq!(a.display_name(), "long?");
    assert_eq!(b.display_name(), "long?");
    assert_ne!(a.fingerprint(), b.fingerprint());
}

// ---- §3 データモデル ----

#[test]
fn display_names() {
    assert_eq!(data_type(POSE2D).display_name(), "robot.geometry.Pose2D");
    assert_eq!(
        data_type(r#"{"type": "array", "items": {"type": "map", "values": "long"}}"#)
            .display_name(),
        "array<map<long>>"
    );
    assert_eq!(
        data_type(r#"{"type": "record", "name": "Any", "namespace": "juiz", "fields": []}"#),
        DataType::Any
    );
    assert_eq!(
        data_type(r#"{"type": "record", "name": "juiz.Image", "fields": []}"#),
        DataType::Image
    );
}

#[test]
fn unsupported_schemas_are_errors() {
    assert_schema_invalid(r#"{"type": "fixed", "name": "F", "size": 4}"#);
    assert_schema_invalid(r#"["null", "long", "string"]"#);
    assert_schema_invalid(r#"["long", "string"]"#);
    assert_schema_invalid(r#"{"type": "long", "logicalType": "timestamp-millis"}"#);
    assert_schema_invalid(r#"{"type": "error", "name": "E", "fields": []}"#);
    // 予約名
    assert_schema_invalid(
        r#"{"type": "record", "name": "Foo", "namespace": "juiz", "fields": []}"#,
    );
    assert_schema_invalid(r#"{"type": "record", "name": "juiz.x.Foo", "fields": []}"#);
    assert_schema_invalid(
        r#"{"type": "record", "name": "Any", "namespace": "juiz", "fields": [{"name": "a", "type": "long"}]}"#,
    );
    // 再帰
    assert_schema_invalid(
        r#"{"type": "record", "name": "Node", "fields": [{"name": "next", "type": ["null", "Node"]}]}"#,
    );
    // image はレコードのフィールド・配列の要素・image? にできない
    let image = r#"{"type": "record", "name": "Image", "namespace": "juiz", "fields": []}"#;
    assert_schema_invalid(&format!(
        r#"{{"type": "record", "name": "R", "fields": [{{"name": "img", "type": {image}}}]}}"#
    ));
    assert_schema_invalid(&format!(r#"{{"type": "array", "items": {image}}}"#));
    assert_schema_invalid(&format!(r#"["null", {image}]"#));
    // 名前の規則・重複
    assert_schema_invalid(
        r#"{"type": "record", "name": "R", "fields": [{"name": "a-b", "type": "long"}]}"#,
    );
    assert_schema_invalid(
        r#"{"type": "record", "name": "R", "fields": [{"name": "a", "type": "long"}, {"name": "a", "type": "long"}]}"#,
    );
    assert_schema_invalid(r#"{"type": "enum", "name": "E", "symbols": ["A", "A"]}"#);
    assert_schema_invalid(r#"{"type": "enum", "name": "E", "symbols": ["A"], "default": "B"}"#);
    assert_schema_invalid(r#""Unknown""#);
    // 既定値が型に合わない、["null",T] の既定値は null だけ
    assert_schema_invalid(
        r#"{"type": "record", "name": "R", "fields": [{"name": "a", "type": "int", "default": "x"}]}"#,
    );
    assert_schema_invalid(
        r#"{"type": "record", "name": "R", "fields": [{"name": "a", "type": "int", "default": 3000000000}]}"#,
    );
    assert_schema_invalid(
        r#"{"type": "record", "name": "R", "fields": [{"name": "a", "type": ["null", "long"], "default": 1}]}"#,
    );
}

#[test]
fn to_avro_is_self_contained_and_keeps_doc_default_aliases() {
    let t = data_type(POSE2D);
    let avro = t.to_avro().unwrap();
    assert_eq!(DataType::from_avro(&avro).unwrap(), t);
    assert_eq!(avro["doc"], jvalue!("姿勢"));
    assert_eq!(avro["fields"][1]["default"], jvalue!(0.0));
    assert_eq!(avro["fields"][3]["aliases"], jvalue!(["note"]));
    // 2 回目の Point2D は完全名で参照する
    assert_eq!(avro["fields"][4]["type"], jvalue!("robot.geometry.Point2D"));
}

#[test]
fn nested_type_in_root_namespace_keeps_its_namespace() {
    let inner =
        DataType::Record(RecordType::new("", "Inner").field(RecordField::new("a", DataType::Long)));
    let outer =
        DataType::Record(RecordType::new("robot", "Outer").field(RecordField::new("inner", inner)));
    let avro = outer.to_avro().unwrap();
    let back = DataType::from_avro(&avro).unwrap();
    assert_eq!(back, outer);
    match &back {
        DataType::Record(r) => assert_eq!(r.fields[0].data_type.display_name(), "Inner"),
        _ => panic!(),
    }
}

#[test]
fn conflicting_named_types_are_errors() {
    let a =
        DataType::Record(RecordType::new("n", "P").field(RecordField::new("x", DataType::Long)));
    let b =
        DataType::Record(RecordType::new("n", "P").field(RecordField::new("x", DataType::Double)));
    let r = DataType::Record(
        RecordType::new("n", "R")
            .field(RecordField::new("a", a))
            .field(RecordField::new("b", b)),
    );
    match error_of(r.to_avro()) {
        JuizError::TypeSchemaInvalidError { .. } => {}
        e => panic!("{e:?}"),
    }
}

#[test]
fn type_schema_validate() {
    let s = schema(POSE2D);
    assert_eq!(s.name, "robot.geometry.Pose2D");
    s.validate().unwrap();
    assert!(s.identifier().starts_with("robot.geometry.Pose2D@sha256:"));
    assert_eq!(
        s.short_identifier(),
        format!("Pose2D@{}", &s.fingerprint[7..15])
    );
    let mut broken = s.clone();
    broken.fingerprint = "sha256:00".to_owned();
    assert!(broken.validate().is_err());
    let mut renamed = s.clone();
    renamed.name = "robot.geometry.Other".to_owned();
    assert!(renamed.validate().is_err());
    // JSON に出して読み戻せる
    let json = serde_json::to_value(&s).unwrap();
    let back: TypeSchema = serde_json::from_value(json).unwrap();
    assert_eq!(back, s);
}

#[test]
fn argument_types_from_schema() {
    let cases = [
        (r#""boolean""#, ArgumentType::Bool),
        (r#""int""#, ArgumentType::Int),
        (r#""long""#, ArgumentType::Int),
        (r#""float""#, ArgumentType::Float),
        (r#""double""#, ArgumentType::Float),
        (r#""string""#, ArgumentType::String),
        (r#""bytes""#, ArgumentType::String),
        (
            r#"{"type": "enum", "name": "E", "symbols": ["A"]}"#,
            ArgumentType::String,
        ),
        (r#"{"type": "array", "items": "long"}"#, ArgumentType::Array),
        (r#"{"type": "map", "values": "long"}"#, ArgumentType::Object),
        (POSE2D, ArgumentType::Object),
        (r#""juiz.Any""#, ArgumentType::Object),
        (r#""juiz.Image""#, ArgumentType::Image),
        (r#"["null", "double"]"#, ArgumentType::Float),
    ];
    for (s, expected) in cases.iter() {
        assert_eq!(&data_type(s).argument_type(), expected, "{s}");
    }
}

// ---- §4 辞書での値 ----

fn check(avro: &str, v: Value) -> JuizResult<()> {
    schema(avro).check_value(&v)
}

#[test]
fn value_check_primitives() {
    check(r#""int""#, jvalue!(1)).unwrap();
    assert!(check(r#""int""#, jvalue!(3000000000i64)).is_err());
    assert!(check(r#""int""#, jvalue!(1.0)).is_err());
    check(r#""long""#, jvalue!(3000000000i64)).unwrap();
    assert!(check(r#""long""#, jvalue!(u64::MAX)).is_err());
    check(r#""double""#, jvalue!(1)).unwrap();
    check(r#""float""#, jvalue!(1.5)).unwrap();
    assert!(check(r#""float""#, jvalue!(1e300)).is_err());
    check(r#""bytes""#, jvalue!("AAEC")).unwrap();
    assert!(check(r#""bytes""#, jvalue!("not base64!")).is_err());
    check(r#""juiz.Any""#, jvalue!({"anything": [1, "x"]})).unwrap();
    assert!(check(r#""juiz.Image""#, jvalue!({})).is_err());
    check(r#"["null", "long"]"#, jvalue!(null)).unwrap();
    check(r#"["null", "long"]"#, jvalue!(1)).unwrap();
    assert!(check(r#""long""#, jvalue!(null)).is_err());
}

#[test]
fn value_check_records() {
    let t = data_type(POSE2D);
    let full = read_value(
        &t,
        &jvalue!({"position": {"x": 1.0, "y": 2.0}, "goal": {"x": 0, "y": 0}, "comment": null, "extra": 1}),
        "Pose2D",
    )
    .unwrap();
    // 足りないフィールドは既定値で埋め、知らないキーは落とす
    assert_eq!(
        full,
        jvalue!({"position": {"x": 1.0, "y": 2.0}, "theta": 0.0, "frame": "map", "comment": null, "goal": {"x": 0, "y": 0}})
    );
    // 既定値のない T? のフィールドがなければエラー (この型は既定値 null があるので通る)
    match error_of(check(
        POSE2D,
        jvalue!({"position": {"x": 1.0}, "goal": {"x": 0, "y": 0}}),
    )) {
        JuizError::ValueDoesNotMatchTypeError {
            type_name, path, ..
        } => {
            assert_eq!(type_name, "robot.geometry.Point2D");
            assert_eq!(path, "Pose2D.position.y");
        }
        e => panic!("{e:?}"),
    }
    let no_default =
        r#"{"type": "record", "name": "R", "fields": [{"name": "c", "type": ["null", "string"]}]}"#;
    assert!(check(no_default, jvalue!({})).is_err());
    check(no_default, jvalue!({"c": null})).unwrap();
}

#[test]
fn value_check_enum_and_bytes_default() {
    let e = r#"{"type": "enum", "name": "E", "symbols": ["A", "B"]}"#;
    check(e, jvalue!("A")).unwrap();
    assert!(check(e, jvalue!("C")).is_err());
    let with_default =
        data_type(r#"{"type": "enum", "name": "E", "symbols": ["A", "B"], "default": "A"}"#);
    assert_eq!(
        read_value(&with_default, &jvalue!("C"), "E").unwrap(),
        jvalue!("A")
    );
    // bytes の既定値は Avro では 0〜255 の文字列、辞書では base64
    let r = data_type(
        r#"{"type": "record", "name": "R", "fields": [{"name": "b", "type": "bytes", "default": "\u0000\u0001ÿ"}]}"#,
    );
    assert_eq!(
        read_value(&r, &jvalue!({}), "R").unwrap(),
        jvalue!({"b": "AAH/"})
    );
}

#[test]
fn record_field_default_value_uses_dict_form() {
    let f = RecordField::new("b", DataType::Bytes)
        .default_value(jvalue!("AAH/"))
        .unwrap();
    assert_eq!(f.default, Some(jvalue!("\u{0}\u{1}\u{ff}")));
    assert_eq!(f.default_as_dict(), Some(jvalue!("AAH/")));
    // T? の並びは既定値で決まる (仕様 §9.2)
    let some = RecordField::new("o", DataType::optional(DataType::Long))
        .default_value(jvalue!(3))
        .unwrap();
    assert_eq!(
        some.data_type,
        DataType::Optional {
            inner: Box::new(DataType::Long),
            null_first: false
        }
    );
    let none = RecordField::new("o", DataType::optional(DataType::Long))
        .default_value(jvalue!(null))
        .unwrap();
    assert_eq!(none.data_type, DataType::optional(DataType::Long));
    assert!(RecordField::new("i", DataType::Int)
        .default_value(jvalue!("x"))
        .is_err());
}

// ---- §7 互換性 ----

fn compat(w: &str, r: &str) -> Compatibility {
    compare_types(Some(&schema(w)), Some(&schema(r))).unwrap()
}

fn record(fields: &str) -> String {
    format!(r#"{{"type": "record", "name": "R", "namespace": "n", "fields": [{fields}]}}"#)
}

#[test]
fn compatibility_results() {
    assert_eq!(compat(POSE2D, POSE2D), Compatibility::Same);
    assert_eq!(
        compare_types(None, Some(&schema(POSE2D))).unwrap(),
        Compatibility::Untyped
    );
    assert_eq!(
        compare_types(Some(&schema(POSE2D)), None).unwrap(),
        Compatibility::Untyped
    );
    // 既定値だけ違う → 指紋が同じなので「同じ」
    assert_eq!(
        compat(
            POSE2D,
            &POSE2D.replace("\"default\": 0.0", "\"default\": 2.0")
        ),
        Compatibility::Same
    );
}

#[test]
fn compatibility_primitives() {
    assert_eq!(compat(r#""int""#, r#""long""#), Compatibility::Compatible);
    assert_eq!(compat(r#""int""#, r#""double""#), Compatibility::Compatible);
    assert_eq!(compat(r#""long""#, r#""float""#), Compatibility::Compatible);
    assert_eq!(
        compat(r#""float""#, r#""double""#),
        Compatibility::Compatible
    );
    assert!(!compat(r#""long""#, r#""int""#).is_connectable());
    assert!(!compat(r#""double""#, r#""float""#).is_connectable());
    assert!(!compat(r#""string""#, r#""bytes""#).is_connectable());
    assert!(!compat(r#""bytes""#, r#""string""#).is_connectable());
    assert!(!compat(r#""boolean""#, r#""long""#).is_connectable());
}

#[test]
fn compatibility_any_image_optional_collections() {
    assert_eq!(compat(POSE2D, r#""juiz.Any""#), Compatibility::Compatible);
    assert_eq!(
        compat(r#""juiz.Any""#, r#""long""#),
        Compatibility::Compatible
    );
    assert_eq!(
        compat(r#""juiz.Image""#, r#""juiz.Any""#),
        Compatibility::Compatible
    );
    assert!(!compat(r#""juiz.Any""#, r#""juiz.Image""#).is_connectable());
    assert!(!compat(r#""juiz.Image""#, r#""bytes""#).is_connectable());
    assert_eq!(
        compat(r#""juiz.Image""#, r#""juiz.Image""#),
        Compatibility::Same
    );

    assert_eq!(
        compat(r#""long""#, r#"["null", "long"]"#),
        Compatibility::Compatible
    );
    assert_eq!(
        compat(r#"["null", "int"]"#, r#"["long", "null"]"#),
        Compatibility::Compatible
    );
    assert_eq!(
        compat(r#""null""#, r#"["null", "long"]"#),
        Compatibility::Compatible
    );
    assert!(!compat(r#"["null", "long"]"#, r#""long""#).is_connectable());

    assert_eq!(
        compat(
            r#"{"type": "array", "items": "int"}"#,
            r#"{"type": "array", "items": "long"}"#
        ),
        Compatibility::Compatible
    );
    assert!(!compat(
        r#"{"type": "map", "values": "long"}"#,
        r#"{"type": "map", "values": "int"}"#
    )
    .is_connectable());
    assert!(!compat(
        r#"{"type": "map", "values": "long"}"#,
        r#"{"type": "array", "items": "long"}"#
    )
    .is_connectable());
}

/// 仕様 §7.3 の表
#[test]
fn compatibility_evolution_table() {
    let old = record(r#"{"name": "a", "type": "long"}"#);
    let with_default =
        record(r#"{"name": "a", "type": "long"}, {"name": "b", "type": "long", "default": 0}"#);
    let without_default = record(r#"{"name": "a", "type": "long"}, {"name": "b", "type": "long"}"#);
    // 既定値つきのフィールドを足す: 新→旧 ○、旧→新 ○
    assert!(compat(&with_default, &old).is_connectable());
    assert!(compat(&old, &with_default).is_connectable());
    // 既定値なしのフィールドを足す: 新→旧 ○、旧→新 ×
    assert!(compat(&without_default, &old).is_connectable());
    match compat(&old, &without_default) {
        Compatibility::Incompatible { path, .. } => assert_eq!(path, "R.b"),
        c => panic!("{c:?}"),
    }
    // 名前を変えて aliases に古い名前
    let renamed = record(r#"{"name": "c", "type": "long", "aliases": ["a"]}"#);
    assert!(compat(&old, &renamed).is_connectable());
    let old_with_default = record(r#"{"name": "a", "type": "long", "default": 0}"#);
    assert!(compat(&renamed, &old_with_default).is_connectable());
    assert!(!compat(&renamed, &old).is_connectable());
    // int → long
    let int_field = record(r#"{"name": "a", "type": "int"}"#);
    assert!(compat(&int_field, &old).is_connectable());
    match compat(&old, &int_field) {
        Compatibility::Incompatible { path, .. } => assert_eq!(path, "R.a"),
        c => panic!("{c:?}"),
    }
    // 順番を変える
    let two = record(r#"{"name": "a", "type": "long"}, {"name": "b", "type": "long"}"#);
    let swapped = record(r#"{"name": "b", "type": "long"}, {"name": "a", "type": "long"}"#);
    assert_eq!(compat(&two, &swapped), Compatibility::Compatible);
    // 名前空間・名前を変える: aliases があれば ○
    let moved = r#"{"type": "record", "name": "R2", "namespace": "m", "aliases": ["n.R"], "fields": [{"name": "a", "type": "long"}]}"#;
    assert!(!compat(moved, &old).is_connectable());
    assert!(compat(&old, moved).is_connectable());
    // 中身が同じでも名前が違えば互換でない
    let other_name = r#"{"type": "record", "name": "S", "namespace": "n", "fields": [{"name": "a", "type": "long"}]}"#;
    assert!(!compat(&old, other_name).is_connectable());
}

#[test]
fn compatibility_enum() {
    let ab = r#"{"type": "enum", "name": "E", "symbols": ["A", "B"]}"#;
    let abc = r#"{"type": "enum", "name": "E", "symbols": ["A", "B", "C"]}"#;
    let ab_default = r#"{"type": "enum", "name": "E", "symbols": ["A", "B"], "default": "A"}"#;
    assert!(compat(ab, abc).is_connectable());
    assert!(!compat(abc, ab).is_connectable());
    assert!(compat(abc, ab_default).is_connectable());
}

#[test]
fn ensure_compatible_error() {
    let w = schema(r#""long""#);
    let r = schema(r#""int""#);
    match error_of(ensure_compatible(Some(&w), Some(&r))) {
        JuizError::TypeIncompatibleError {
            writer,
            reader,
            path,
            ..
        } => {
            assert_eq!(writer, w.identifier());
            assert_eq!(reader, r.identifier());
            assert_eq!(path, "int");
        }
        e => panic!("{e:?}"),
    }
    ensure_compatible(None, Some(&r)).unwrap();
    ensure_compatible(Some(&r), Some(&w)).unwrap();
}

// ---- §6 manifest ----

#[test]
fn typed_argument_manifest() {
    let s = schema(POSE2D);
    let arg = ArgumentManifest::new_typed(
        "target",
        s.clone(),
        jvalue!({"position": {"x": 0.0, "y": 0.0}, "goal": {"x": 0.0, "y": 0.0}}),
    )
    .unwrap();
    assert_eq!(arg.type_name, ArgumentType::Object);
    assert_eq!(arg.schema.as_ref(), Some(&s));
    // 既定値が型に合わない
    assert!(ArgumentManifest::new_typed("target", s.clone(), jvalue!({"x": 1})).is_err());
    // null は既定値なし
    ArgumentManifest::new_typed("target", s.clone(), Value::Null).unwrap();
    // 型付きの引数名は Avro の名前の規則
    assert!(ArgumentManifest::new_typed("my-target", s.clone(), Value::Null).is_err());
    // null だけの型は引数にできない
    assert!(ArgumentManifest::new_typed("n", schema(r#""null""#), Value::Null).is_err());
    // image は既定値を見ない
    let img = ArgumentManifest::new_typed("img", schema(r#""juiz.Image""#), jvalue!({})).unwrap();
    assert_eq!(img.type_name, ArgumentType::Image);

    // JSON に schema が載り、型なしの引数には出ない
    let json = serde_json::to_value(&arg).unwrap();
    assert_eq!(json["schema"]["name"], jvalue!("robot.geometry.Pose2D"));
    let untyped = serde_json::to_value(ArgumentManifest::new_int("a", 1)).unwrap();
    assert!(untyped.get("schema").is_none());
    let back: ArgumentManifest = serde_json::from_value(json).unwrap();
    assert_eq!(back.schema, arg.schema);
    back.check_schema().unwrap();
}

#[test]
fn array_and_image_argument_types() {
    assert_eq!(
        ArgumentManifest::new_array("a", vec![]).type_name,
        ArgumentType::Array
    );
    assert_eq!(
        ArgumentManifest::new_image("i").type_name,
        ArgumentType::Image
    );
    assert_eq!(
        ArgumentProfile::new_array("a", vec![]).type_name,
        ArgumentType::Array
    );
    assert_eq!(
        ArgumentProfile::new_image("i").type_name,
        ArgumentType::Image
    );
}

use juiz_sdk::manifests::ArgumentProfile;

#[test]
fn process_manifest_output_schema() {
    let s = schema(POSE2D);
    let manifest = ProcessManifest::new("move_to")
        .add_typed_arg("target", "goal pose", s.clone(), Value::Null)
        .unwrap()
        .add_float_arg("speed", "", 1.0)
        .set_output_schema(Some(s.clone()))
        .unwrap();
    manifest.check_schemas().unwrap();
    // 古いクライアント向けに outputs も埋まる
    assert_eq!(
        manifest
            .outputs
            .as_ref()
            .unwrap()
            .as_primitive()
            .unwrap()
            .type_name,
        "Object"
    );
    // JSON を経由しても残る
    let json = serde_json::to_value(&manifest).unwrap();
    let back: ProcessManifest = json.clone().try_into().unwrap();
    assert_eq!(back.output_schema.as_ref(), Some(&s));
    assert_eq!(back.arguments[0].schema.as_ref(), Some(&s));
    // インスタンスの manifest にも引き継ぐ
    let instance = manifest
        .build_instance_manifest(ProcessManifest::new("move_to").name("m0"))
        .unwrap();
    assert_eq!(instance.output_schema.as_ref(), Some(&s));
    assert_eq!(instance.arguments[0].schema.as_ref(), Some(&s));
    let profile: ProcessProfile = instance.try_into().unwrap();
    assert_eq!(profile.output_schema.as_ref(), Some(&s));
    assert_eq!(profile.arguments[0].schema.as_ref(), Some(&s));
    // 型なしなら JSON に出ない
    let untyped = serde_json::to_value(ProcessManifest::new("p")).unwrap();
    assert!(untyped.get("output_schema").is_none());
}

// ---- JuizType ----

/// 平面上の点
#[derive(Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
struct Point2D {
    x: f64,
    y: f64,
}

#[derive(Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
struct Pose2D {
    position: Point2D,
    #[juiz(default = 0.0)]
    theta: f64,
    #[juiz(default = "map")]
    frame: String,
    /// メモ
    #[juiz(default = null)]
    comment: Option<String>,
}

#[derive(Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
enum MotorMode {
    #[juiz(default)]
    Idle,
    Velocity,
    Position,
}

#[derive(Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot")]
struct Sample {
    count: i32,
    total: i64,
    ratio: f32,
    flag: bool,
    data: Vec<u8>,
    tags: Vec<String>,
    table: std::collections::HashMap<String, i64>,
    mode: MotorMode,
    maybe: Option<Point2D>,
    extra: Value,
}

#[test]
fn derived_record_matches_hand_written_schema() {
    let hand = DataType::from_avro_str(
        r#"{"type": "record", "name": "Pose2D", "namespace": "robot.geometry", "fields": [
            {"name": "position", "type": {"type": "record", "name": "Point2D", "fields": [
                {"name": "x", "type": "double"}, {"name": "y", "type": "double"}]}},
            {"name": "theta", "type": "double", "default": 0.0},
            {"name": "frame", "type": "string", "default": "map"},
            {"name": "comment", "type": ["null", "string"], "default": null}]}"#,
    )
    .unwrap();
    assert_eq!(Pose2D::data_type().fingerprint(), hand.fingerprint());
    assert_eq!(Pose2D::type_schema().unwrap().name, "robot.geometry.Pose2D");
    assert!(Pose2D::type_identifier().starts_with("robot.geometry.Pose2D@sha256:"));
    match Point2D::data_type() {
        DataType::Record(r) => assert_eq!(r.doc.as_deref(), Some("平面上の点")),
        _ => panic!(),
    }
    assert_eq!(Pose2D::type_tag(), None);
}

#[test]
fn derived_value_conversion() {
    let pose = Pose2D {
        position: Point2D { x: 1.0, y: 2.0 },
        theta: 0.5,
        frame: "odom".to_owned(),
        comment: None,
    };
    let v = pose.to_value().unwrap();
    assert_eq!(
        v,
        jvalue!({"position": {"x": 1.0, "y": 2.0}, "theta": 0.5, "frame": "odom", "comment": null})
    );
    assert_eq!(Pose2D::from_value(&v).unwrap(), pose);
    // 既定値で埋める
    let short = Pose2D::from_value(&jvalue!({"position": {"x": 1.0, "y": 2.0}})).unwrap();
    assert_eq!(short.theta, 0.0);
    assert_eq!(short.frame, "map");
    assert!(Pose2D::from_value(&jvalue!({"theta": 1.0})).is_err());
    // Capsule を経由する
    let capsule: CapsulePtr = pose.into_capsule().unwrap().into();
    assert_eq!(Pose2D::from_capsule_ptr(&capsule).unwrap().frame, "odom");
}

#[test]
fn derived_enum_and_collections() {
    assert_eq!(MotorMode::Velocity.to_value().unwrap(), jvalue!("Velocity"));
    assert_eq!(
        MotorMode::from_value(&jvalue!("Position")).unwrap(),
        MotorMode::Position
    );
    // 知らない記号は既定値
    assert_eq!(
        MotorMode::from_value(&jvalue!("Unknown")).unwrap(),
        MotorMode::Idle
    );
    let sample = Sample {
        count: 1,
        total: 2,
        ratio: 0.5,
        flag: true,
        data: vec![0, 1, 255],
        tags: vec!["a".to_owned()],
        table: [("k".to_owned(), 3)].into_iter().collect(),
        mode: MotorMode::Velocity,
        maybe: Some(Point2D { x: 0.0, y: 1.0 }),
        extra: jvalue!({"free": [1]}),
    };
    let v = sample.to_value().unwrap();
    assert_eq!(v["data"], jvalue!("AAH/"));
    Sample::type_schema().unwrap().check_value(&v).unwrap();
    assert_eq!(Sample::from_value(&v).unwrap(), sample);
    assert_eq!(Sample::data_type().display_name(), "robot.Sample");
    // i32 の範囲外はエラー
    let mut bad = v.clone();
    bad["count"] = jvalue!(3000000000i64);
    assert!(Sample::from_value(&bad).is_err());
}

#[test]
fn juiz_type_argument_and_returns() {
    let manifest = ProcessManifest::new("move_to")
        .add_arg(
            ArgumentManifest::new_juiz_type::<f64>("speed", Some(&1.0))
                .unwrap()
                .description("speed"),
        )
        .returns::<Pose2D>()
        .unwrap();
    assert_eq!(manifest.arguments[0].type_name, ArgumentType::Float);
    assert_eq!(
        manifest.arguments[0].schema.as_ref().unwrap().name,
        "double"
    );
    assert_eq!(
        manifest.output_schema.unwrap(),
        Pose2D::type_schema().unwrap()
    );
    assert_eq!(
        <Vec<Option<i64>>>::data_type().display_name(),
        "array<long?>"
    );
    assert_eq!(<()>::data_type(), DataType::Null);
}
