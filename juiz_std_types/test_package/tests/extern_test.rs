//! extern_path で参照した型は juiz_std_types の型そのもの (issue #199)

use juiz_sdk::prelude::*;
use juiz_sdk::types::JuizType;
use juiz_std_types::geometry::{Point2D, Pose2D};
use juiz_std_types_test_package::my::robot::Goal;

#[test]
fn extern_types_are_the_std_types() {
    let goal = Goal::from_value(&jvalue!({"pose": {"position": {"x": 1.0, "y": 2.0}}})).unwrap();
    // 型が juiz_std_types::geometry::Pose2D なので、そのまま比べられる
    let pose: Pose2D = goal.pose.clone();
    assert_eq!(
        pose,
        Pose2D {
            position: Point2D { x: 1.0, y: 2.0 },
            theta: 0.0
        }
    );
    assert_eq!(goal.header.frame_id, "map");
    assert_eq!(Goal::from_value(&goal.to_value().unwrap()).unwrap(), goal);
    assert_eq!(Goal::type_schema().unwrap().name, "my.robot.Goal");
}

/// 生成したモジュールの木に juiz_std は入らない
#[test]
fn std_types_are_not_generated() {
    let tree = std::fs::read_to_string(concat!(env!("OUT_DIR"), "/juiz_idl.rs")).unwrap();
    assert!(!tree.contains("juiz_std"), "{tree}");
    assert!(!std::path::Path::new(concat!(env!("OUT_DIR"), "/juiz_std.geometry.rs")).exists());
}
