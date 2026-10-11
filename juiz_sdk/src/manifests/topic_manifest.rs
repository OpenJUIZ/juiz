use std::fmt::Display;

use crate::prelude::*;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopicManifest {
    pub name: String,
}

impl TopicManifest {
    pub fn new(name: &str) -> Self {
        TopicManifest {
            name: name.to_owned(),
        }
    }
}

impl Display for TopicManifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TopicManifest({})", self.name))
    }
}

impl TryFrom<Value> for TopicManifest {
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value.as_str() {
            Some(v_str) => Ok(TopicManifest {
                name: v_str.to_owned(),
            }),
            None => Err(anyhow!(JuizError::TopicManifestInvalidError {
                message: "Topic manifest can not convert to Value.".to_owned()
            })),
        }
    }

    type Error = anyhow::Error;
}

impl Into<Value> for TopicManifest {
    fn into(self) -> Value {
        self.name.into()
    }
}
