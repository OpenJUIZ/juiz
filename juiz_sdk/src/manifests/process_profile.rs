use super::{ArgumentProfile, TopicManifest};
use crate::{
    manifests::OutputProfile,
    prelude::*,
    process_identifier::ProcessIdentifier,
};
use anyhow::anyhow;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::BTreeMap,
    fmt::Display,
    ops::{Deref, DerefMut},
};

/// Process の profile。インスタンスの manifest ([`ProcessManifest`]) と実行時の状態を合わせたもの
/// (docs/spec/manifest-profile.md §3・§4)。
///
/// 実行時の状態は引数ごとの今の値 (拘束値か push で届いた値、issue #112) で、
/// JSON では `arguments` の各要素の `value` に出る。状態が引数の要素の中にあるので、
/// `#[serde(flatten)]` だけでは書けず、manifest の JSON に `value` を差し込む直列化を手で書いている
/// (仕様 §8)。JSON の形は manifest のキー + `arguments[].value`。
///
/// manifest の項目は [`Deref`] で読める (`profile.type_name` など)。名前は manifest では
/// `Option<String>` なので、文字列で欲しいときは [`ProcessProfile::instance_name`] を使う。
#[derive(Clone, Debug)]
pub struct ProcessProfile {
    /// インスタンスの manifest。`name` は必ず `Some`
    pub manifest: ProcessManifest,
    /// 引数名 → 次の実行で使われる値。値が無い引数は入れない
    pub argument_values: BTreeMap<String, Value>,
}

impl Deref for ProcessProfile {
    type Target = ProcessManifest;
    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

impl DerefMut for ProcessProfile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manifest
    }
}

impl TryFrom<ProcessManifest> for ProcessProfile {
    type Error = anyhow::Error;

    /// 状態を空にした profile を作る。インスタンスの manifest でなければ (名前が無ければ) エラー
    fn try_from(value: ProcessManifest) -> Result<Self, Self::Error> {
        if value.name.is_none() {
            return Err(anyhow!(JuizError::InvalidArgumentError {
                message: format!(
                    "Invalid ProcessProfile passed. name is none (type_name={}).",
                    value.type_name
                )
            }));
        }
        Ok(Self {
            manifest: value,
            argument_values: BTreeMap::new(),
        })
    }
}

impl From<ProcessProfile> for ProcessManifest {
    fn from(value: ProcessProfile) -> Self {
        value.manifest
    }
}

/// manifest にはあるが、今の profile の JSON に無いキー。JSON の形を変えないために出さない
/// (足すかは別の issue で決める、仕様 §3 の 4・§8)
const HIDDEN_MANIFEST_KEYS: [&str; 1] = ["trace"];

impl Serialize for ProcessProfile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::to_value(&self.manifest).map_err(ser::Error::custom)?;
        if let Some(map) = value.as_object_mut() {
            for key in HIDDEN_MANIFEST_KEYS {
                map.remove(key);
            }
        }
        if let Some(args) = value.get_mut("arguments").and_then(|a| a.as_array_mut()) {
            for arg in args.iter_mut() {
                let Some(obj) = arg.as_object_mut() else {
                    continue;
                };
                let current = obj
                    .get("name")
                    .and_then(|n| n.as_str())
                    .and_then(|n| self.argument_values.get(n));
                if let Some(v) = current {
                    obj.insert("value".to_owned(), v.clone());
                }
            }
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let mut argument_values = BTreeMap::new();
        if let Some(args) = value.get_mut("arguments").and_then(|a| a.as_array_mut()) {
            for arg in args.iter_mut() {
                let Some(obj) = arg.as_object_mut() else {
                    continue;
                };
                let Some(v) = obj.remove("value") else {
                    continue;
                };
                if v.is_null() {
                    continue;
                }
                if let Some(name) = obj.get("name").and_then(|n| n.as_str()) {
                    argument_values.insert(name.to_owned(), v);
                }
            }
        }
        let manifest: ProcessManifest = serde_json::from_value(value).map_err(de::Error::custom)?;
        if manifest.name.is_none() {
            return Err(de::Error::custom(format!(
                "ProcessProfile needs 'name' (type_name={}).",
                manifest.type_name
            )));
        }
        Ok(Self {
            manifest,
            argument_values,
        })
    }
}

