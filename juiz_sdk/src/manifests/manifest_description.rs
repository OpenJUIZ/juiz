
use std::fmt::Display;

use crate::anyhow::anyhow;
use serde::{de::{MapAccess, Visitor}, Deserialize, Serialize};

use crate::prelude::*;


#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all="camelCase")]
pub struct Description {
    pub text: String
}

impl<'de> Deserialize<'de> for Description {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
            // println!("deserialize!()");
            struct DescriptionVisitor;

            impl<'de> Visitor<'de> for DescriptionVisitor {
                type Value = Description;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("struct Description")
                }

                fn visit_str<E>(self, value: &str) -> Result<Description, E>
                    where E: serde::de::Error,
                {
                    // println!("visit_str({value}) called");
                    Ok(Description {
                        text: value.to_owned(),
                    })
                }

                fn visit_string<E>(self, value: String) -> Result<Description, E>
                    where E: serde::de::Error,
                {
                    // println!("visit_string({value}) called");
                    Ok(Description {
                        text: value,
                    })
                }

                fn visit_map<V>(self, mut _map: V) -> Result<Description, V::Error>
                    where V: MapAccess<'de>,
                {
                    println!("visit_map is called!!!");
                    

                    Ok(Description{ text: "".to_owned()})
                }
            }

            

            // deserializer.deserialize_any(DescriptionVisitor)
            deserializer.deserialize_string(DescriptionVisitor)
    }
}

impl Display for Description {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Description(\"{}\")", self.text))
    }
}

impl Description {
    pub fn new(text: &str) -> Self {
        Description{
            text: text.to_owned()
        }
    }

    pub fn to_str(self) -> String {
        self.text
    }

    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }
}

impl Into<Value> for Description {
    fn into(self) -> Value {
        jvalue!(self.text)
    }
}


impl TryFrom<Value> for Description {
    type Error = anyhow::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if value.is_string() { 
            return Ok(Description::new(value.as_str().unwrap()));
        }
        return Err(anyhow!(JuizError::InvalidArgumentError { message: format!("Invalid Value Type for Description struct.") }));
    }
}

impl From<&str> for Description {
    fn from(value: &str) -> Self {
        Description::new(value)
    }
}