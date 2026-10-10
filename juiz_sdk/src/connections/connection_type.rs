

use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ConnectionType {
    Pull,
    Push,
    Unknown,
}

impl ConnectionType {
    pub fn to_string(&self) -> String {
        match self {
            ConnectionType::Pull => "Pull".to_owned(),
            ConnectionType::Push => "Push".to_owned(),
            _ => "Unknown".to_owned(),
        }
    }
}

impl From<&str> for ConnectionType {
    fn from(value: &str) -> Self {
        match value {
            "pull" => ConnectionType::Pull,
            "PULL" => ConnectionType::Pull,
            "Pull" => ConnectionType::Pull,
            "push" => ConnectionType::Push,
            "Push" => ConnectionType::Push,
            "PUSH" => ConnectionType::Push,
            _ => ConnectionType::Unknown
        }
    }
}

impl Display for ConnectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ConnectionType({})", self.to_string()))
    }
}

// fn connection_type_from(typ_str_result: JuizResult<&str>) -> JuizResult<ConnectionType> {
//     if typ_str_result.is_err() {
//         return Ok(ConnectionType::Push);
//     }
//     let typ_str = typ_str_result.unwrap();
//     match typ_str {
//         "pull" => Ok(ConnectionType::Pull),
//         "push" => Ok(ConnectionType::Push),
//         _ => {
//             Err(anyhow::Error::from(JuizError::ConnectionTypeError { type_string: typ_str.to_string() }))
//         }
//     }
// }