impl Display for ProcessProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcessProfile(")?;
        f.write_fmt(format_args!("{}", self.instance_name()))?;
        f.write_fmt(format_args!("::{}, args=[", self.type_name.as_str()))?;
        for a in self.argument_profiles().iter() {
            f.write_fmt(format_args!("{}, ", a))?;
        }
        f.write_fmt(format_args!(
            "], language={}, use_memo={}, publishes=[",
            self.language, self.use_memo
        ))?;
        for tm in self.publishes.iter() {
            f.write_fmt(format_args!("{}, ", tm))?;
        }
        f.write_str("], subscribes={")?;
        for (arg_name, tm) in self.subscribes.iter() {
            f.write_fmt(format_args!("{}:{}, ", arg_name, tm))?;
        }
        match self.outputs.as_ref() {
            None => f.write_fmt(format_args!("}}, outputs=null"))?,
            Some(outputs) => f.write_fmt(format_args!("}}, outputs={}", outputs))?,
        }
        f.write_str(")")?;
        Ok(())
    }
}

impl ProcessProfile {
    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessProfile::new("hoge01", "hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into())
    ///   .add_float_arg("arg1", "float_arg", 1.0.into())
    ///   .add_string_arg("arg2", "string_arg", "default_string".into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Int");
    /// assert_eq!(manifest.arguments[1].name, "arg1");
    /// assert_eq!(manifest.arguments[1].type_name.as_str(), "Float");
    /// assert_eq!(manifest.arguments[2].name, "arg2");
    /// assert_eq!(manifest.arguments[2].type_name.as_str(), "String");
    /// ```
    pub fn new(name: &str, type_name: &str) -> Self {
        Self {
            manifest: ProcessManifest::new(type_name).name(name),
            argument_values: BTreeMap::new(),
        }
    }

    /// インスタンスの名前 (manifest の `name`)
    pub fn instance_name(&self) -> &str {
        self.manifest.name.as_deref().unwrap_or("")
    }

    /// インスタンスの manifest を取り出す
    pub fn into_manifest(self) -> ProcessManifest {
        self.manifest
    }

    /// 引数の profile (引数の manifest + 今の値) の一覧。並びは manifest の `arguments` の順
    pub fn argument_profiles(&self) -> Vec<ArgumentProfile> {
        self.manifest
            .arguments
            .iter()
            .map(|a| ArgumentProfile {
                manifest: a.clone(),
                value: self.argument_values.get(&a.name).cloned(),
            })
            .collect()
    }

    /// 引数 `name` の今の値を設定する。`None` なら消す
    pub fn set_argument_value(&mut self, name: &str, value: Option<Value>) {
        match value {
            Some(v) => {
                self.argument_values.insert(name.to_owned(), v);
            }
            None => {
                self.argument_values.remove(name);
            }
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        self.manifest.name = Some(name.to_owned());
        self
    }

    pub fn language(mut self, language: &str) -> Self {
        self.manifest.language = language.to_owned();
        self
    }

    pub fn use_memo(mut self, use_memo: bool) -> Self {
        self.manifest.use_memo = use_memo;
        self
    }

    pub fn factory(mut self, factory: &str) -> Self {
        self.manifest.factory = factory.to_owned();
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.manifest.description = description.into();
        self
    }

    /// 引数を足す。`arg.value` があれば今の値としても持つ
    pub fn add_arg(mut self, arg: ArgumentProfile) -> Self {
        let ArgumentProfile { manifest, value } = arg;
        self.set_argument_value(&manifest.name.clone(), value);
        self.manifest.arguments.push(manifest);
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessProfile::new("hoge01", "hoge_type")
    ///   .description("hoge manifest")
    ///   .add_bool_arg("arg0", "boolt_arg", false.into());
    /// assert_eq!(manifest.arguments[0].name, "arg0");
    /// assert_eq!(manifest.arguments[0].type_name.as_str(), "Bool");
    /// ```
    pub fn add_bool_arg(self, name: &str, description: &str, default: bool) -> Self {
        self.add_arg(ArgumentProfile::new_bool(name, default).description(description))
    }

    pub fn add_int_arg(self, name: &str, description: &str, default: i64) -> Self {
        self.add_arg(ArgumentProfile::new_int(name, default).description(description))
    }

    pub fn add_float_arg(self, name: &str, description: &str, default: f64) -> Self {
        self.add_arg(ArgumentProfile::new_float(name, default).description(description))
    }

    pub fn add_array_arg(self, name: &str, description: &str, default: Vec<Value>) -> Self {
        self.add_arg(ArgumentProfile::new_array(name, default).description(description))
    }

    pub fn add_object_arg(self, name: &str, description: &str, default: Value) -> Self {
        self.add_arg(ArgumentProfile::new_object(name, default).description(description))
    }

    pub fn add_image_arg(self, name: &str, description: &str) -> Self {
        self.add_arg(ArgumentProfile::new_image(name).description(description))
    }

    pub fn add_string_arg(self, name: &str, description: &str, default: &str) -> Self {
        self.add_arg(ArgumentProfile::new_string(name, default).description(description))
    }

    pub fn container(mut self, container: ContainerManifest) -> Self {
        self.manifest.container_type = Some(container.type_name.clone());
        self.manifest.container_name = container.name.clone();
        self
    }

    pub fn container_type(mut self, container: Option<String>) -> Self {
        self.manifest.container_type = container;
        self
    }

    pub fn container_name(mut self, container: Option<String>) -> Self {
        self.manifest.container_name = container;
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessProfile::new("hoge01", "hoge_type")
    ///   .description("hoge manifest")
    ///   .publishes("topic1");
    /// assert_eq!(manifest.publishes[0].name, "topic1");
    /// ```
    pub fn publishes(mut self, topic_name: &str) -> Self {
        self.manifest.publishes.push(TopicManifest::new(topic_name));
        self
    }

    /// ```
    /// use juiz_sdk::prelude::*;
    /// let manifest = ProcessProfile::new("hoge01", "hoge_type")
    ///   .description("hoge manifest")
    ///   .add_int_arg("arg0", "int_arg", 1.into())
    ///   .subscribes("arg0", "topic1");
    /// assert_eq!(manifest.subscribes.get("arg0").unwrap().name, "topic1");
    /// ```
    pub fn subscribes(mut self, arg_name: &str, topic_name: &str) -> Self {
        self.manifest
            .subscribes
            .insert(arg_name.to_owned(), TopicManifest::new(topic_name));
        self
    }

    pub fn identifier(&self) -> ProcessIdentifier {
        // ContainerProcess の ID は型名の後ろに「:Container 名」が付く (issue #46)
        let m = &self.manifest;
        let name = self.instance_name().to_owned();
        match &m.container_name {
            Some(container_name) => ProcessIdentifier::new_container_process_id(
                m.broker_name.clone(),
                m.broker_type_name.clone(),
                m.type_name.clone(),
                container_name.clone(),
                name,
            )
            .with_namespace(&m.namespace),
            None => ProcessIdentifier::new_process_id(
                m.broker_name.clone(),
                m.broker_type_name.clone(),
                m.type_name.clone(),
                name,
            )
            .with_namespace(&m.namespace),
        }
    }

    pub fn outputs(&self) -> Option<OutputProfile> {
        self.manifest.outputs.clone()
    }

    pub fn set_outputs(mut self, outputs: Option<OutputProfile>) -> Self {
        self.manifest.outputs = outputs;
        self
    }

}
