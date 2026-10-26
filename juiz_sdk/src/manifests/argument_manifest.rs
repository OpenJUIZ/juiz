use std::fmt::Display;

use super::argument_type::{type_check, ArgumentType};
use super::manifest_description::Description;
use crate::prelude::*;
use crate::types::{type_error, validate_type_name, ArgumentTypeOf, DataType, JuizType, TypeSchema};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};

fn default_description() -> Description {
    Description {
        text: "".to_owned(),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArgumentManifest {
    pub type_name: ArgumentType,
    pub name: String,

    #[serde(default = "default_description")]
    pub description: Description,
    /// 省略時は null。pyjuiz は既定値のない Image 引数で default を出力しない (issue #169 で判明)
    #[serde(default)]
    pub default: Value,
    /// 型参照 (型付きの引数だけ、仕様 docs/spec/datatype.md §6、issue #190)。型なしなら `None` で、JSON には出さない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<TypeSchema>,
}

impl Display for ArgumentManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "ArgumentManifest({}::{},default={})",
            self.name, self.type_name, self.default
        ))
    }
}

impl ArgumentManifest {
    pub fn new(
        type_name: ArgumentType,
        name: &str,
        description: Description,
        default: Value,
    ) -> Self {
        ArgumentManifest {
            type_name,
            name: name.to_owned(),
            description,
            default,
            schema: None,
        }
    }

    /// 型付きの引数を作る (仕様 §6)。
    ///
    /// `type_name` は型から決める。引数名は Avro の名前の規則に従わなければならない。
    /// `default` は辞書の表し方 (§4) で検査する。`null` は「既定値なし」として扱う (`T?` なら既定値 `null`)。
    ///
    /// ```
    /// use juiz_sdk::prelude::*;
    /// use juiz_sdk::types::{DataType, TypeSchema};
    /// let schema = TypeSchema::from_data_type(&DataType::array(DataType::Long)).unwrap();
    /// let arg = ArgumentManifest::new_typed("values", schema, jvalue!([1, 2])).unwrap();
    /// assert_eq!(arg.type_name.as_str(), "Array");
    /// assert!(ArgumentManifest::new_typed("values", arg.schema.clone().unwrap(), jvalue!(["a"])).is_err());
    /// ```
    pub fn new_typed(name: &str, schema: TypeSchema, default: Value) -> JuizResult<Self> {
        let data_type = schema.validate().map_err(type_error)?;
        check_typed_argument(name, &data_type, &default)?;
        Ok(ArgumentManifest {
            type_name: data_type.argument_type()?,
            name: name.to_owned(),
            description: default_description(),
            default,
            schema: Some(schema),
        })
    }

    /// Rust の型 `T` の引数を作る。`default` が `None` なら既定値なし
    pub fn new_juiz_type<T: JuizType>(name: &str, default: Option<&T>) -> JuizResult<Self> {
        let default = match default {
            Some(d) => d.to_value()?,
            None => Value::Null,
        };
        Self::new_typed(name, T::type_schema()?, default)
    }

    /// 型付きなら、`schema` を検査し、`default` が型に合うかを検査する (型なしなら何もしない)
    pub fn check_schema(&self) -> JuizResult<()> {
        match self.schema.as_ref() {
            Some(schema) => {
                let data_type = schema.validate().map_err(type_error)?;
                check_typed_argument(&self.name, &data_type, &self.default)
            }
            None => Ok(()),
        }
    }

    pub fn new_with_check(
        type_name: ArgumentType,
        name: &str,
        description: Description,
        default: Value,
    ) -> JuizResult<Self> {
        type_check(&type_name, &default)?;
        Ok(ArgumentManifest {
            type_name,
            name: name.to_owned(),
            description,
            default,
            schema: None,
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
        Self::new(ArgumentType::Array, name, "".into(), default.into())
    }

    pub fn new_object(name: &str, default: Value) -> Self {
        Self::new(ArgumentType::Object, name, "".into(), default)
    }

    pub fn new_image(name: &str) -> Self {
        Self::new(ArgumentType::Image, name, "".into(), jvalue!({}))
    }
}

/// 型付きの引数の規則 (仕様 §3.1、§6、§8)
pub(crate) fn check_typed_argument(name: &str, data_type: &DataType, default: &Value) -> JuizResult<()> {
    validate_type_name(name, name).map_err(type_error)?;
    if *data_type == DataType::Null {
        return Err(anyhow!(JuizError::TypeSchemaInvalidError {
            path: name.to_owned(),
            message: "null cannot be the type of an argument".to_owned()
        }));
    }
    // image は辞書の外の値で、既定値を持たない (今の Image 引数は `{}` か `null`)
    if *data_type == DataType::Image || (default.is_null() && !data_type.is_nullable()) {
        return Ok(());
    }
    crate::types::read_value(data_type, default, name)
        .map(|_| ())
        .map_err(type_error)
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
