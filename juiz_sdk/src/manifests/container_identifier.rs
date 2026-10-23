use std::fmt::Display;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::identifier::{join_namespace, split_namespace, validate_name, validate_namespace};
use crate::result::{JuizError, JuizResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerIdentifier {
    pub broker_name: String,
    pub broker_type_name: String,
    /// ネームスペース (`/` 区切り、空ならルート)
    #[serde(default)]
    pub namespace: String,
    /// ネームスペースを含まない短い名前
    pub name: String,
    pub type_name: String,
    pub class_name: String,
}

impl ContainerIdentifier {
    pub(crate) fn new(
        broker_name: String,
        broker_type_name: String,
        name: String,
        type_name: String,
    ) -> Self {
        Self {
            broker_name,
            broker_type_name,
            namespace: String::new(),
            name,
            type_name,
            class_name: "container".to_owned(),
        }
    }

    /// ネームスペースを設定する (検査は [`ContainerIdentifier::validate`] で行う)
    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_owned();
        self
    }

    /// ネームスペースを含めた名前 (`<namespace>/<name>`)
    pub fn full_name(&self) -> String {
        join_namespace(&self.namespace, &self.name)
    }

    /// 名前とネームスペースに `/` `:` `|` が無いかを検査する
    pub fn validate(&self) -> JuizResult<()> {
        validate_namespace(&self.namespace)?;
        validate_name(&self.name)
    }

    pub fn to_string(&self) -> String {
        self.clone().into()
    }
}

impl Display for ContainerIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "ContainerIdentifier({}://{}/{}/{}::{})",
            self.broker_type_name,
            self.broker_name,
            self.class_name,
            self.full_name(),
            self.type_name
        ))
    }
}

impl TryFrom<String> for ContainerIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match crate::identifier::OBJECT_IDENTIFIER_REGEX.as_ref() {
            Ok(re) => match re.captures(value.as_str()) {
                Some(caps) => {
                    let class_name = caps[3].to_owned();
                    let type_name = caps[5].to_owned();
                    let (namespace, name) = split_namespace(&caps[4]);
                    let broker_name = caps[2].to_owned();
                    let broker_type_name = caps[1].to_owned();
                    Ok(Self {
                        class_name,
                        type_name,
                        namespace,
                        name,
                        broker_name,
                        broker_type_name,
                    })
                }
                None => {
                    log::error!("TryFrom<String> for ContainerIdentifier error. Invalid Identifier ({value}).");
                    return Err(anyhow!(JuizError::InvalidIdentifierError {
                        message: value
                    }));
                }
            },
            Err(e) => Err(anyhow!(e.clone())),
        }
    }
}

impl Into<String> for ContainerIdentifier {
    fn into(self) -> String {
        format!(
            "{}://{}/{}/{}::{}",
            self.broker_type_name,
            self.broker_name,
            self.class_name,
            self.full_name(),
            self.type_name
        )
    }
}
