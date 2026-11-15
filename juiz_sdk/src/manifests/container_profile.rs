use std::collections::BTreeMap;
use std::fmt::Display;
use std::ops::{Deref, DerefMut};

use anyhow::{anyhow, Context};
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

use super::{ArgumentProfile, ProcessProfile};
use crate::prelude::*;

/// Container の profile。インスタンスの manifest ([`ContainerManifest`]) と実行時の状態を合わせたもの
/// (docs/spec/manifest-profile.md §3・§4)。
///
/// JSON は manifest のキーに状態のキーを重ねた形 (仕様 §8)。`processes` と `model` は、
/// manifest の宣言ではなく状態 (今ある ContainerProcess の profile、読み直したモデル情報) を出す。
/// manifest の項目は [`Deref`] で読める (`profile.type_name` など)。`processes` と `model` は
/// このフィールドの方が先に見えるので、manifest の宣言は `profile.manifest.processes` で読む。
#[derive(Debug, Clone)]
pub struct ContainerProfile {
    /// インスタンスの manifest。`name` は必ず `Some`
    pub manifest: ContainerManifest,
    /// この Container に属する ContainerProcess の profile
    pub processes: Vec<ProcessProfile>,
    /// CompositeContainer のスロット名 → 参照先の Container の ID (issue #168)。
    /// CompositeContainer 以外では `None` で、出力しない
    pub slots: Option<BTreeMap<String, String>>,
    /// Anchor の種類と周期 `{"kind": "periodic" | "static", "rate": <Hz>}`
    /// (docs/spec/transformations.md §4.1)。Anchor 以外では `None` で、出力しない
    pub anchor: Option<Value>,
    /// この Container の写しを取りに来ている別 System `[{system, destinations, last_access}]`
    /// (docs/spec/transformations.md §19.10)。いなければ `None` で、出力しない
    pub remote_children: Option<Value>,
    /// 3D モデルのモデル情報 (`juiz_std.model.ContainerModelInfo` の形、docs/spec/container-model.md §7・§8)。
    /// モデルが無ければ `null`
    pub model: Option<Value>,
}

impl Deref for ContainerProfile {
    type Target = ContainerManifest;
    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

impl DerefMut for ContainerProfile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manifest
    }
}

/// 状態のキー。直列化では manifest の同じキーを上書きし、読むときは manifest から外す
const STATE_KEYS: [&str; 5] = ["processes", "slots", "anchor", "remote_children", "model"];

/// manifest にはあるが、今の profile の JSON に無いキー。JSON の形を変えないために出さない
/// (足すかは別の issue で決める、仕様 §3 の 4・§8)
const HIDDEN_MANIFEST_KEYS: [&str; 3] = ["broker_type_name", "broker_name", "rate"];

impl Serialize for ContainerProfile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::to_value(&self.manifest).map_err(ser::Error::custom)?;
        let map = value
            .as_object_mut()
            .ok_or_else(|| ser::Error::custom("ContainerManifest must be serialized as an object"))?;
        for key in STATE_KEYS.iter().chain(HIDDEN_MANIFEST_KEYS.iter()) {
            map.remove(*key);
        }
        let processes = serde_json::to_value(&self.processes).map_err(ser::Error::custom)?;
        map.insert("processes".to_owned(), processes);
        if let Some(slots) = self.slots.as_ref() {
            map.insert(
                "slots".to_owned(),
                serde_json::to_value(slots).map_err(ser::Error::custom)?,
            );
        }
        if let Some(anchor) = self.anchor.as_ref() {
            map.insert("anchor".to_owned(), anchor.clone());
        }
        if let Some(rc) = self.remote_children.as_ref() {
            map.insert("remote_children".to_owned(), rc.clone());
        }
        map.insert(
            "model".to_owned(),
            self.model.clone().unwrap_or(Value::Null),
        );
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ContainerProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let map = value
            .as_object_mut()
            .ok_or_else(|| de::Error::custom("ContainerProfile must be an object"))?;
        let mut take = |key: &str| map.remove(key).filter(|v| !v.is_null());
        let processes: Vec<ProcessProfile> = match take("processes") {
            Some(v) => serde_json::from_value(v).map_err(de::Error::custom)?,
            None => Vec::new(),
        };
        let slots = match take("slots") {
            Some(v) => Some(serde_json::from_value(v).map_err(de::Error::custom)?),
            None => None,
        };
        let anchor = take("anchor");
        let remote_children = take("remote_children");
        let model = take("model");
        let mut manifest: ContainerManifest =
            serde_json::from_value(value).map_err(de::Error::custom)?;
        if manifest.name.is_none() {
            return Err(de::Error::custom(format!(
                "ContainerProfile needs 'name' (type_name={}).",
                manifest.type_name
            )));
        }
        manifest.processes = processes.iter().map(|p| p.manifest.clone()).collect();
        Ok(Self {
            manifest,
            processes,
            slots,
            anchor,
            remote_children,
            model,
        })
    }
}

impl Display for ContainerProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "ContainerProfile(\"{}\", \"{}\", processes=[",
            self.type_name, self.language,
        ))?;
        for p in self.processes.iter() {
            f.write_fmt(format_args!("{}, ", p))?;
        }
        f.write_str("])")?;

        Ok(())
    }
}

