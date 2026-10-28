//! ROS 2 の型の取り込み (`juiz idl import-ros2`、issue #201、仕様 §25) と JIDL の属性・定数のテスト

use std::path::{Path, PathBuf};

use juiz_idl::ros2::{import_ros2, Ros2Import, Ros2ImportOptions};
use juiz_idl::{generate, load_files, load_files_with, load_str, LoadOptions, OutputKind};
use juiz_types::{check_value, compare_data_types, DataType};
use serde_json::json;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ros2")
}

/// test_package に置いた取り込み済みの .jidl (build.rs で Rust にしてコンパイルする)
fn test_package_idl() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test_package/idl")
}

const INPUTS: [&str; 5] = [
    "demo_msgs/msg/Sample.msg",
    "demo_msgs/msg/Reading.idl",
    "demo_msgs/msg/Mapped.msg",
    "demo_msgs/srv/AddTwoInts.srv",
    "demo_msgs/srv/Trigger2.idl",
];

fn import(inputs: &[&str], std_map: bool) -> Ros2Import {
    let paths: Vec<PathBuf> = inputs.iter().map(|i| fixtures().join(i)).collect();
    import_ros2(
        &paths,
        &Ros2ImportOptions {
            search_dirs: Vec::new(),
            std_map,
        },
    )
    .unwrap()
}

fn import_err(inputs: &[&str], search_dirs: Vec<PathBuf>) -> String {
    let paths: Vec<PathBuf> = inputs.iter().map(|i| fixtures().join(i)).collect();
    import_ros2(
        &paths,
        &Ros2ImportOptions {
            search_dirs,
            std_map: true,
        },
    )
    .unwrap_err()
    .to_string()
}

/// 書き出して読み直す
fn import_and_load(inputs: &[&str], std_map: bool, dir: &str) -> (Ros2Import, juiz_idl::IdlModel) {
    let imported = import(inputs, std_map);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(dir);
    let _ = std::fs::remove_dir_all(&out);
    juiz_idl::write_files(&imported.files, &out).unwrap();
    let written: Vec<PathBuf> = imported.files.iter().map(|f| out.join(&f.path)).collect();
    let model = load_files_with(&written, &LoadOptions::default()).unwrap();
    (imported, model)
}

fn record(model: &juiz_idl::IdlModel, name: &str) -> juiz_types::RecordType {
    match model.get_type(name) {
        Some(DataType::Record(r)) => r.clone(),
        other => panic!("{name}: {other:?}"),
    }
}

/// test_package/idl/ros2 が取り込みの結果と一致するか。
/// `JUIZ_IDL_UPDATE_ROS2_GEN=1 cargo test -p juiz_idl --test ros2_test` で作り直す
#[test]
fn test_package_ros2_idl_is_up_to_date() {
    let imported = import(&INPUTS, true);
    let dir = test_package_idl();
    if std::env::var_os("JUIZ_IDL_UPDATE_ROS2_GEN").is_some() {
        let _ = std::fs::remove_dir_all(dir.join("ros2"));
        juiz_idl::write_files(&imported.files, &dir).unwrap();
    }
    for f in imported.files.iter() {
        let path = dir.join(&f.path);
        let on_disk = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            on_disk == f.contents,
            "{} is out of date; regenerate with JUIZ_IDL_UPDATE_ROS2_GEN=1",
            path.display()
        );
    }
}

