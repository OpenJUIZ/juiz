use crate::brokers::BrokerBrokerProxy;
use juiz_sdk::{anyhow::anyhow, log, prelude::*};
use std::str::FromStr;

use super::CallbackContainerType;

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    //
    let mut broker_cbs = CallbackContainerType::new();
    broker_cbs.insert("profile_full", |_crud, cb, args| {
        log::debug!("[READ  ] broker/profile_full called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(value_to_capsule(cb.lock()?.broker_profile_full(id)?.into()))
    });
    broker_cbs.insert("list", |_crud, cb, args| {
        log::debug!("[READ  ] broker/list called");
        let recursive: bool = bool::from_str(
            args.get_param("recursive")
                .and_then(|v| Some(v.clone()))
                .or_else(|| Some("false".to_owned()))
                .unwrap()
                .as_str(),
        )?;
        let ids: Vec<String> = cb.lock()?.broker_list(recursive)?;
        Ok(Value::from(ids).into())
    });
    broker_cbs
}
