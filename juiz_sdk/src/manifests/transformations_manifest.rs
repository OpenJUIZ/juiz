//! Transformations の manifest (docs/spec/transformations.md §4・§5・§8.3・§11)
//!
//! - [`AnchorManifest`]: Anchor (特別な Container) の更新方式と周期。conf の `containers:` の
//!   `type_name: periodic_anchor` / `static_anchor` の項目から読む
//! - [`TransformManifest`]: conf の `transforms:` の 1 項目
//! - [`PoseListenerManifest`]: conf の `pose_listeners:` の 1 項目
//!
//! 名前の解決 (ネームスペース・type_name の省略) と木の規則の検査は core の登録所が行う (Step 2)。
//! ここでは 1 項目だけで分かる不備だけを検査する。

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::geometry::{Quaternion, Transform, Vector3};
use crate::prelude::*;

/// PeriodicExecutionAnchor の type_name
pub const PERIODIC_ANCHOR_TYPE_NAME: &str = "periodic_anchor";
/// StaticAnchor の type_name
pub const STATIC_ANCHOR_TYPE_NAME: &str = "static_anchor";
/// PeriodicExecutionAnchor の `rate` の既定値 [Hz] (§4.3)
pub const DEFAULT_ANCHOR_RATE: f64 = 10.0;
/// System をまたぐ Transform の取り込みの `rate` の既定値 [Hz] (§19.4)
pub const DEFAULT_REMOTE_RATE: f64 = 10.0;
/// System をまたぐ Transform の取り込みの `timeout` の既定値 [秒] (§19.4・§19.6)
pub const DEFAULT_REMOTE_TIMEOUT: f64 = 3.0;

/// Anchor の更新方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorKind {
    /// 自分の周期で木を更新する (`periodic_anchor`)
    Periodic,
    /// 作られたとき・木が変わったとき・頼まれたときだけ更新する (`static_anchor`)
    Static,
}

impl AnchorKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            AnchorKind::Periodic => PERIODIC_ANCHOR_TYPE_NAME,
            AnchorKind::Static => STATIC_ANCHOR_TYPE_NAME,
        }
    }

    /// type_name から。Anchor の type_name でなければ `None`
    pub fn from_type_name(type_name: &str) -> Option<Self> {
        match type_name {
            PERIODIC_ANCHOR_TYPE_NAME => Some(AnchorKind::Periodic),
            STATIC_ANCHOR_TYPE_NAME => Some(AnchorKind::Static),
            _ => None,
        }
    }

    /// `profile` に載せる名前 (`periodic` / `static`)
    pub fn as_str(&self) -> &'static str {
        match self {
            AnchorKind::Periodic => "periodic",
            AnchorKind::Static => "static",
        }
    }
}

/// type_name が Anchor のものか
pub fn is_anchor_type_name(type_name: &str) -> bool {
    AnchorKind::from_type_name(type_name).is_some()
}

/// Anchor の設定 (§4)。Periodic なら周期 `rate` [Hz] を持つ
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnchorManifest {
    pub kind: AnchorKind,
    /// 周期 [Hz]。Periodic だけが持つ
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
}

fn container_manifest_invalid(message: String) -> anyhow::Error {
    anyhow!(JuizError::ContainerManifestInvalidError { message })
}

impl AnchorManifest {
    /// PeriodicExecutionAnchor。`rate` は正の有限の数でなければエラー
    pub fn periodic(rate: f64) -> JuizResult<Self> {
        if !(rate.is_finite() && rate > 0.0) {
            return Err(container_manifest_invalid(format!(
                "rate of {PERIODIC_ANCHOR_TYPE_NAME} must be a positive number, but {rate}"
            )));
        }
        Ok(Self {
            kind: AnchorKind::Periodic,
            rate: Some(rate),
        })
    }

    /// StaticAnchor
    pub fn static_anchor() -> Self {
        Self {
            kind: AnchorKind::Static,
            rate: None,
        }
    }

    /// Container manifest から読む。Anchor の type_name でなければ `Ok(None)`。
    ///
    /// - Periodic の `rate` は省略時 [`DEFAULT_ANCHOR_RATE`]。0 以下・NaN・無限大はエラー
    /// - Anchor には ContainerProcess を付けられない (`processes:` があればエラー、§4.1)
    /// - Static に `rate` を書いても使わない (WARN を出す)
    pub fn from_container_manifest(manifest: &ContainerManifest) -> JuizResult<Option<Self>> {
        let Some(kind) = AnchorKind::from_type_name(&manifest.type_name) else {
            return Ok(None);
        };
        if !manifest.processes.is_empty() {
            return Err(container_manifest_invalid(format!(
                "Anchor ({}) can not have processes.",
                manifest.type_name
            )));
        }
        match kind {
            AnchorKind::Periodic => Ok(Some(Self::periodic(
                manifest.rate.unwrap_or(DEFAULT_ANCHOR_RATE),
            )?)),
            AnchorKind::Static => {
                if manifest.rate.is_some() {
                    log::warn!(
                        "rate of {STATIC_ANCHOR_TYPE_NAME} ({:?}) is ignored.",
                        manifest.name
                    );
                }
                Ok(Some(Self::static_anchor()))
            }
        }
    }

