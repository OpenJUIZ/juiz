//! すでに発表済みの位置管理機能に関する実装途中のモジュール
//!

pub mod pose;

pub use pose::{
    euler_xyz_from_quaternion, quaternion_from_euler_xyz, Orientation3D, Point3D, Pose3D,
    Quaternion, Transform3D, Vec3,
};
