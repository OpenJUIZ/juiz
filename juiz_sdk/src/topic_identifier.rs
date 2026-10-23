use std::{fmt::Display, str::FromStr};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::{
    identifier::{join_namespace, split_namespace, validate_namespace},
    prelude::*,
};

/// Topic の識別子
///
/// `name` はネームスペースを含めた完全な名前 (`robot1/value_topic`、ルートなら `value_topic`)。
/// Topic は名前がそのまま識別子なので、Process の profile と違って `name` は短い名前ではない。
/// `namespace` は `name` の最後の `/` より前 (ルートなら空) で、`name` から決まる (issue #149)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "TopicIdentifierRepr")]
pub struct TopicIdentifier {
    pub name: String,
    pub namespace: String,
}

/// 逆シリアライズ用。`namespace` が無い (古い版の) 値も読めるように、`name` から計算し直す
#[derive(Deserialize)]
struct TopicIdentifierRepr {
    name: String,
}

impl From<TopicIdentifierRepr> for TopicIdentifier {
    fn from(value: TopicIdentifierRepr) -> Self {
        Self::new(value.name)
    }
}

impl TopicIdentifier {
    pub fn new(name: String) -> Self {
        let (namespace, _) = split_namespace(name.as_str());
        Self { name, namespace }
    }

    /// ネームスペースを除いた短い名前
    pub fn short_name(&self) -> String {
        split_namespace(self.name.as_str()).1
    }
}

/// conf に書いた Topic 名を、書いたオブジェクトのネームスペース `namespace` で解決する (issue #149)
///
/// `value_topic` は `namespace` の下 (`robot1/value_topic`)、`/value_topic` はルート (`value_topic`)。
/// 返す値は先頭に `/` を付けない完全な名前。`namespace` が空 (ルート) なら相対名はそのままになる。
/// 各要素が空、または `:` `|` を含むとエラー。
///
/// ```
/// use juiz_sdk::topic_identifier::resolve_topic_name;
/// assert_eq!(resolve_topic_name("robot1", "value_topic").unwrap(), "robot1/value_topic");
/// assert_eq!(resolve_topic_name("robot1", "/value_topic").unwrap(), "value_topic");
/// assert_eq!(resolve_topic_name("", "value_topic").unwrap(), "value_topic");
/// assert!(resolve_topic_name("robot1", "a:b").is_err());
/// ```
pub fn resolve_topic_name(namespace: &str, topic_name: &str) -> JuizResult<String> {
    let resolved = match topic_name.strip_prefix('/') {
        Some(absolute) => absolute.to_owned(),
        None => join_namespace(namespace, topic_name),
    };
    if resolved.is_empty() || topic_name.is_empty() {
        return Err(anyhow!(JuizError::InvalidNameError {
            message: format!("topic name must not be empty ('{topic_name}')")
        }));
    }
    validate_namespace(resolved.as_str()).map_err(|_| {
        anyhow!(JuizError::InvalidNameError {
            message: format!(
                "topic name '{topic_name}' (resolved to '{resolved}') has an invalid element (elements must be non-empty and must not contain ':' or '|')"
            )
        })
    })?;
    Ok(resolved)
}

impl Display for TopicIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TopicID(name={})", self.name))
    }
}

impl TryFrom<String> for TopicIdentifier {
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(Self::new(value))
    }

    type Error = anyhow::Error;
}

impl FromStr for TopicIdentifier {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.to_owned()))
    }
}

impl From<&str> for TopicIdentifier {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

impl TryInto<String> for TopicIdentifier {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<String, Self::Error> {
        Ok(self.name.clone())
    }
}