    /// 更新の間隔。Static なら `None`
    pub fn period(&self) -> Option<std::time::Duration> {
        self.rate
            .map(|r| std::time::Duration::from_secs_f64(1.0 / r))
    }

    /// `list --long`・`profile_full` に足す `anchor` の値 `{"kind": "periodic" | "static", "rate": <Hz>}` (§4.1)
    pub fn to_profile_value(&self) -> Value {
        match self.rate {
            Some(rate) => jvalue!({"kind": self.kind.as_str(), "rate": rate}),
            None => jvalue!({"kind": self.kind.as_str()}),
        }
    }
}

/// Transform の ID `transform://<source の ID>|<destination の ID>` (§5.1)
pub fn transform_identifier(source_id: &str, destination_id: &str) -> String {
    format!("transform://{source_id}|{destination_id}")
}

/// Pose 通知の ID `pose_listener://<X の ID>|<Y の ID>|<arg>` (§8.3)
pub fn pose_listener_identifier(container_id: &str, process_id: &str, arg: &str) -> String {
    format!("pose_listener://{container_id}|{process_id}|{arg}")
}

fn transform_manifest_invalid(message: String) -> anyhow::Error {
    anyhow!(JuizError::TransformManifestInvalidError { message })
}

/// conf の `transforms:` の 1 項目 (§11)。`source` (親) から見た `destination` (子) の位置姿勢を表す。
/// `translation` / `rotation` は省略時それぞれ 0 と回転なし。キーは `juiz_std.geometry.Transform` と同じ
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformManifest {
    /// 始点 (親) の Container の ID か名前
    pub source: String,
    /// 終点 (子) の Container の ID か名前
    pub destination: String,
    #[serde(default)]
    pub translation: Vector3,
    #[serde(default)]
    pub rotation: Quaternion,
    /// System をまたぐ Transform の取り込みの周期 [Hz] (§19.4)。省略時 [`DEFAULT_REMOTE_RATE`]。
    /// またぐ Transform でなければ使わない (WARN)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
    /// System をまたぐ Transform の取り込みが途絶えてから写しを消すまでの時間 [秒] (§19.6)。
    /// 省略時 [`DEFAULT_REMOTE_TIMEOUT`]。またぐ Transform でなければ使わない (WARN)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
    /// 別 System から回された作成 (§19.2 の 5) のとき、始点の Container を持つ System の UUID。
    /// `source` はその System での core の ID で、受けた System がその System へのブローカーで書き換える
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,
}

impl TransformManifest {
    pub fn new(source: &str, destination: &str) -> Self {
        Self {
            source: source.to_owned(),
            destination: destination.to_owned(),
            translation: Vector3::zero(),
            rotation: Quaternion::identity(),
            rate: None,
            timeout: None,
            source_system: None,
        }
    }

    /// 取り込みの周期 [Hz] (System をまたぐ Transform だけ、§19.4)
    pub fn rate(mut self, rate: f64) -> Self {
        self.rate = Some(rate);
        self
    }

    /// 取り込みの timeout [秒] (System をまたぐ Transform だけ、§19.6)
    pub fn timeout(mut self, timeout: f64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn translation(mut self, translation: Vector3) -> Self {
        self.translation = translation;
        self
    }

    pub fn rotation(mut self, rotation: Quaternion) -> Self {
        self.rotation = rotation;
        self
    }

    /// Transform の値
    pub fn transform(&self) -> Transform {
        Transform::new(self.translation, self.rotation)
    }

    /// 1 項目だけで分かる不備を検査し、回転を正規化したものを返す。
    /// 端点が空・同じ名前は `TransformManifestInvalidError`、値の不備は `InvalidPoseValueError`。
    /// 名前の解決後に同じ Container を指していないかは core が改めて検査する
    pub fn validated(&self) -> JuizResult<Self> {
        if self.source.is_empty() || self.destination.is_empty() {
            return Err(transform_manifest_invalid(format!(
                "source and destination are required: {self:?}"
            )));
        }
        if self.source == self.destination {
            return Err(transform_manifest_invalid(format!(
                "source and destination must be different Containers: {}",
                self.source
            )));
        }
        for (key, v) in [("rate", self.rate), ("timeout", self.timeout)] {
            if let Some(v) = v {
                if !(v.is_finite() && v > 0.0) {
                    return Err(transform_manifest_invalid(format!(
                        "{key} must be a positive number, but {v}: {self:?}"
                    )));
                }
            }
        }
        let t = self.transform().normalized()?;
        Ok(Self {
            translation: t.translation,
            rotation: t.rotation,
            ..self.clone()
        })
    }

    /// conf 全体から `transforms:` を読む (なければ空)。各項目は [`TransformManifest::validated`] 済み
    pub fn list_from_conf(conf: &Value) -> JuizResult<Vec<Self>> {
        list_from_conf(conf, "transforms", |v| {
            Self::deserialize(v)
                .map_err(|e| transform_manifest_invalid(format!("{v}: {e}")))?
                .validated()
        })
    }
}

impl TryFrom<&Value> for TransformManifest {
    type Error = anyhow::Error;
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        Self::deserialize(value)
            .map_err(|e| transform_manifest_invalid(format!("{value}: {e}")))?
            .validated()
    }
}

