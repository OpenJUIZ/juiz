//! Transformations の API の引数・戻り値の型 (issue #370、仕様 docs/spec/transformations.md §13.1)
//!
//! 値の形は `juiz_std.geometry` の型。型の登録所 (`type/list`)・OpenAPI の `components.schemas` に載せ、
//! API で受け取った値はここの型で検査する (データ型の検査と同じ規則、datatype.md §4)。

use juiz_sdk::types::{type_error, JuizType, TypeSchema};
use juiz_std_types::geometry;

use crate::prelude::*;

/// 型の登録所に載せるときの「使っている側」の名前
pub const TRANSFORMATIONS_TYPE_USER: &str = "transformations";

/// API で使う `juiz_std.geometry` の型 (Pose・PoseStamped・Transform・TransformStamped)
pub fn transformations_type_schemas() -> JuizResult<Vec<TypeSchema>> {
    Ok(vec![
        geometry::Pose::type_schema()?,
        geometry::PoseStamped::type_schema()?,
        geometry::Transform::type_schema()?,
        geometry::TransformStamped::type_schema()?,
    ])
}

fn check<T: JuizType>(what: &str, value: &Value) -> JuizResult<()> {
    let schema = T::type_schema()?;
    schema
        .check_value(value)
        .map_err(type_error)
        .map_err(|e| e.context(format!("{what} must be {} ({value})", schema.name)))
}

/// `juiz_std.geometry.Pose` の値か検査する
pub fn check_pose_value(value: &Value) -> JuizResult<()> {
    check::<geometry::Pose>("pose", value)
}

/// `juiz_std.geometry.Vector3` の値か検査する
pub fn check_vector3_value(what: &str, value: &Value) -> JuizResult<()> {
    check::<geometry::Vector3>(what, value)
}

/// `juiz_std.geometry.Quaternion` の値か検査する
pub fn check_quaternion_value(what: &str, value: &Value) -> JuizResult<()> {
    check::<geometry::Quaternion>(what, value)
}
