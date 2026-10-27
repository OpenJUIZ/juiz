//! juiz_std_types の型 (issue #199)

use juiz_sdk::prelude::*;
use juiz_sdk::types::JuizType;
use juiz_std_types::common::{Header, Time};
use juiz_std_types::geometry::{Point2D, Pose, Pose2D, Quaternion, Twist2D, Vector2};
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
