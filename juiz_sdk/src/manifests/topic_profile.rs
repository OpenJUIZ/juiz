use std::fmt::Display;
use std::ops::Deref;

use crate::prelude::*;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

/// Topic の profile。中身は manifest ([`TopicManifest`]) と同じ (docs/spec/manifest-profile.md §4・§8)。
///
/// Topic は内部で Process として持つので、API の profile は Process の profile
/// ([`ProcessProfile`](super::ProcessProfile)) で返る。この型は Topic の名前だけを
/// profile として扱う所のために残す。JSON は `{"name": ...}`。manifest の項目は [`Deref`] で読める。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopicProfile {
    #[serde(flatten)]
    pub manifest: TopicManifest,
}

impl Deref for TopicProfile {
    type Target = TopicManifest;
    fn deref(&self) -> &Self::Target {
        &self.manifest
    }
}

impl TopicProfile {
    pub fn new(name: &str) -> Self {
        TopicManifest::new(name).into()
    }
}

impl From<TopicManifest> for TopicProfile {
    fn from(manifest: TopicManifest) -> Self {
        Self { manifest }
    }
}

impl From<TopicProfile> for TopicManifest {
    fn from(value: TopicProfile) -> Self {
        value.manifest
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
            Some(v_str) => Ok(TopicProfile::new(v_str)),
            None => Err(anyhow!(JuizError::TopicManifestInvalidError {
                message: "Topic manifest can not convert to Value.".to_owned()
            })),
        }
    }

    type Error = anyhow::Error;
}

impl Into<Value> for TopicProfile {
    fn into(self) -> Value {
        self.manifest.name.into()
    }
}
