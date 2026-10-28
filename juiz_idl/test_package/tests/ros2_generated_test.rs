//! ROS 2 の型から取り込んだ JIDL (idl/ros2) で生成した Rust のコードのテスト (issue #201)

use juiz_idl_test_package::juiz_std::geometry::Pose;
use juiz_idl_test_package::ros2::demo_msgs::msg::{Mapped, Reading, Sample};
use juiz_idl_test_package::ros2::demo_msgs::srv::{AddTwoInts_Request, AddTwoInts_Response};
use juiz_sdk::prelude::*;

#[test]
fn defaults_and_constants() {
    let s = Sample::from_value(&jvalue!({})).unwrap();
    assert_eq!(s.small, -5);
    assert_eq!(s.fixed, vec![1, 2, 3]);
    assert_eq!(s.label, "abc");
    assert_eq!(s.pair, vec![0u8, 0u8]);
    assert!(s.flag);
    assert_eq!(s.pose, Pose::from_value(&jvalue!({})).unwrap());
    assert_eq!(Sample::MODE_A, 1);
    assert_eq!(Sample::NAME_PREFIX, "demo_");
    assert_eq!(Sample::SCALE, 0.5);
    assert_eq!(Reading::UNIT, "m");
    let r = Reading::from_value(&jvalue!({"value": 2.0})).unwrap();
    assert_eq!(r.covariance, vec![0.0; 9]);
    assert_eq!(r.name, "hello");
}

#[test]
fn constraints_are_checked_when_reading() {
    // 生成コードの from_value は型で読むので、属性の制約にも合わなければエラー
    assert!(Sample::from_value(&jvalue!({"label": "01234567890"})).is_err());
    assert!(Sample::from_value(&jvalue!({"fixed": [1]})).is_err());
    assert!(Sample::from_value(&jvalue!({"small": 1000})).is_ok());
}

#[test]
fn standard_messages_use_juiz_std() {
    let m = Mapped::from_value(
        &jvalue!({"imu": {"orientation_covariance": [-1.0, 0, 0, 0, 0, 0, 0, 0, 0]}}),
    )
    .unwrap();
    assert_eq!(m.imu.orientation_covariance[0], -1.0);
    assert_eq!(m.range.radiation_type, 0);
    let v = m.to_value().unwrap();
    assert_eq!(v["scan"]["intensities"], jvalue!([]));
}

#[test]
fn service_types() {
    let req = AddTwoInts_Request::from_value(&jvalue!({"a": 1})).unwrap();
    assert_eq!((req.a, req.b), (1, 2));
    let resp = AddTwoInts_Response { sum: 3 };
    assert_eq!(resp.to_value().unwrap(), jvalue!({"sum": 3}));
}
