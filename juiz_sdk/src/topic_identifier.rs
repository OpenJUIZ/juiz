use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicIdentifier {
    pub name: String
}

impl TopicIdentifier {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}


impl Display for TopicIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TopicID(name={})", self.name))
    }
}

impl TryFrom<String> for TopicIdentifier {
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self { name : value.clone() })
    }
    
    type Error = anyhow::Error;
}

impl FromStr for TopicIdentifier {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self { name : s.to_owned() })
    }
}


impl From<&str> for TopicIdentifier {
    fn from(value: &str) -> Self {
        Self { name : value.to_owned() }
    }
}

impl TryInto<String> for TopicIdentifier {
    
    type Error = anyhow::Error;
    
    fn try_into(self) -> Result<String, Self::Error> {
        Ok(self.name.clone())
    }
}