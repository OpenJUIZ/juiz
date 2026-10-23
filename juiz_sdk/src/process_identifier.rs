use std::fmt::Display;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::{
    identifier::{
        join_namespace, namespace_from_manifest, split_namespace, validate_name, validate_namespace,
    },
    result::{JuizError, JuizResult},
    value::Value,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessIdentifier {
    pub broker_name: String,
    pub broker_type_name: String,
    /// ネームスペース (`/` 区切り、空ならルート)。ContainerProcess は所属 Container と同じネームスペースに置く
    #[serde(default)]
    pub namespace: String,
    /// ネームスペースを含まない短い名前
    pub name: String,
    pub type_name: String,
    pub class_name: String,
    pub container_name: Option<String>,
}

impl ProcessIdentifier {
    pub fn new(
        broker_name: String,
        broker_type_name: String,
        type_name: String,
        container_name: Option<String>,
        name: String,
    ) -> Self {
        Self {
            class_name: if container_name.is_none() {
                "process".to_owned()
            } else {
                "container_process".to_owned()
            },
            broker_name,
            broker_type_name,
            namespace: String::new(),
            name,
            type_name,
            container_name: container_name,
        }
    }

    pub fn new_process_id(
        broker_name: String,
        broker_type_name: String,
        type_name: String,
        name: String,
    ) -> Self {
        Self {
            class_name: "process".to_owned(),
            broker_name,
            broker_type_name,
            namespace: String::new(),
            name,
            type_name,
            container_name: None,
        }
    }

    pub fn new_container_process_id(
        broker_name: String,
        broker_type_name: String,
        type_name: String,
        container_name: String,
        name: String,
    ) -> Self {
        Self {
            class_name: "container_process".to_owned(),
            broker_name,
            broker_type_name,
            namespace: String::new(),
            name,
            type_name: format!("{}:{}", type_name, container_name),
            container_name: Some(container_name),
        }
    }

    /// ネームスペースを設定する (検査は [`ProcessIdentifier::validate`] で行う)
    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_owned();
        self
    }

    /// ネームスペースを含めた名前 (`<namespace>/<name>`)
    pub fn full_name(&self) -> String {
        join_namespace(&self.namespace, &self.name)
    }

    /// 名前・ネームスペース・所属 Container 名に `/` `:` `|` が無いかを検査する
    pub fn validate(&self) -> JuizResult<()> {
        validate_namespace(&self.namespace)?;
        validate_name(&self.name)?;
        if let Some(container_name) = &self.container_name {
            validate_name(container_name)?;
        }
        Ok(())
    }

    /// 文字列 (ID) か manifest から作る。manifest のときは名前とネームスペースを検査する
    pub fn from_value(value: &Value) -> JuizResult<Self> {
        if value.is_string() {
            // もしstringならそれはIdentifier
            value.as_str().unwrap().try_into()
        } else if let Some(value_obj) = value.as_object() {
            if let Some(value_id) = value_obj.get("identifier") {
                // 中にidentifierがあればidなので自動的にid化
                return ProcessIdentifier::from_value(value_id);
            }
            let type_name = value_obj
                .get("type_name")
                .and_then(|v| v.as_str())
                .ok_or(anyhow!(JuizError::InvalidValueError {
                    message: format!("ProcessIdentifier need type_name")
                }))?
                .to_owned();
            let broker_type = value_obj
                .get("broker_type_name")
                .and_then(|v| v.as_str())
                .or(Some("core"))
                .unwrap()
                .to_owned();
            let broker_name = value_obj
                .get("broker_name")
                .and_then(|v| v.as_str())
                .or(Some("core"))
                .unwrap()
                .to_owned();
            let name = value_obj
                .get("name")
                .and_then(|v| v.as_str())
                .and_then(|s| Some(s.to_owned()))
                .or(Some(format!("{}0", type_name)))
                .unwrap();
            let container_name = value_obj
                .get("container_name")
                .and_then(|v| v.as_str())
                .and_then(|s| Some(s.to_owned()));
            let namespace = namespace_from_manifest(value)?;
            let id = Self::new(broker_name, broker_type, type_name, container_name, name)
                .with_namespace(&namespace);
            id.validate()?;
            Ok(id)
        } else {
            Err(anyhow!(JuizError::InvalidValueError {
                message: format!("Value is not valid ProcessIdentifier.")
            }))
        }
    }

    pub fn to_string(&self) -> String {
        self.clone().into()
    }
}

impl Display for ProcessIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "ProcessIdentifier({}://{}/{}/{}::{}",
            self.broker_type_name,
            self.broker_name,
            self.class_name,
            self.full_name(),
            self.type_name
        ))
    }
}

impl TryFrom<&str> for ProcessIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match crate::identifier::OBJECT_IDENTIFIER_REGEX.as_ref() {
            Ok(re) => {
                match re.captures(value) {
                    Some(caps) => {
                        let class_name = caps[3].to_owned();
                        let type_name = caps[5].to_owned();
                        let (namespace, name) = split_namespace(&caps[4]);
                        let broker_name = caps[2].to_owned();
                        let broker_type_name = caps[1].to_owned();
                        match class_name.as_str() {
                            "process" => Ok(Self::new_process_id(
                                broker_name,
                                broker_type_name,
                                type_name,
                                name,
                            )
                            .with_namespace(&namespace)),
                            "container_process" => {
                                let tokens = type_name.split(":").collect::<Vec<&str>>();
                                if tokens.len() != 2 {
                                    return Err(anyhow!(JuizError::InvalidIdentifierError{ message: format!("Invalid ContainerProcess type_name format. Value is '{}'", type_name)}));
                                }
                                let container_name = tokens[1].to_owned();
                                let container_process_type_name = tokens[0].to_owned();
                                Ok(Self::new_container_process_id(
                                    broker_name,
                                    broker_type_name,
                                    container_process_type_name,
                                    container_name,
                                    name,
                                )
                                .with_namespace(&namespace))
                            }
                            _ => Err(anyhow!(JuizError::InvalidIdentifierError {
                                message: format!("Invalid Class Name {class_name}")
                            })),
                        }
                    }
                    None => {
                        log::error!("TryFrom<String> for ProcessIdentifier error. Invalid Identifier ({value}).");
                        return Err(anyhow!(JuizError::InvalidIdentifierError {
                            message: value.to_owned()
                        }));
                    }
                }
            }
            Err(e) => Err(anyhow!(e.clone())),
        }
    }
}

impl TryFrom<String> for ProcessIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl Into<String> for ProcessIdentifier {
    fn into(self) -> String {
        // ContainerProcess の型名の後ろの「:Container 名」にはネームスペースを付けない
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
