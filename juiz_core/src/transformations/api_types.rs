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

/// `container/pose_state` の引数 (§19.4)。取り込む側 (子の System) が相手の System に聞くときに使う
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct PoseStateQuery {
    /// P の ID か名前 (相手の System での core の ID)
    pub identifier: String,
    /// 前回受け取った `version`。同じなら `entries` を省いてもらう
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    /// 前回受け取った `system` (相手の UUID)。相手の今の UUID と違えば `since` は無視される
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// 取りに来た System の UUID (`container/profile_full` の `remote_children` に使う、§19.10)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscriber: Option<String>,
    /// 取りに来た System での終点の ID (§19.10)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destination: Vec<String>,
    /// 取りに来た側の timeout [秒]。`remote_children` から消すまでの時間 (§19.10)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
}

impl PoseStateQuery {
    pub fn new(identifier: &str) -> Self {
        Self {
            identifier: identifier.to_owned(),
            ..Default::default()
        }
    }

    /// BrokerProxy が相手に渡す param の組 (`destination` は `,` でつなぐ、§19.13)
    pub fn to_params(&self) -> Vec<(&'static str, String)> {
        let mut pairs = vec![("identifier", self.identifier.clone())];
        if let Some(v) = self.since {
            pairs.push(("since", v.to_string()));
        }
        if let Some(v) = self.system.as_ref() {
            pairs.push(("system", v.clone()));
        }
        if let Some(v) = self.subscriber.as_ref() {
            pairs.push(("subscriber", v.clone()));
        }
        if !self.destination.is_empty() {
            pairs.push(("destination", self.destination.join(",")));
        }
        if let Some(v) = self.timeout {
            pairs.push(("timeout", v.to_string()));
        }
        pairs
    }
}

/// `container/pose_state` の戻り値の `entries` の 1 項目 (§19.4)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PoseStateEntry {
    /// 相手の System での Anchor の ID (相手が写しとして持つ Anchor ならブローカー付きの ID)
    pub anchor: String,
    /// その Anchor を本当に持つ System の UUID
    pub origin_system: String,
    /// その Anchor を本当に持つ System での core の ID
    pub origin_anchor: String,
    pub pose: juiz_sdk::geometry::Pose,
    pub stamp: juiz_sdk::geometry::Time,
    pub version: u64,
}

/// `container/pose_state` の戻り値 (§19.4)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PoseStateResponse {
    pub identifier: String,
    /// 答えた System の UUID
    pub system: String,
    /// P の PoseState の版番号
    pub version: u64,
    /// `false` なら `entries` は空 (前回から変わっていない)
    pub changed: bool,
    #[serde(default)]
    pub entries: Vec<PoseStateEntry>,
}