#[test]
fn msg_fields_become_types_with_attributes() {
    let (imported, model) = import_and_load(&INPUTS, true, "ros2_msg");
    let r = record(&model, "ros2.demo_msgs.msg.Sample");
    assert_eq!(
        r.doc.as_deref(),
        Some("ROS 2 の型の取り込みのテスト用 (issue #201)\n2 行目の説明")
    );
    let field = |n: &str| r.get_field(n).unwrap().clone();
    // 型は近いものに寄せる (仕様 §10 の表)
    assert_eq!(field("small").data_type, DataType::Int);
    assert_eq!(field("count").data_type, DataType::Long);
    assert_eq!(field("big").data_type, DataType::Long);
    assert_eq!(field("label").data_type, DataType::String);
    assert_eq!(field("fixed").data_type, DataType::array(DataType::Int));
    assert_eq!(field("blob").data_type, DataType::Bytes);
    assert_eq!(field("pair").data_type, DataType::Bytes);
    assert_eq!(field("wide").data_type, DataType::String);
    assert_eq!(field("letter").data_type, DataType::Int);
    // 制約と元の型は属性に残す
    let attrs = |n: &str| serde_json::Value::Object(field(n).attributes);
    assert_eq!(
        attrs("small"),
        json!({"juiz.ros_type": "int16", "juiz.minimum": -32768, "juiz.maximum": 32767})
    );
    assert_eq!(
        attrs("count"),
        json!({"juiz.ros_type": "uint32", "juiz.minimum": 0, "juiz.maximum": 4294967295u64})
    );
    assert_eq!(
        attrs("big"),
        json!({"juiz.ros_type": "uint64", "juiz.minimum": 0})
    );
    assert_eq!(attrs("label"), json!({"juiz.maxLength": 10}));
    assert_eq!(
        attrs("fixed"),
        json!({"juiz.minItems": 3, "juiz.maxItems": 3})
    );
    assert_eq!(attrs("bounded"), json!({"juiz.maxItems": 4}));
    assert_eq!(
        attrs("pair"),
        json!({"juiz.ros_type": "uint8", "juiz.minItems": 2, "juiz.maxItems": 2})
    );
    assert_eq!(attrs("wide"), json!({"juiz.ros_type": "wstring"}));
    // 既定値: 書いたもの、なければ ROS 2 と同じゼロ値
    assert_eq!(field("small").default_as_dict(), Some(json!(-5)));
    assert_eq!(field("count").default_as_dict(), Some(json!(0)));
    assert_eq!(field("label").default_as_dict(), Some(json!("abc")));
    assert_eq!(field("fixed").default_as_dict(), Some(json!([1, 2, 3])));
    assert_eq!(field("pair").default_as_dict(), Some(json!("AAA=")));
    assert_eq!(field("flag").default_as_dict(), Some(json!(true)));
    assert_eq!(field("small").doc.as_deref(), Some("行末のコメント"));
    assert_eq!(field("header").doc.as_deref(), Some("時刻と座標系"));
    // 定数
    assert_eq!(
        r.attributes["juiz.constants"],
        json!([
            {"name": "MODE_A", "type": "int", "ros_type": "uint8", "value": 1},
            {"name": "NAME_PREFIX", "type": "string", "ros_type": "string", "value": "demo_"},
            {"name": "SCALE", "type": "double", "ros_type": "float64", "value": 0.5}
        ])
    );
    // 同じパッケージの型は入力の隣から読み、一緒に書き出す
    let inner = record(&model, "ros2.demo_msgs.msg.Inner");
    assert_eq!(
        inner.get_field("tags").unwrap().default_as_dict(),
        Some(json!(["a", "b,c"]))
    );
    assert!(imported
        .files
        .iter()
        .any(|f| f.path == Path::new("ros2/demo_msgs/msg/Inner.jidl")));
    // uint64 は警告
    assert!(imported
        .warnings
        .iter()
        .any(|w| w.contains("Sample.big") && w.contains("uint64")));
}

