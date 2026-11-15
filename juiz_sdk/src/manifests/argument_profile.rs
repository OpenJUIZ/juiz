use std::fmt::Display;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use super::argument_type::{type_check, ArgumentType};
use super::manifest_description::Description;
use crate::prelude::*;

/// 引数の profile。引数の manifest ([`ArgumentManifest`]) と、実行時の状態 (今の値 `value`) を合わせたもの
/// (docs/spec/manifest-profile.md §3・§4)。JSON は manifest のキーに `value` を足した形
/// (`#[serde(flatten)]`)。manifest の項目は [`Deref`] で読める。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArgumentProfile {
    #[serde(flatten)]
    pub manifest: ArgumentManifest,
    /// 次の実行で使われる値。p_apply で拘束した値か、push で届いた値 (issue #112)。
    /// 値がまだ無いとき、画像のように値型でないとき、ファクトリの profile では None で、JSON には出さない。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

impl Deref for ArgumentProfile {
    type Target = ArgumentManifest;
    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

impl DerefMut for ArgumentProfile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manifest
    }
}

impl Display for ArgumentProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "ArgumentProfile({}::{},default={})",
            self.name, self.type_name, self.default
        ))
    }
}

impl From<ArgumentManifest> for ArgumentProfile {
    fn from(value: ArgumentManifest) -> Self {
        Self {
            manifest: value,
            value: None,
        }
    }
}

impl From<ArgumentProfile> for ArgumentManifest {
    fn from(value: ArgumentProfile) -> Self {
        value.manifest
    }
}

impl ArgumentProfile {
    pub fn new(
        type_name: ArgumentType,
        name: &str,
        description: Description,
        default: Value,
    ) -> Self {
        ArgumentManifest::new(type_name, name, description, default).into()
    }

    pub fn new_with_check(
        type_name: ArgumentType,
        name: &str,
        description: Description,
        default: Value,
    ) -> JuizResult<Self> {
        type_check(&type_name, &default)?;
        Ok(Self::new(type_name, name, description, default))
    }

    /// 引数の manifest を取り出す
    pub fn into_manifest(self) -> ArgumentManifest {
        self.manifest
    }

    pub fn description(mut self, description: &str) -> Self {
        self.manifest.description = description.into();
        self
    }

    pub fn new_bool(name: &str, default: bool) -> Self {
        Self::new(ArgumentType::Bool, name, "".into(), default.into())
    }

    pub fn new_int(name: &str, default: i64) -> Self {
        Self::new(ArgumentType::Int, name, "".into(), default.into())
    }

    pub fn new_float(name: &str, default: f64) -> Self {
        Self::new(ArgumentType::Float, name, "".into(), default.into())
    }

    pub fn new_string(name: &str, default: &str) -> Self {
        Self::new(ArgumentType::String, name, "".into(), default.into())
    }

    pub fn new_array(name: &str, default: Vec<Value>) -> Self {
        Self::new(ArgumentType::Array, name, "".into(), default.into())
    }

    pub fn new_object(name: &str, default: Value) -> Self {
        Self::new(ArgumentType::Object, name, "".into(), default)
    }

    pub fn new_image(name: &str) -> Self {
        Self::new(ArgumentType::Image, name, "".into(), jvalue!({}))
    }
}

// impl Into<Value> for ArgumentProfile {
//     fn into(self) -> Value {
//         jvalue!({
//             "name": self.name,
//             "type": self.type_name.as_str(),
//             "description": self.description.to_str(),
//             "default": self.default,
//         })
//     }
// }

// /// ```
// /// use juiz_core::prelude::*;
// /// fn main() -> JuizResult<()> {
// /// let arg_value = jvalue!({
// ///   "type": "int",
// ///   "name": "arg0",
// ///   "description": "int arg",
// ///   "default": 1
// /// });
// /// let arg: ArgumentProfile = arg_value.try_into()?;
// ///
// ///
// /// Ok(())}
// /// ```
// impl TryFrom<Value> for ArgumentProfile {
//     type Error = anyhow::Error;

//     fn try_from(value: Value) -> Result<Self, Self::Error> {
//         let name = obj_get_str(&value, "name")?;
//         let description = obj_get_str(&value, "description").or::<JuizError>(Ok("")).unwrap();
//         let type_name: ArgumentType = obj_get_str(&value, "type")?.try_into().or_else(|e| {
//             log::error!("TryFrom<Value> for ArgumentProfile::try_from({value:?}) failed. {e}");
//             Err(e)
//         })?;
//         let default_value = obj_get(&value, "default").or_else(|e| {
//             if type_name == ArgumentType::Image {
//                 return Ok(&Value::Null);
//             }
//             log::error!("TryFrom<Value> for ArgumentProfile::try_from({value:?}) failed. {e}");
//             Err(e)
//         })?;
//         ArgumentProfile::new_with_check(type_name, name, description.into(), default_value.clone())
//     }
// }
