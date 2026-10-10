use std::fmt::Display;

use crate::prelude::*;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopicProfile {
    pub name: String
}

impl TopicProfile {
    pub fn new(name: &str) -> Self {
        TopicProfile{name: name.to_owned()}
    }
}

impl From<TopicManifest> for TopicProfile {
    fn from(value: TopicManifest) -> Self {
        Self {
            name: value.name
        }
    }
}

impl Display for TopicProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TopicProfile({})", self.name))
    }
}

impl TryFrom<Value> for TopicProfile {
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value.as_str() {
            Some(v_str) => {
                Ok(TopicProfile{name: v_str.to_owned()})
            },
            None => Err(anyhow!(JuizError::TopicManifestInvalidError{message: "Topic manifest can not convert to Value.".to_owned()})),
        }
    }
    
    type Error = anyhow::Error;
}

impl Into<Value> for TopicProfile {
    fn into(self) -> Value {
        self.name.into()
    }
}