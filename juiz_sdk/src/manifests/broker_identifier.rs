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
///   let bi_str = "http://http0/broker0::qmp";
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
        match regex::Regex::new(r"^(.+?)://(.+?)/(.+?)::(.+?)$") {
            Ok(re) => match re.captures(value) {
                Some(caps) => Ok(Self::new(
                    caps[3].as_ref(),
                    caps[4].as_ref(),
                    caps[1].as_ref(),
                    caps[2].as_ref(),
                )),
                None => {
                    log::error!("TryFrom<String> for ProcessIdentifier error. Invalid Identifier ({value}).");
                    Err(anyhow!(JuizError::InvalidIdentifierError {
                        message: value.to_owned()
                    }))
                }
            },
            Err(e) => Err(anyhow!(e)),
        }
    }
}

/// ```
/// use juiz_sdk::prelude::*;
/// fn main() -> JuizResult<()> {
///   let bi_str = "http://http0/broker0::qmp".to_owned();
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
///   assert_eq!(bi_str, "http://http0/broker0::qmp");
///   Ok(())
/// }
///
/// ```
impl Into<String> for BrokerIdentifier {
    fn into(self) -> String {
        format!(
            "{}://{}/{}::{}",
            self.broker_type_name, self.broker_name, self.name, self.type_name
        )
    }
}
