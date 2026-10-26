//! データ型の manifest と JuizType (issue #190、仕様 docs/spec/datatype.md §6、§8、§11、§15) のテスト
//!
//! 型のモデル・指紋・値の検査・互換性のテストは juiz_types/tests/datatype_test.rs にある。

use juiz_sdk::manifests::ArgumentProfile;
use juiz_sdk::prelude::*;
use juiz_sdk::types::*;

const POSE2D: &str = r#"{
  "type": "record", "name": "Pose2D", "namespace": "robot.geometry",
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

fn error_of<T: std::fmt::Debug>(r: JuizResult<T>) -> JuizError {
    r.unwrap_err().downcast::<JuizError>().unwrap()
}

#[test]
fn type_errors_become_juiz_errors() {
    match error_of(ArgumentManifest::new_typed(
        "a",
        schema(r#""long""#),
        jvalue!("x"),
    )) {
        JuizError::ValueDoesNotMatchTypeError { type_name, .. } => assert_eq!(type_name, "long"),
        e => panic!("{e:?}"),
    }
    match error_of(i32::from_value(&jvalue!(3000000000i64))) {
        JuizError::ValueDoesNotMatchTypeError { .. } => {}
        e => panic!("{e:?}"),
    }
    let mut broken = schema(r#""long""#);
    broken.fingerprint = "sha256:00".to_owned();
    match error_of(ProcessManifest::new("p").set_output_schema(Some(broken))) {
        JuizError::TypeSchemaInvalidError { .. } => {}
        e => panic!("{e:?}"),
    }
    assert_eq!(
        schema(POSE2D).argument_type().unwrap(),
        ArgumentType::Object
    );
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