impl TryFrom<ContainerManifest> for ContainerProfile {
    /// 状態を空にした profile を作る。`processes` には manifest に書いた ContainerProcess を
    /// (状態なしの profile にして) 入れる。インスタンスの manifest でなければ (名前が無ければ) エラー
    fn try_from(value: ContainerManifest) -> Result<Self, Self::Error> {
        let name = value
            .name
            .clone()
            .ok_or(anyhow!(JuizError::ContainerManifestInvalidError {
                message: format!(
                    "ContainerManifest (type_name={}) does not include 'name' value.",
                    value.type_name
                )
            }))?;
        let processes = value
            .processes
            .iter()
            .map(|p| -> JuizResult<ProcessProfile> { p.clone().try_into() })
            .collect::<JuizResult<Vec<ProcessProfile>>>()
            .with_context(|| {
                format!(
                    "ContainerProfile::try_from(ContainerManifest(type_name={}, name={})) failed. Each process in 'processes' must have a name.",
                    value.type_name, name
                )
            })?;
        Ok(Self {
            manifest: value,
            processes,
            slots: None,
            anchor: None,
            remote_children: None,
            model: None,
        })
    }

    type Error = anyhow::Error;
}

impl From<ContainerProfile> for ContainerManifest {
    fn from(value: ContainerProfile) -> Self {
        value.manifest
    }
}

impl ContainerProfile {
    pub fn new(type_name: &str, name: &str) -> Self {
        Self::from_manifest_unchecked(ContainerManifest::new(type_name).name(name))
    }

    fn from_manifest_unchecked(manifest: ContainerManifest) -> Self {
        Self {
            manifest,
            processes: Vec::new(),
            slots: None,
            anchor: None,
            remote_children: None,
            model: None,
        }
    }

    /// インスタンスの名前 (manifest の `name`)
    pub fn instance_name(&self) -> &str {
        self.manifest.name.as_deref().unwrap_or("")
    }

    /// インスタンスの manifest を取り出す
    pub fn into_manifest(self) -> ContainerManifest {
        self.manifest
    }

    pub fn parent_container_profile(&self) -> Option<Self> {
        self.parent_name.as_ref().map(|pn| {
            let mut m = ContainerManifest::new(self.parent_type_name.as_ref().unwrap())
                .name(pn)
                .namespace(&self.namespace)
                .factory("component_factory");
            m.broker_name = self.broker_name.clone();
            m.broker_type_name = self.broker_type_name.clone();
            Self::from_manifest_unchecked(m)
        })
    }

    pub fn name(mut self, name: &str) -> Self {
        self.manifest.name = Some(name.to_owned());
        self
    }

    pub fn language(mut self, language: &str) -> Self {
        self.language = language.to_owned();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.into();
        self
    }

    pub fn parent_type_name(mut self, parent_type_name: &str) -> Self {
        self.parent_type_name = Some(parent_type_name.to_owned());
        self
    }

    pub fn parent_name(mut self, parent_name: &str) -> Self {
        self.parent_name = Some(parent_name.to_owned());
        self
    }

    pub fn add_process(mut self, process_profile: ProcessProfile) -> Self {
        let p = process_profile
            .container_name(self.manifest.name.clone())
            .container_type(Some(self.type_name.clone()));
        self.manifest.processes.push(p.manifest.clone());
        self.processes.push(p);
        self
    }

    pub fn factory(mut self, factory: &str) -> Self {
        self.factory = factory.to_owned();
        self
    }

    pub fn add_arg(mut self, arg: ArgumentProfile) -> Self {
        self.manifest.arguments.push(arg.into_manifest());
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessManifest::new("hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Int");
    /// ```
    pub fn add_int_arg(self, name: &str, description: &str, default: i64) -> Self {
        self.add_arg(ArgumentProfile::new_int(name, default).description(description))
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessProfile::new("hoge01", "hoge_type")
    ///   .description("hoge manifest")
    ///   .add_float_arg("arg1", "float_arg", 1.0.into());
    /// assert_eq!(manifest.arguments[0].name, "arg1");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Float");
    /// ```
    pub fn add_float_arg(self, name: &str, description: &str, default: f64) -> Self {
        self.add_arg(ArgumentProfile::new_float(name, default).description(description))
    }

    pub fn add_object_arg(self, name: &str, description: &str, default: Value) -> Self {
        self.add_arg(ArgumentProfile::new_object(name, default).description(description))
    }

    pub fn add_string_arg(self, name: &str, description: &str, default: &str) -> Self {
        self.add_arg(ArgumentProfile::new_string(name, default).description(description))
    }

    pub fn add_image_arg(self, name: &str, description: &str) -> Self {
        self.add_arg(ArgumentProfile::new_image(name).description(description))
    }

    pub fn identifier(&self) -> ContainerIdentifier {
        ContainerIdentifier::new(
            format!("core"),
            format!("core"),
            self.instance_name().to_owned(),
            self.type_name.clone(),
        )
        .with_namespace(&self.namespace)
    }
}
