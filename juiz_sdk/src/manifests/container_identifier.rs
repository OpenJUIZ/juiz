use std::fmt::Display;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::result::JuizError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerIdentifier {
    pub broker_name: String, 
    pub broker_type_name: String,
    pub name: String,
    pub type_name: String,
    pub class_name: String,
}

impl ContainerIdentifier {
    pub(crate) fn new(broker_name: String, broker_type_name: String, name: String, type_name: String) -> Self {
        Self { broker_name, broker_type_name, name, type_name, class_name: "Container".to_owned()}
    }

    pub fn to_string(&self) -> String {
        self.clone().into()
    }
}

impl Display for ContainerIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ContainerIdentifier({}://{}/{}/{}::{}", self.broker_type_name, self.broker_name, self.class_name, self.name, self.type_name))
    }
}

impl TryFrom<String> for ContainerIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match regex::Regex::new(r"^(.+?)://(.+?)/(.+?)/(.+?)::(.+?)$") {
            Ok(re) => {
                match re.captures(value.as_str()) {
                    Some(caps) => {
                        let class_name = caps[3].to_owned();
                        let type_name = caps[5].to_owned();
                        let name = caps[4].to_owned();
                        let broker_name = caps[2].to_owned();
                        let broker_type_name = caps[1].to_owned();
                        Ok(Self{ 
                            class_name, 
                            type_name, 
                            name, 
                            broker_name, 
                            broker_type_name})
                    },
                    None => {
                        log::error!("TryFrom<String> for ContainerIdentifier error. Invalid Identifier ({value}).");
                        return Err(anyhow!(JuizError::InvalidIdentifierError{message: value}))
                    },
                }
            }
            Err(e) => Err(anyhow!(e))
        }    
    }
}

impl Into<String> for ContainerIdentifier {
    fn into(self) -> String {
        format!("{}://{}/{}/{}::{}", self.broker_type_name, self.broker_name, self.class_name, self.name, self.type_name)
    }
}