impl TryFrom<Value> for TransformManifest {
    type Error = anyhow::Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl From<TransformManifest> for Value {
    fn from(m: TransformManifest) -> Value {
        serde_json::to_value(m).expect("TransformManifest always serializes")
    }
}

fn pose_listener_invalid(message: String) -> anyhow::Error {
    anyhow!(JuizError::PoseListenerError { message })
}

/// conf の `pose_listeners:` の 1 項目 (§8.3)。
/// 「Container `container` の Pose が書かれたら Process `process` を呼ぶ」という登録
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseListenerManifest {
    /// 見張る Container の ID か名前
    pub container: String,
    /// 呼ぶ Process の ID か名前 (ContainerProcess も可)
    pub process: String,
    /// Process の引数の名前。引数が 1 つなら省略可 (core が埋める)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<String>,
    /// 渡す値の基準 Container。省略時は書き込みで指定された基準
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Anchor の更新で値が変わったときも呼ぶか。`true` なら `reference` が必須
    #[serde(default)]
    pub on_update: bool,
}

impl PoseListenerManifest {
    pub fn new(container: &str, process: &str) -> Self {
        Self {
            container: container.to_owned(),
            process: process.to_owned(),
            arg: None,
            reference: None,
            on_update: false,
        }
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.arg = Some(arg.to_owned());
        self
    }

    pub fn reference(mut self, reference: &str) -> Self {
        self.reference = Some(reference.to_owned());
        self
    }

    pub fn on_update(mut self, on_update: bool) -> Self {
        self.on_update = on_update;
        self
    }

    /// 1 項目だけで分かる不備の検査 (`PoseListenerError`)。
    /// 引数の数・型の検査は Process が分かる core が行う
    pub fn validated(self) -> JuizResult<Self> {
        if self.container.is_empty() || self.process.is_empty() {
            return Err(pose_listener_invalid(format!(
                "container and process are required: {self:?}"
            )));
        }
        if matches!(&self.arg, Some(a) if a.is_empty()) {
            return Err(pose_listener_invalid(format!(
                "arg must not be empty: {self:?}"
            )));
        }
        if self.on_update && self.reference.as_deref().map_or(true, str::is_empty) {
            return Err(pose_listener_invalid(format!(
                "reference is required when on_update is true: {self:?}"
            )));
        }
        Ok(self)
    }

    /// conf 全体から `pose_listeners:` を読む (なければ空)。各項目は検査済み
    pub fn list_from_conf(conf: &Value) -> JuizResult<Vec<Self>> {
        list_from_conf(conf, "pose_listeners", |v| {
            Self::deserialize(v)
                .map_err(|e| pose_listener_invalid(format!("{v}: {e}")))?
                .validated()
        })
    }
}

impl TryFrom<&Value> for PoseListenerManifest {
    type Error = anyhow::Error;
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        Self::deserialize(value)
            .map_err(|e| pose_listener_invalid(format!("{value}: {e}")))?
            .validated()
    }
}

impl TryFrom<Value> for PoseListenerManifest {
    type Error = anyhow::Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl From<PoseListenerManifest> for Value {
    fn from(m: PoseListenerManifest) -> Value {
        serde_json::to_value(m).expect("PoseListenerManifest always serializes")
    }
}

fn list_from_conf<T>(
    conf: &Value,
    key: &str,
    f: impl Fn(&Value) -> JuizResult<T>,
) -> JuizResult<Vec<T>> {
    match conf.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items.iter().map(f).collect(),
        Some(other) => Err(anyhow!(JuizError::ValueIsNotArrayError {
            value: other.clone()
        })),
    }
}
