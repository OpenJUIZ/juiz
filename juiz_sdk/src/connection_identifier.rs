use std::fmt::Display;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::anyhow;
use crate::{process_identifier::ProcessIdentifier, result::JuizError};


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionIdentifier {
    pub source_identifier: ProcessIdentifier,
    pub destination_identifier: ProcessIdentifier,
    pub arg_name: String,
}

impl Display for ConnectionIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ConnectionID(src={}, arg={}, dst={})", self.source_identifier, self.arg_name, self.destination_identifier))
    }
}

impl ConnectionIdentifier {
    pub fn new(source_identifier: ProcessIdentifier, arg_name: &str, destination_identifier: ProcessIdentifier) -> Self {
        Self {
            source_identifier,
            arg_name: arg_name.to_owned(),
            destination_identifier,
        }
    }
}

impl TryFrom<Value> for ConnectionIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(s) => return s.try_into(),
            Value::Object(map) => {
                let arg_name = map.get("arg_name").and_then(|v|{v.as_str()}).ok_or(anyhow!(JuizError::InvalidIdentifierError{message: format!("")}))?.to_owned();
                let source_identifier = map.get("source_identifier").and_then(|v|{v.as_str()}).ok_or(anyhow!(JuizError::InvalidIdentifierError{message: format!("")}))?.to_owned();
                let destination_identifier = map.get("destination_identifier").and_then(|v|{v.as_str()}).ok_or(anyhow!(JuizError::InvalidIdentifierError{message: format!("")}))?.to_owned();
                Ok(Self {
                    arg_name,
                    source_identifier: source_identifier.try_into()?,
                    destination_identifier: destination_identifier.try_into()?
                })
            },
            _ => {
                log::error!("TryFrom<Value>(for ConnectionIdentifier)::try_from(Value) failed. Value must be String for Object.");
                Err(anyhow::Error::from(JuizError::InvalidConnectionIdentifierError{identifier: format!("")}))
            }
        }
    }
}

impl TryFrom<String> for ConnectionIdentifier {
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let tokens = value[13..].split('|').collect::<Vec<&str>>();
        if tokens.len() != 3 {
            log::error!("TryFrom<String>(for ConnectionIdentifier)::try_from({value}) failed. The number of split tokens by '|' is not three but {}", tokens.len());
            return Err(anyhow::Error::from(JuizError::InvalidConnectionIdentifierError{identifier: value}));
        }
        Ok(ConnectionIdentifier{
            source_identifier: tokens[0].to_owned().try_into()?,
            arg_name: tokens[1].to_owned(),
            destination_identifier: tokens[2].to_owned().try_into()?,
        })
    }
    type Error = anyhow::Error;
}

impl Into<String> for ConnectionIdentifier {
    fn into(self) -> String {
        format!("connection://{}|{}|{}", self.source_identifier, self.arg_name, self.destination_identifier)
    }
}