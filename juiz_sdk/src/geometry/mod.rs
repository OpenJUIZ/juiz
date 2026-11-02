//! 位置姿勢と座標変換 (Transformations) の値と計算 (docs/spec/transformations.md §3)
//!
//! 値の形は `juiz_std.geometry` の型に合わせてある。`juiz_sdk` は `juiz_std_types` に依存できないので、
//! 同じ辞書の形の型をここに持ち、`Value` (辞書) で行き来する。
//!
//! ```
//! use juiz_sdk::geometry::{Pose, Quaternion, Transform, Vector3};
//! // map → base0 (z 軸まわり 90°) → camera0
//! let map_base0 = Transform::new(Vector3::new(1.0, 0.0, 0.0),
//!     Quaternion::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), std::f64::consts::FRAC_PI_2).unwrap());
//! let base0_camera0 = Transform::new(Vector3::new(0.1, 0.0, 0.3), Quaternion::identity());
//! let pose_map_camera0 = Pose::identity() * map_base0 * base0_camera0;
//! assert!((pose_map_camera0.position.y - 0.1).abs() < 1e-9);
//! ```

mod pose;
mod stamped;

pub use pose::{Point3D, Pose, Quaternion, Transform, Vector3, QUATERNION_NORM_EPSILON};
pub use stamped::{Header, PoseStamped, Time, TransformStamped};
