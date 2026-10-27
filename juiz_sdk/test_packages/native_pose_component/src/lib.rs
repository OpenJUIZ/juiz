//! 遅延評価 L2 (#176) のテスト用コンポーネント (juiz_core/tests/rust_native_test.rs)
//!
//! Pose2D を作る・動かす Process と、Pose2D を clone した回数を返す Process を持つ。
//! 同じプラグインの中なので、Pose2D はネイティブ値のまま渡る。

use std::sync::atomic::{AtomicI64, Ordering};

use juiz_sdk::prelude::*;

static CLONES: AtomicI64 = AtomicI64::new(0);

#[derive(Clone, Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, PartialEq, JuizType)]
#[juiz(namespace = "robot.geometry")]
pub struct Pose2D {
    pub position: Point2D,
    #[juiz(default = 0.0)]
    pub theta: f64,
}

impl Clone for Pose2D {
    fn clone(&self) -> Self {
        CLONES.fetch_add(1, Ordering::SeqCst);
        Pose2D {
            position: self.position.clone(),
            theta: self.theta,
        }
    }
}

#[juiz_component_process(arguments = { default = { x = 0.0 } })]
fn native_pose_make(x: f64) -> JuizResult<Pose2D> {
    Ok(Pose2D {
        position: Point2D { x, y: 0.0 },
        theta: 0.0,
    })
}

#[juiz_component_process(
    arguments = {
        default = {
            pose = { position = { x = 0.0, y = 0.0 } }
            dx = 1.0
        }
    }
)]
fn native_pose_move(pose: Pose2D, dx: f64) -> JuizResult<Pose2D> {
    let mut pose = pose;
    pose.position.x += dx;
    Ok(pose)
}

/// clone した回数を返して 0 に戻す
#[juiz_component_process]
fn native_pose_clones() -> JuizResult<i64> {
    Ok(CLONES.swap(0, Ordering::SeqCst))
}

juiz_component_manifest!(
    component_name = "native_pose_component"
    processes = [
        native_pose_make,
        native_pose_move,
        native_pose_clones
    ]
);
