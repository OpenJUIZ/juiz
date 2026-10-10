
use std::fmt::Display;

use crate::prelude::*;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};



#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ArgumentType {
    Bool, 
    Int,
    Float,
    String,
    Array,
    Object,
    Image,
}

impl ArgumentType {

    pub fn as_str(&self) -> &'static str {
        match self {
            ArgumentType::Bool => "Bool", 
            ArgumentType::Int => "Int",
            ArgumentType::Float => "Float",
            ArgumentType::String => "String",
            ArgumentType::Array => "Array",
            ArgumentType::Object => "Object",
            ArgumentType::Image => "Image",
        }
    }
}

impl Display for ArgumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ArgumentType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Bool" => Ok(ArgumentType::Bool),
            "Int" => Ok(ArgumentType::Int),
            "Float" => Ok(ArgumentType::Float),
            "String" => Ok(ArgumentType::String),
            "Array" => Ok(ArgumentType::Array),
            "Object" => Ok(ArgumentType::Object),
            "Image" => Ok(ArgumentType::Image),
            _ => Err(anyhow!(JuizError::ProcessManifestInvalidError{message: "Argument type is invalid in ArgumentManifest in ProcessManifest.".to_owned()}))
        }
    }
}


pub(super) fn type_check(arg_type: &ArgumentType, value: &Value) -> JuizResult<()> {
    fn ret_err() -> JuizResult<()> {
        Err(anyhow!(JuizError::ArguemntTypeIsInvalidError {}))
    }
    // println!("type_check({arg_type:?}, {value:?}) called");
    match arg_type {
        ArgumentType::Bool => if value.is_boolean() { Ok(()) } else {ret_err() },
        ArgumentType::Int => if value.is_i64() {Ok(())} else { ret_err() },
        ArgumentType::Float => if value.is_f64() {Ok(())} else { ret_err() },
        ArgumentType::String => if value.is_string() {Ok(())} else { ret_err() },
        ArgumentType::Array => if value.is_array() {Ok(())} else { ret_err() },
        ArgumentType::Object => if value.is_object() {Ok(())} else { ret_err() },
        ArgumentType::Image => if value.is_null() || value.is_object() {Ok(())} else { ret_err() },
    }
}