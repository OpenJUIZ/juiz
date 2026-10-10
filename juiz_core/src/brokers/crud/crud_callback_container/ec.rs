
use juiz_sdk::{anyhow::anyhow, log, prelude::*};
use std::str::FromStr;
use crate::brokers::ExecutionContextBrokerProxy;

use super::CallbackContainerType;

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
 
    let mut ec_callbacks = CallbackContainerType::new();
    ec_callbacks.insert("create",  |_crud, cb, args| {
        log::debug!("[CREATE] ec/create called");
        let manifest = args.get("__manifest__")?.extract_value()?;
        Ok(cb.lock_mut()?.ec_create(&manifest)?.into())}
    );
    ec_callbacks
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut ec_cbs = CallbackContainerType::new();
    ec_cbs.insert("profile_full", |_crud,cb, args| {
        log::debug!("[READ  ] ec/profile_full called");
        let id = args.get_param("identifier").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "identifier".to_owned() })})?;
        Ok(value_to_capsule(cb.lock()?.ec_profile_full(id)?))
    });
    ec_cbs.insert("list", |_crud,cb, args| {
        log::debug!("[READ  ] ec/list called");
        let recursive_str = args.get_param("recursive").and_then(|v|{Some(v.clone())}).or_else(||{Some("false".to_owned())}).unwrap();
        let recursive: bool = FromStr::from_str(recursive_str.as_str())?;
        Ok(value_to_capsule(cb.lock()?.ec_list(recursive)?))
    });
    ec_cbs.insert("get_state", |_crud,cb, args| {
        log::debug!("[READ  ] ec/get_state called");
        let id = args.get_param("identifier").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "identifier".to_owned() })})?;
        Ok(value_to_capsule(cb.lock()?.ec_get_state(id)?))
    });
    ec_cbs
}

pub(crate) fn create_update_callbacks() -> CallbackContainerType {
    let mut ec_cbs = CallbackContainerType::new();
    ec_cbs.insert("start", |_crud,cb, args| {
        log::debug!("[UPDATE] ec/start called");
        let id = args.get_param("identifier").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "identifier".to_owned() })})?;
        Ok(value_to_capsule(cb.lock_mut()?.ec_start(id)?))
    });
    ec_cbs.insert("stop", |_crud,cb, args| {
        log::debug!("[UPDATE] ec/stop called");
        let id = args.get_param("identifier").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "identifier".to_owned() })})?;
        Ok(value_to_capsule(cb.lock_mut()?.ec_stop(id)?))
    });
    ec_cbs
}
    