#[test]
fn standard_messages_are_replaced_with_juiz_std() {
    let (imported, model) = import_and_load(&INPUTS, true, "ros2_std_map");
    let r = record(&model, "ros2.demo_msgs.msg.Sample");
    assert_eq!(
        r.get_field("header")
            .unwrap()
            .data_type
            .full_name()
            .as_deref(),
        Some("juiz_std.common.Header")
    );
    assert_eq!(
        r.get_field("pose")
            .unwrap()
            .data_type
            .full_name()
            .as_deref(),
        Some("juiz_std.geometry.Pose")
    );
    let m = record(&model, "ros2.demo_msgs.msg.Mapped");
    for (field, std) in [
        ("imu", "juiz_std.sensor.Imu"),
        ("scan", "juiz_std.sensor.LaserScan"),
        ("range", "juiz_std.sensor.Range"),
        ("twist", "juiz_std.geometry.Twist"),
        ("point", "juiz_std.geometry.Point3D"),
        ("stamp", "juiz_std.common.Time"),
        ("period", "juiz_std.common.Duration"),
    ] {
        assert_eq!(
            m.get_field(field).unwrap().data_type.full_name().as_deref(),
            Some(std),
            "{field}"
        );
    }
    // 置き換えた型は ros2.* として書き出さない
    assert!(
        imported
            .files
            .iter()
            .all(|f| !f.path.starts_with("ros2/std_msgs")
                && !f.path.starts_with("ros2/geometry_msgs"))
    );
    assert!(model.get_type("ros2.std_msgs.msg.Header").is_none());
}

#[test]
fn std_map_can_be_turned_off() {
    let (imported, model) =
        import_and_load(&["demo_msgs/msg/Sample.msg"], false, "ros2_no_std_map");
    let r = record(&model, "ros2.demo_msgs.msg.Sample");
    assert_eq!(
        r.get_field("header")
            .unwrap()
            .data_type
            .full_name()
            .as_deref(),
        Some("ros2.std_msgs.msg.Header")
    );
    let paths: Vec<&Path> = imported.files.iter().map(|f| f.path.as_path()).collect();
    for p in [
        "ros2/builtin_interfaces/msg/Time.jidl",
        "ros2/std_msgs/msg/Header.jidl",
        "ros2/geometry_msgs/msg/Pose.jidl",
        "ros2/geometry_msgs/msg/Point.jidl",
        "ros2/geometry_msgs/msg/Quaternion.jidl",
    ] {
        assert!(paths.contains(&Path::new(p)), "{p}");
    }
    // ほかのパッケージは相対パスで import する
    let sample = imported
        .files
        .iter()
        .find(|f| f.path == Path::new("ros2/demo_msgs/msg/Sample.jidl"))
        .unwrap();
    assert!(sample
        .contents
        .contains("import idl \"../../std_msgs/msg/Header.jidl\";"));
    // ROS 2 の Time は int32 / uint32 なので、juiz_std の Time とは別の型
    let time = record(&model, "ros2.builtin_interfaces.msg.Time");
    assert_eq!(time.get_field("sec").unwrap().data_type, DataType::Int);
    assert_eq!(time.get_field("nanosec").unwrap().data_type, DataType::Long);
    // 置き換えの対象をそのまま入力にすると、置き換えるときは書き出さない
    let replaced = import(&["geometry_msgs/msg/Pose.msg"], true);
    assert!(replaced.files.is_empty());
    assert!(replaced.warnings[0].contains("juiz_std.geometry.Pose"));
}

#[test]
fn ros_idl_files_are_read() {
    let (_, model) = import_and_load(&INPUTS, true, "ros2_idl");
    let r = record(&model, "ros2.demo_msgs.msg.Reading");
    assert_eq!(r.doc.as_deref(), Some("センサの値\n2 行目"));
    let field = |n: &str| r.get_field(n).unwrap().clone();
    assert_eq!(field("value").doc.as_deref(), Some("値"));
    assert_eq!(field("value").default_as_dict(), Some(json!(1.5)));
    // typedef の固定長配列
    assert_eq!(
        field("covariance").data_type,
        DataType::array(DataType::Double)
    );
    assert_eq!(field("covariance").attributes["juiz.minItems"], json!(9));
    assert_eq!(
        field("covariance").default_as_dict(),
        Some(json!(vec![0.0; 9]))
    );
    assert_eq!(field("samples").attributes["juiz.maxItems"], json!(5));
    assert_eq!(field("name").attributes["juiz.maxLength"], json!(8));
    assert_eq!(field("name").default_as_dict(), Some(json!("hello")));
    assert_eq!(field("pair").default_as_dict(), Some(json!([1, 2])));
    assert_eq!(field("big").attributes["juiz.ros_type"], json!("uint64"));
    assert_eq!(field("raw").attributes["juiz.ros_type"], json!("byte"));
    assert_eq!(
        r.attributes["juiz.constants"],
        json!([
            {"name": "KIND_A", "type": "int", "ros_type": "uint8", "value": 0},
            {"name": "UNIT", "type": "string", "ros_type": "string", "value": "m"}
        ])
    );
    // rosidl が空のメッセージに足すフィールドは除く
    let req = record(&model, "ros2.demo_msgs.srv.Trigger2_Request");
    assert!(req.fields.is_empty());
}

