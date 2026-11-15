use std::{collections::HashMap, fmt::Display, iter::Map, process::Output};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct StructManifest {
    pub type_name: String,
    pub members: HashMap<String, OutputManifest>,
}

impl Display for StructManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("struct {} {{", self.type_name))?;
        for (key, value) in self.members.iter() {
            match value {
                OutputManifest::Struct(struct_manifest) => {
                    f.write_fmt(format_args!("{}::{}", key, struct_manifest))?;
                }
                OutputManifest::Primitive(primitive_manifest) => {
                    f.write_fmt(format_args!("{}::{}", key, primitive_manifest))?;
                }
            }
        }
        f.write_str("}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrimitiveManifest {
    pub type_name: String,
    pub default_value: Option<String>,
}

impl Display for PrimitiveManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.default_value.is_some() {
            f.write_fmt(format_args!(
                "{}={}",
                self.type_name,
                self.default_value.as_ref().unwrap()
            ))
        } else {
            f.write_fmt(format_args!("{}", self.type_name))
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "enum_type")]
pub enum OutputManifest {
    Struct(StructManifest),
    Primitive(PrimitiveManifest),
}

impl Display for OutputManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OutputManifest(")?;
        match self {
            OutputManifest::Struct(struct_manifest) => {
                f.write_fmt(format_args!("{}", struct_manifest))?;
            }
            OutputManifest::Primitive(primitive_manifest) => {
                f.write_fmt(format_args!("{}", primitive_manifest))?;
            }
        }
        f.write_str(")")
    }
}

impl OutputManifest {
    pub fn default() -> Self {
        OutputManifest::Primitive(PrimitiveManifest {
            type_name: "int".to_owned(),
            default_value: Some("0".to_owned()),
        })
    }

    pub fn is_struct(&self) -> bool {
        match self {
            OutputManifest::Struct(_s) => true,
            OutputManifest::Primitive(_p) => false,
        }
    }

    pub fn is_primitive(&self) -> bool {
        match self {
            OutputManifest::Struct(_s) => false,
            OutputManifest::Primitive(_p) => true,
        }
    }

    pub fn as_struct(&self) -> Option<&StructManifest> {
        match self {
            OutputManifest::Struct(s) => Some(s),
            OutputManifest::Primitive(p) => None,
        }
    }

    pub fn as_primitive(&self) -> Option<&PrimitiveManifest> {
        match self {
            OutputManifest::Struct(s) => None,
            OutputManifest::Primitive(p) => Some(p),
        }
    }
}

// 古い名前 (0.2 系のあいだ残す。消すのは 0.3.0 以降、仕様 docs/spec/manifest-profile.md §7、#506)

/// 古い名前。`OutputManifest` を使う。
#[deprecated(since = "0.2.11", note = "use OutputManifest")]
pub type OutputProfile = OutputManifest;

/// 古い名前。`PrimitiveManifest` を使う。
#[deprecated(since = "0.2.11", note = "use PrimitiveManifest")]
pub type PrimitiveProfile = PrimitiveManifest;

/// 古い名前。`StructManifest` を使う。
#[deprecated(since = "0.2.11", note = "use StructManifest")]
pub type StructProfile = StructManifest;
