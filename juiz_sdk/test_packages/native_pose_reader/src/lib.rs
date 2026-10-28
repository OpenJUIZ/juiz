//! 遅延評価 L2 (#176) のテスト用 Process (juiz_core/tests/integration/rust_native_test.rs)
//!
//! native_pose_component と同じ形の Pose2D を、別のプラグインで受け取る。
//! ABI タグが違うので、辞書を通して受け取る。

use juiz_sdk::prelude::*;

#[derive(Clone, Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
pub struct Pose2D {
    pub position: Point2D,
    #[juiz(default = 0.0)]
    pub theta: f64,
}

#[juiz_process(arguments = { default = { pose = { position = { x = 0.0, y = 0.0 } } } })]
fn native_pose_reader(pose: Pose2D) -> JuizResult<f64> {
    Ok(pose.position.x)
}
