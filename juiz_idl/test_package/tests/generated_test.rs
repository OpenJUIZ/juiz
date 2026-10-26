//! juiz_idl の build.rs ヘルパで生成した Rust のコードのテスト (issue #191)

use juiz_idl_test_package::robot::common::{Header, Stamp};
use juiz_idl_test_package::robot::geometry::{Everything, MotorMode, Point2D, Pose2D};
use juiz_sdk::prelude::*;

/// 同じ形の `#[derive(JuizType)]` (仕様 §11: JIDL と derive の指紋は一致しなければならない)
mod derived {
    use juiz_sdk::prelude::*;

    /// 平面上の点
    #[derive(JuizType)]
    #[juiz(namespace = "robot.geometry")]
    pub struct Point2D {
        pub x: f64,
        pub y: f64,
    }

    #[derive(JuizType)]
    #[juiz(namespace = "robot.geometry")]
    pub enum MotorMode {
        #[juiz(default)]
        Idle,
        Velocity,
        Position,
    }
}

#[test]
fn fingerprints_match_derive() {
    assert_eq!(
        Point2D::type_identifier(),
        derived::Point2D::type_identifier()
    );
    assert_eq!(
        MotorMode::type_identifier(),
        derived::MotorMode::type_identifier()
    );
    assert_eq!(MotorMode::data_type(), derived::MotorMode::data_type());
}

#[test]
fn fingerprints_match_written_avsc() {
    let out_dir = env!("OUT_DIR");
    let avsc = std::fs::read_to_string(format!("{out_dir}/robot.geometry.Pose2D.avsc")).unwrap();
    let schema = TypeSchema::from_avro_str(&avsc).unwrap();
    assert_eq!(schema, Pose2D::type_schema().unwrap());
    // 手で書いた .avsc を import した型も同じ指紋になる
    let stamp =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/idl/stamp.avsc")).unwrap();
    assert_eq!(
        TypeSchema::from_avro_str(&stamp).unwrap().fingerprint,
        Stamp::type_schema().unwrap().fingerprint
    );
}

#[test]
fn record_round_trip_and_defaults() {
    let pose = Pose2D::from_value(&jvalue!({"position": {"x": 1.0, "y": 2.0}})).unwrap();
    assert_eq!(
        pose,
        Pose2D {
            position: Point2D { x: 1.0, y: 2.0 },
            theta: 0.0,
            frame: "map".to_owned(),
            comment: None,
        }
    );
    let v = pose.to_value().unwrap();
    assert_eq!(
        v,
        jvalue!({"position": {"x": 1.0, "y": 2.0}, "theta": 0.0, "frame": "map", "comment": null})
    );
    assert_eq!(Pose2D::from_value(&v).unwrap(), pose);
    // 既定値のないフィールドが足りなければエラー
    assert!(Pose2D::from_value(&jvalue!({"theta": 1.0})).is_err());
}

#[test]
fn everything_round_trip() {
    let e = Everything::from_value(&jvalue!({"header": {"stamp": {}}})).unwrap();
    assert!(e.flag);
    assert_eq!(e.i, -1);
    assert_eq!(e.l, 9007199254740993);
    assert_eq!(e.f, 1.5);
    assert_eq!(e.data, vec![0u8, 1, 2]);
    assert_eq!(e.list, vec![1, 2]);
    assert_eq!(e.table.get("a"), Some(&1.0));
    assert_eq!(e.maybe, Some(3));
    assert_eq!(e.mode, None);
    assert_eq!(e.extra, jvalue!({"free": [1, "x"]}));
    assert_eq!(
        e.header,
        Header {
            frame_id: "map".to_owned(),
            stamp: Stamp { sec: 0, nanosec: 0 }
        }
    );
    assert_eq!(e.r#type, "t");
    assert_eq!(e.self_, 0);
    let v = e.to_value().unwrap();
    assert_eq!(v["data"], jvalue!("AAEC"));
    assert_eq!(v["type"], jvalue!("t"));
    assert_eq!(v["self"], jvalue!(0));
    assert_eq!(Everything::from_value(&v).unwrap(), e);
}

#[test]
fn enum_values() {
    assert_eq!(MotorMode::default(), MotorMode::Idle);
    assert_eq!(MotorMode::Velocity.to_value().unwrap(), jvalue!("Velocity"));
    assert_eq!(
        MotorMode::from_value(&jvalue!("Position")).unwrap(),
        MotorMode::Position
    );
    // 知らない記号は既定値になる
    assert_eq!(
        MotorMode::from_value(&jvalue!("Unknown")).unwrap(),
        MotorMode::Idle
    );
}

#[test]
fn processes_json_is_written() {
    let out_dir = env!("OUT_DIR");
    let s = std::fs::read_to_string(format!("{out_dir}/robot.geometry.processes.json")).unwrap();
    let v: Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["namespace"], "robot.geometry");
    let move_to = &v["processes"][0];
    assert_eq!(move_to["type_name"], "move_to");
    assert_eq!(move_to["description"], "指定の姿勢まで動く");
    assert_eq!(move_to["arguments"][0]["name"], "target");
    assert_eq!(move_to["arguments"][0]["type_name"], "Object");
    assert_eq!(
        move_to["arguments"][0]["schema"]["fingerprint"],
        jvalue!(Pose2D::type_schema().unwrap().fingerprint)
    );
    assert_eq!(move_to["arguments"][1]["default"], 1.0);
    assert_eq!(move_to["output_schema"]["name"], "robot.geometry.Pose2D");
    assert_eq!(v["processes"][1]["arguments"][0]["type_name"], "Image");
    let motor = &v["containers"][0];
    assert_eq!(motor["type_name"], "motor");
    assert_eq!(
        motor["container_processes"][1]["output_schema"]["name"],
        "null"
    );
}