#[test]
fn srv_becomes_process() {
    let (_, model) = import_and_load(&INPUTS, true, "ros2_srv");
    let srv = model
        .signatures
        .iter()
        .find(|s| s.namespace == "ros2.demo_msgs.srv")
        .unwrap();
    let names: Vec<&str> = srv.processes.iter().map(|p| p.type_name.as_str()).collect();
    assert_eq!(names, ["add_two_ints", "trigger2"]);
    let add = &srv.processes[0];
    let args: Vec<(&str, Option<serde_json::Value>)> = add
        .arguments
        .iter()
        .map(|a| (a.name.as_str(), a.default.clone()))
        .collect();
    assert_eq!(args, [("a", Some(json!(0))), ("b", Some(json!(2)))]);
    assert_eq!(
        add.output_schema.name,
        "ros2.demo_msgs.srv.AddTwoInts_Response"
    );
    assert!(record(&model, "ros2.demo_msgs.srv.AddTwoInts_Request")
        .get_field("b")
        .is_some());
}

#[test]
fn import_errors() {
    // 参照先が見つからない
    let e = import_err(&["broken_msgs/msg/Missing.msg"], Vec::new());
    assert!(e.contains("broken_msgs/msg/Nowhere is not found"), "{e}");
    assert!(e.contains("AMENT_PREFIX_PATH"), "{e}");
    // 探し先 (-I) から見つける
    let found = import_ros2(
        &[fixtures().join("broken_msgs/msg/Missing.msg")],
        &Ros2ImportOptions {
            search_dirs: vec![fixtures().join("extra")],
            std_map: true,
        },
    )
    .unwrap();
    assert!(found
        .files
        .iter()
        .any(|f| f.path == Path::new("ros2/broken_msgs/msg/Nowhere.jidl")));
    // 範囲外の既定値
    let e = import_err(&["broken_msgs/msg/OutOfRange.msg"], Vec::new());
    assert!(e.contains("out of the range 0..=255"), "{e}");
    // 固定長配列の既定値の数
    let e = import_err(&["broken_msgs/msg/FixedDefault.msg"], Vec::new());
    assert!(e.contains("must have 3 items"), "{e}");
    // 置き場所が <package>/msg/ でない
    let e = import_err(&["broken_msgs/Loose.msg"], Vec::new());
    assert!(e.contains("<package>/msg/<Name>.msg"), "{e}");
    // action は対象外
    let e = import_err(&["broken_msgs/action/Move.action"], Vec::new());
    assert!(e.contains("actions are not supported"), "{e}");
    // .srv の区切り
    let e = import_err(&["broken_msgs/srv/NoSeparator.srv"], Vec::new());
    assert!(e.contains("'---'"), "{e}");
}

