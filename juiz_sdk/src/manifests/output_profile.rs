
use std::{collections::HashMap, fmt::Display, iter::Map, process::Output};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct StructProfile {
    pub type_name: String,
    pub members: HashMap<String, OutputProfile>
}

impl Display for StructProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("struct {} {{", self.type_name))?;
        for (key, value) in self.members.iter() {
            match value {
                OutputProfile::Struct(struct_profile) => {
                    f.write_fmt(format_args!("{}::{}", key, struct_profile))?;
                },
                OutputProfile::Primitive(primitive_profile) => {
                    f.write_fmt(format_args!("{}::{}", key, primitive_profile))?;
                }
            }
        }
        f.write_str("}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrimitiveProfile {
    pub type_name: String,
    pub default_value: Option<String>
}

impl Display for PrimitiveProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.default_value.is_some() {
            f.write_fmt(format_args!("{}={}", self.type_name, self.default_value.as_ref().unwrap()))
        } else {
            f.write_fmt(format_args!("{}", self.type_name))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag="enum_type")]
pub enum OutputProfile {
    Struct(StructProfile),
    Primitive(PrimitiveProfile),
}


impl Display for OutputProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OutputProfile(")?;
        match self {
            OutputProfile::Struct(struct_profile) => {
                f.write_fmt(format_args!("{}", struct_profile))?;
            }
            OutputProfile::Primitive(primitive_profile) => {
                f.write_fmt(format_args!("{}", primitive_profile))?;
            }
        }
        f.write_str(")")
    }
}

impl OutputProfile {

    pub fn default() -> Self {
        OutputProfile::Primitive(PrimitiveProfile{type_name: "int".to_owned(), default_value: Some("0".to_owned())})
    }

    pub fn is_struct(&self) -> bool {
        match self {
            OutputProfile::Struct(_s) => true,
            OutputProfile::Primitive(_p) => false,
        }
    }

    pub fn is_primitive(&self) -> bool {
        match self {
            OutputProfile::Struct(_s) => false,
            OutputProfile::Primitive(_p) => true,
        }
    }

    pub fn as_struct(&self) -> Option<&StructProfile> {
        match self {
            OutputProfile::Struct(s) => Some(s),
            OutputProfile::Primitive(p) => None
        }
    }

    pub fn as_primitive(&self) -> Option<&PrimitiveProfile> {
        match self {
            OutputProfile::Struct(s) => None,
            OutputProfile::Primitive(p) => Some(p)
        }
    }
}