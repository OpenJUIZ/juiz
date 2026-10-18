use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::{
    result::{JuizError, JuizResult},
    value::Value,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrokerIdentifier {
    pub name: String,
    pub type_name: String,
    pub broker_type_name: String,
    pub broker_name: String,
}

impl BrokerIdentifier {
    /// ```
    /// use juiz_sdk::prelude::*;
    /// fn main() -> JuizResult<()> {
    ///   let bi = BrokerIdentifier::new("broker0", "qmp", "http", "http0");
    ///   assert_eq!(bi.type_name, "qmp");
    ///   assert_eq!(bi.name, "broker0");
    ///   assert_eq!(bi.broker_type_name, "http");
    ///   assert_eq!(bi.broker_name, "http0");
    ///   Ok(())
    /// }
    ///
    /// ```
    pub fn new(name: &str, type_name: &str, broker_type_name: &str, broker_name: &str) -> Self {
        Self {
            name: name.to_owned(),
            type_name: type_name.to_owned(),
            broker_type_name: broker_type_name.to_owned(),
            broker_name: broker_name.to_owned(),
        }
    }

    pub fn to_string(&self) -> String {
        self.clone().into()
    }
}

/// ```
/// use juiz_sdk::prelude::*;
/// fn main() -> JuizResult<()> {
///   let arg_value = jvalue!({
///     "type_name": "qmp",
///     "name": "broker0",
///     "broker_type_name": "http",
///     "broker_name": "http0"
///   });
///   let bi: BrokerIdentifier = (&arg_value).try_into()?;
///   assert_eq!(bi.type_name, "qmp");
///   assert_eq!(bi.name, "broker0");
///   assert_eq!(bi.broker_type_name, "http");
///   assert_eq!(bi.broker_name, "http0");
///   Ok(())
/// }
///
/// ```
impl TryFrom<&Value> for BrokerIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        if value.is_string() {
            // もしstringならそれはIdentifier
            Self::try_from(value.as_str().unwrap())
        } else if let Some(value_obj) = value.as_object() {
            if let Some(value_id) = value_obj.get("identifier") {
                // 中にidentifierがあればidなので自動的にid化
                Self::try_from(value_id)
            } else {
                let type_name =
                    value_obj
                        .get("type_name")
                        .and_then(|v| v.as_str())
                        .ok_or(anyhow!(JuizError::InvalidValueError {
                            message: format!("BrokerIdentifier need type_name")
                        }))?;
                let name = value_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Some(s.to_owned()))
                    .or(Some(format!("{}0", type_name)))
                    .unwrap();
                let broker_name = value_obj
                    .get("broker_name")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Some(s.to_owned()))
                    .or(Some("core".to_owned()))
                    .unwrap();
                let broker_type_name = value_obj
                    .get("broker_type_name")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Some(s.to_owned()))
                    .or(Some("core".to_owned()))
                    .unwrap();
                Ok(Self::new(
                    name.as_str(),
                    type_name,
                    broker_type_name.as_str(),
                    broker_name.as_str(),
                ))
            }
        } else {
            Err(anyhow!(JuizError::InvalidValueError {
                message: format!("Value is not valid ProcessIdentifier.")
            }))
        }
    }
}

/// ```
/// use juiz_sdk::prelude::*;
/// fn main() -> JuizResult<()> {
///   let bi_str = "http://http0/broker/broker0::qmp";
///   let bi : BrokerIdentifier = bi_str.try_into()?;
///   assert_eq!(bi.type_name, "qmp", "broker's type_name");
///   assert_eq!(bi.name, "broker0", "broker's name");
///   assert_eq!(bi.broker_type_name, "http", "broker's broker_type_name");
///   assert_eq!(bi.broker_name, "http0", "broker's broker_name");
///
///   Ok(())
/// }
///
/// ```
impl TryFrom<&str> for BrokerIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (name, type_name, broker_type_name, broker_name) = parse_broker_identifier(value)?;
        Ok(Self::new(name, type_name, broker_type_name, broker_name))
    }
}

/// ブローカーの識別子を (name, type_name, broker_type_name, broker_name) に分解する。
///
/// 識別子は他のオブジェクトと同じく `{broker_type_name}://{broker_name}/broker/{name}::{type_name}` の形。
/// クラス名の無い旧形式 `{broker_type_name}://{broker_name}/{name}::{type_name}` も読み込める (issue #35)。
/// ```
/// use juiz_sdk::manifests::parse_broker_identifier;
/// fn main() -> juiz_sdk::prelude::JuizResult<()> {
///   assert_eq!(
///     parse_broker_identifier("http://localhost:8000/broker/0.0.0.0:8000::http")?,
///     ("0.0.0.0:8000", "http", "http", "localhost:8000"));
///   assert_eq!(
///     parse_broker_identifier("core://core/local::local")?,
///     ("local", "local", "core", "core"));
///   assert!(parse_broker_identifier("core://core/process/inc0::increment").is_err());
///   Ok(())
/// }
/// ```
pub fn parse_broker_identifier(value: &str) -> JuizResult<(&str, &str, &str, &str)> {
    static RE: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
        std::sync::LazyLock::new(|| {
            regex::Regex::new(r"^([^:/]+)://([^/]+)/(?:[Bb]roker/)?([^/]+?)::([^/]+)$")
        });
    let re = RE.as_ref().map_err(|e| anyhow::anyhow!(e.clone()))?;
    match re.captures(value) {
        Some(caps) => Ok((
            caps.get(3).unwrap().as_str(),
            caps.get(4).unwrap().as_str(),
            caps.get(1).unwrap().as_str(),
            caps.get(2).unwrap().as_str(),
        )),
        None => {
            log::error!("parse_broker_identifier error. Invalid Identifier ({value}).");
            Err(anyhow!(JuizError::InvalidIdentifierError {
                message: value.to_owned()
            }))
        }
    }
}

/// ```
/// use juiz_sdk::prelude::*;
/// fn main() -> JuizResult<()> {
///   let bi_str = "http://http0/broker/broker0::qmp".to_owned();
///   let bi : BrokerIdentifier = bi_str.try_into()?;
///   assert_eq!(bi.type_name, "qmp", "broker's type_name");
///   assert_eq!(bi.name, "broker0", "broker's name");
///   assert_eq!(bi.broker_type_name, "http", "broker's broker_type_name");
///   assert_eq!(bi.broker_name, "http0", "broker's broker_name");
///
///   Ok(())
/// }
///
/// ```
impl TryFrom<String> for BrokerIdentifier {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

/// ```
/// use juiz_sdk::prelude::*;
/// fn main() -> JuizResult<()> {
///   let bi = BrokerIdentifier::new("broker0", "qmp", "http", "http0");
///   assert_eq!(bi.type_name, "qmp");
///   assert_eq!(bi.name, "broker0");
///   assert_eq!(bi.broker_type_name, "http");
///   assert_eq!(bi.broker_name, "http0");
///   let bi_str: String = bi.into();
///   assert_eq!(bi_str, "http://http0/broker/broker0::qmp");
///   Ok(())
/// }
///
/// ```
impl Into<String> for BrokerIdentifier {
    fn into(self) -> String {
        format!(
            "{}://{}/broker/{}::{}",
            self.broker_type_name, self.broker_name, self.name, self.type_name
        )
    }
}
