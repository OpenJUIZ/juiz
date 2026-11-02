//! juiz_std_types の型 (issue #199)

use juiz_sdk::prelude::*;
use juiz_sdk::types::JuizType;
use juiz_std_types::common::{Header, Time};
use juiz_std_types::geometry::{
    Point2D, Pose, Pose2D, Quaternion, Transform, TransformStamped, Twist2D, Vector2, Vector3,
};
use juiz_std_types::sensor::{ImageInfo, LaserScan};

#[test]
fn values_round_trip() {
    let pose = Pose2D {
        position: Point2D { x: 1.0, y: -2.0 },
        theta: 0.5,
    };
    let v = pose.to_value().unwrap();
    assert_eq!(
        v,
        jvalue!({"position": {"x": 1.0, "y": -2.0}, "theta": 0.5})
    );
    assert_eq!(Pose2D::from_value(&v).unwrap(), pose);

    let scan = LaserScan::from_value(&jvalue!({"ranges": [1.0, 2.5]})).unwrap();
    assert_eq!(scan.ranges, vec![1.0, 2.5]);
    assert_eq!(scan.header.frame_id, "");
}

#[test]
fn every_field_has_a_default() {
    let pose = Pose::from_value(&jvalue!({})).unwrap();
    assert_eq!(
        pose.orientation,
        Quaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0
        }
    );
    let info = ImageInfo::from_value(&jvalue!({"width": 640, "height": 480})).unwrap();
    assert_eq!(info.encoding, "rgb8");
    assert_eq!(
        info.header,
        Header {
            stamp: Time { sec: 0, nanosec: 0 },
            frame_id: String::new()
        }
    );
    assert_eq!(
        Twist2D::from_value(&jvalue!({"angular": 1.0})).unwrap(),
        Twist2D {
            linear: Vector2 { x: 0.0, y: 0.0 },
            angular: 1.0
        }
    );
}

/// Transform / TransformStamped は ROS 2 の geometry_msgs と同じ辞書の形 (issue #367)
#[test]
fn transform_has_ros2_shape_and_defaults() {
    let t = TransformStamped::from_value(&jvalue!({
        "header": {"frame_id": "base0"},
        "child_frame_id": "pantilt0",
        "transform": {"translation": {"x": 0.1, "z": 0.3}}
    }))
    .unwrap();
    assert_eq!(t.header.frame_id, "base0");
    assert_eq!(t.child_frame_id, "pantilt0");
    assert_eq!(
        t.transform,
        Transform {
            translation: Vector3 {
                x: 0.1,
                y: 0.0,
                z: 0.3
            },
            rotation: Quaternion {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0
            }
        }
    );
    let v = t.transform.to_value().unwrap();
    assert_eq!(
        v,
        jvalue!({"translation": {"x": 0.1, "y": 0.0, "z": 0.3}, "rotation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}})
    );
    assert_eq!(Transform::from_value(&v).unwrap(), t.transform);
}