#[test]
fn constraints_are_checked_against_values() {
    let (_, model) = import_and_load(&INPUTS, true, "ros2_constraints");
    let t = model.get_type("ros2.demo_msgs.msg.Sample").unwrap();
    assert!(check_value(t, &json!({})).is_ok());
    assert!(check_value(t, &json!({"small": 100, "label": "0123456789"})).is_ok());
    for (bad, reason) in [
        (json!({"small": 40000}), "juiz.maximum"),
        (json!({"count": -1}), "juiz.minimum"),
        (json!({"label": "01234567890"}), "juiz.maxLength"),
        (json!({"fixed": [1, 2]}), "juiz.minItems"),
        (json!({"bounded": [1, 2, 3, 4, 5]}), "juiz.maxItems"),
        (json!({"pair": "AAAA"}), "juiz.maxItems"),
        (json!({"letter": 256}), "juiz.maximum"),
    ] {
        let e = check_value(t, &bad).unwrap_err().to_string();
        assert!(e.contains(reason), "{bad}: {e}");
    }
    // 配列の要素ごとに範囲を見る
    let r = model.get_type("ros2.demo_msgs.msg.Reading").unwrap();
    let e = check_value(r, &json!({"pair": [1, 70000]}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("pair[1]"), "{e}");
}

#[test]
fn attributes_round_trip_in_jidl() {
    let src = r#"
        namespace robot;
        @juiz.constants([{"name": "MAX", "type": "long", "value": 10}])
        record Limited {
          @juiz.maxLength(4) @juiz.ros_type("string") string name = "";
          @juiz.minItems(2) @juiz.maxItems(2) array<double> pair = [0.0, 0.0];
        }
    "#;
    let model = load_str(src, "limited.jidl").unwrap();
    let t = model.get_type("robot.Limited").unwrap().clone();
    let DataType::Record(r) = &t else { panic!() };
    assert_eq!(
        r.get_field("name").unwrap().attributes["juiz.maxLength"],
        json!(4)
    );
    // 属性は指紋に入らない
    let plain = load_str(
        "namespace robot; record Limited { string name = \"\"; array<double> pair = [0.0, 0.0]; }",
        "plain.jidl",
    )
    .unwrap();
    assert_eq!(t.fingerprint(), plain.types[0].fingerprint());
    // .avsc にも残る
    let avro = t.to_avro().unwrap();
    assert_eq!(avro["fields"][0]["juiz.maxLength"], json!(4));
    assert_eq!(avro["juiz.constants"][0]["name"], json!("MAX"));
    // JIDL に書き戻して読み直すと同じ
    let jidl = juiz_idl::to_jidl(&t);
    assert!(jidl.contains("@juiz.maxLength(4)"), "{jidl}");
    let again = load_str(&jidl, "again.jidl").unwrap();
    assert_eq!(again.types[0], t);
}

#[test]
fn bad_annotations_and_constants() {
    let err = |src: &str| load_str(src, "t.jidl").unwrap_err().to_string();
    let e = err("namespace a; record R { @unknown(1) int x; }");
    assert!(e.contains("@unknown"), "{e}");
    let e = err("namespace a; record R { @juiz.maxLength(1) @juiz.maxLength(2) string x; }");
    assert!(e.contains("written twice"), "{e}");
    let e = err(
        r#"namespace a; @juiz.constants([{"name": "X", "type": "int", "value": "s"}]) record R { int y; }"#,
    );
    assert!(e.contains("constant 'X'"), "{e}");
    let e = err(
        r#"namespace a; @juiz.constants([{"name": "y", "type": "int", "value": 1}]) record R { int y; }"#,
    );
    assert!(e.contains("name of a field"), "{e}");
    let e = err(
        r#"namespace a; @juiz.constants([{"name": "X", "type": "bytes", "value": ""}]) record R { int y; }"#,
    );
    assert!(e.contains("use boolean, int"), "{e}");
}

#[test]
fn constants_in_generated_code() {
    let model = load_str(
        r#"
        namespace robot;
        @juiz.constants([
          {"name": "MODE", "type": "int", "value": -1, "doc": "モード"},
          {"name": "BIG", "type": "long", "value": 5},
          {"name": "RATIO", "type": "float", "value": 0.5},
          {"name": "SCALE", "type": "double", "value": 2},
          {"name": "ON", "type": "boolean", "value": true},
          {"name": "UNIT", "type": "string", "value": "m\"s"}
        ])
        record Spec { int x = 0; }
        "#,
        "spec.jidl",
    )
    .unwrap();
    let file = |kind: OutputKind, path: &str| {
        generate(&model, &[kind])
            .unwrap()
            .into_iter()
            .find(|f| f.path == Path::new(path))
            .unwrap_or_else(|| panic!("{path}"))
            .contents
    };
    let rust = file(OutputKind::Rust, "robot.rs");
    for s in [
        "impl Spec {",
        "pub const MODE: i32 = -1;",
        "pub const BIG: i64 = 5;",
        "pub const RATIO: f32 = 0.5;",
        "pub const SCALE: f64 = 2.0;",
        "pub const ON: bool = true;",
        "pub const UNIT: &'static str = \"m\\\"s\";",
        "/// モード",
    ] {
        assert!(rust.contains(s), "{s}\n{rust}");
    }
    let py = file(OutputKind::Python, "robot/__init__.py");
    for s in [
        "Spec.MODE = -1",
        "Spec.SCALE = 2",
        "Spec.ON = True",
        "Spec.UNIT = \"m\\\"s\"",
    ] {
        assert!(py.contains(s), "{s}\n{py}");
    }
    let cpp = file(OutputKind::Cpp, "robot.h");
    for s in [
        "static constexpr int32_t MODE = -1;",
        "static constexpr int64_t BIG = INT64_C(5);",
        "static constexpr float RATIO = 0.5f;",
        "static constexpr double SCALE = 2.0;",
        "static constexpr bool ON = true;",
        "static constexpr const char* UNIT = \"m\\\"s\";",
    ] {
        assert!(cpp.contains(s), "{s}\n{cpp}");
    }
    let hs = file(OutputKind::Haskell, "Robot.hs");
    for s in [
        "specMODE :: HI.Int32\nspecMODE = (-1)",
        "specON = True",
        "specSCALE :: Double",
        ", specUNIT",
    ] {
        assert!(hs.contains(s), "{s}\n{hs}");
    }
    let ts = file(OutputKind::TypeScript, "robot.ts");
    for s in [
        "export const SpecConstants = {",
        "  MODE: -1,",
        "  UNIT: \"m\\\"s\",",
        "} as const;",
    ] {
        assert!(ts.contains(s), "{s}\n{ts}");
    }
}

/// juiz_std の Imu・LaserScan・Range に足したフィールドは既定値つきなので、前の版 (0.0.24) の型と互換 (仕様 §7)
#[test]
fn extended_std_types_are_compatible_with_previous_ones() {
    let current = load_files(&["juiz_std/sensor.jidl"]).unwrap();
    for (name, added, previous_fingerprint) in [
        (
            "Imu",
            &[
                "orientation_covariance",
                "angular_velocity_covariance",
                "linear_acceleration_covariance",
            ][..],
            "sha256:ecf878ed8e4c3c5256440d854e6928405de03891d559f971b63a0d0dd73830cf",
        ),
        (
            "LaserScan",
            &["time_increment", "scan_time", "intensities"][..],
            "sha256:25fcb9214e49be850477e56173ac17546cd17826c6987f43b8315b2b168878d8",
        ),
        (
            "Range",
            &["radiation_type", "field_of_view"][..],
            "sha256:03fb8b5d1132e768632b7f4db0b4e7e08f353af7b7919c460863a3e988f82aff",
        ),
    ] {
        let new = current
            .get_type(&format!("juiz_std.sensor.{name}"))
            .unwrap();
        // 足したフィールドを除くと前の版の型に戻る
        let DataType::Record(mut r) = new.clone() else {
            panic!()
        };
        r.fields.retain(|f| !added.contains(&f.name.as_str()));
        let old = DataType::Record(r);
        assert_eq!(old.fingerprint(), previous_fingerprint, "{name}");
        assert!(compare_data_types(&old, new).is_ok(), "{name}: old -> new");
        assert!(compare_data_types(new, &old).is_ok(), "{name}: new -> old");
    }
}
