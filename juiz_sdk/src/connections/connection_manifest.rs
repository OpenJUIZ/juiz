use std::fmt::Display;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};
use crate::{connection_identifier::ConnectionIdentifier, process_identifier::ProcessIdentifier, result::{JuizError, JuizResult}, value::{CapsuleMap, Value}};

use super::connection_type::ConnectionType;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionManifest {
    // pub identifier: Option<String>,
    // pub identifier: Option<ConnectionIdentifier>,
    pub connection_type: ConnectionType,
    pub source_process_id: ProcessIdentifier,
    pub destination_process_id: ProcessIdentifier,
    pub arg_name: String,
}

impl Into<Value> for ConnectionManifest {
    fn into(self) -> Value {
        let mut map: Map<String, Value> = Map::new();
        map.insert("connection_type".to_owned(), self.connection_type.to_string().into());
        map.insert("source_process_id".to_owned(), self.source_process_id.to_string().into());
        map.insert("destination_process_id".to_owned(), self.destination_process_id.to_string().into());
        map.insert("arg_name".to_owned(), self.arg_name.into());
        //if self.identifier.is_some() {
        //    map.insert("identifier".to_owned(), self.identifier.unwrap().into());
        //}
        map.into()
    }
}

impl TryFrom<CapsuleMap> for ConnectionManifest {
    type Error = anyhow::Error;
    
    fn try_from(value: CapsuleMap) -> Result<Self, Self::Error> {
        log::debug!("TryFrom<CapsuleMap> for ConnectionManifestが呼ばれました(value={value})");
        let source_process_id = ProcessIdentifier::from_value(&value.get("source_process_id")?.extract_value()?)?;
        let destination_process_id = ProcessIdentifier::from_value(&value.get("destination_process_id")?.extract_value()?)?;
        
        Ok(ConnectionManifest {
            connection_type: value.get_str("connection_type")?.as_str().try_into()?,
            //identifier: value.get_str("identifier").ok(),
            source_process_id,
            destination_process_id,
            arg_name:  value.get_str("arg_name")?
        })
    }
}

fn err_handle(value: Option<&Value>) -> anyhow::Error {
    anyhow!(JuizError::InvalidArgumentError{message: format!("Conversion faild Value({value:?}) -> ConnectionManifest.")})
}
            

fn value_to_identifier(value: Option<&Value>) -> JuizResult<String> {
    match value {
        Some(source_val) => {
            match source_val {
                Value::String(source_id) => Ok(source_id.clone()),
                Value::Object(source_obj) => {
                    if let Some(id_val) = source_obj.get("identifier") {
                        return Ok(id_val.as_str().unwrap().to_owned())
                    }
                    Err(err_handle(Some(source_val)))
                },
                _ => {
                    Err(err_handle(Some(source_val)))
                }
            }
        }
        None => Err(err_handle(None)),
    }
}

impl TryFrom<Value> for ConnectionManifest {
    type Error = anyhow::Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
       let err_handle = ||{err_handle(Some(&value))};
        match value.as_object() {
            Some(vobj) => {
                let connection_type = vobj.get("connection_type").or(Some(&json!("push"))).unwrap().as_str().ok_or_else(err_handle)?.to_owned();
                Ok( ConnectionManifest{
                    connection_type: ConnectionType::from(connection_type.as_str()),
                    source_process_id: value_to_identifier(vobj.get("source_process_id"))?.try_into()?,
                    destination_process_id: value_to_identifier(vobj.get("destination_process_id"))?.try_into()?,
                    arg_name: vobj.get("arg_name").ok_or_else(err_handle)?.as_str().ok_or_else(err_handle)?.to_owned()
                } )
            }
            None => todo!(),
        }
    }
}

impl ConnectionManifest {

    pub fn new(connection_type: ConnectionType, source_process_id: ProcessIdentifier, arg_name: String, destination_process_id: ProcessIdentifier) -> Self {
        Self {
            connection_type,
            source_process_id,
            destination_process_id,
            arg_name
        }
    }
}

impl Into<ConnectionIdentifier> for ConnectionManifest {
    fn into(self) -> ConnectionIdentifier {
        ConnectionIdentifier::new(self.source_process_id, self.arg_name.as_str(), self.destination_process_id)
    }
}

impl Display for ConnectionManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ConnectionManifest({}, {}, {}, {})", self.connection_type, self.source_process_id, self.arg_name, self.destination_process_id))
    }
}