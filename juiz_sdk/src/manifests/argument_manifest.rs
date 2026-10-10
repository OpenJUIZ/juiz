
use std::fmt::Display;

use crate::prelude::*;
use super::manifest_description::Description;
use serde::{Deserialize, Serialize};
use super::argument_type::{ArgumentType, type_check};

fn default_description() -> Description { Description{text: "".to_owned()} }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArgumentManifest {
    pub type_name: ArgumentType,
    pub name: String,

    #[serde(default="default_description")]
    pub description: Description,
    pub default: Value
}

impl Display for ArgumentManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ArgumentManifest({}::{},default={})", self.name, self.type_name, self.default))
    }
}

impl ArgumentManifest {

    pub fn new(type_name: ArgumentType, name: &str, description: Description, default: Value) -> Self {
        ArgumentManifest {
            type_name,
            name: name.to_owned(),
            description,
            default
        }
    }

    pub fn new_with_check(type_name: ArgumentType, name: &str, description: Description, default: Value) -> JuizResult<Self> {
        type_check(&type_name, &default)?;
        Ok(ArgumentManifest {
            type_name,
            name: name.to_owned(),
            description,
            default
        })
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = description.into();
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
        Self::new(ArgumentType::Object, name, "".into(), default.into())
    }

    pub fn new_object(name: &str, default: Value) -> Self {
        Self::new(ArgumentType::Object, name, "".into(), default)
    }

    pub fn new_image(name: &str) -> Self {
        Self::new(ArgumentType::String, name, "".into(), jvalue!({}))
    }
    
}

// impl Into<Value> for ArgumentManifest {
//     fn into(self) -> Value {
//         jvalue!({
//             "name": self.name,
//             "type": self.type_name.as_str(),
//             "description": self.description.to_str(),
//             "default": self.default,
//         })
//     }
// }

// / ```
// / use juiz_core::prelude::*;
// / fn main() -> JuizResult<()> {
// / let arg_value = jvalue!({
// /   "type": "int",
// /   "name": "arg0",
// /   "description": "int arg",
// /   "default": 1
// / });
// / let arg: ArgumentManifest = arg_value.try_into()?;
// / 
// / 
// / Ok(())}
// / ```

// impl TryFrom<Value> for ArgumentManifest {
//     type Error = anyhow::Error;

//     fn try_from(value: Value) -> Result<Self, Self::Error> {
//         let name = obj_get_str(&value, "name")?;
//         let description = obj_get_str(&value, "description").or::<JuizError>(Ok("")).unwrap();
//         let type_name: ArgumentType = obj_get_str(&value, "type")?.try_into().or_else(|e| {
//             log::error!("TryFrom<Value> for ArgumentManifest::try_from({value:?}) failed. {e}");
//             Err(e)
//         })?;
//         let default_value = obj_get(&value, "default").or_else(|e| {
//             if type_name == ArgumentType::Image {
//                 return Ok(&Value::Null);
//             }
//             log::error!("TryFrom<Value> for ArgumentManifest::try_from({value:?}) failed. {e}");
//             Err(e)
//         })?;
//         ArgumentManifest::new_with_check(type_name, name, description.into(), default_value.clone())
//     }
// }
