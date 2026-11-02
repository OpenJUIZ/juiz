use crate::brokers::ExecutionContextBrokerProxy;
use juiz_sdk::{anyhow::anyhow, log, prelude::*};
use std::str::FromStr;

use super::{CallbackContainerType, NamespaceFilter};

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
    let mut ec_callbacks = CallbackContainerType::new();
    ec_callbacks.insert("create", |cb, args| {
        log::debug!("[CREATE] ec/create called");
        let manifest = args.get("__manifest__")?.extract_value()?;
        Ok(cb.lock_mut()?.ec_create(&manifest)?.into())
    });
    ec_callbacks
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut ec_cbs = CallbackContainerType::new();
    ec_cbs.insert("profile_full", |cb, args| {
        log::debug!("[READ  ] ec/profile_full called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(value_to_capsule(cb.lock()?.ec_profile_full(id)?))
    });
    ec_cbs.insert("list", |cb, args| {
        log::debug!("[READ  ] ec/list called");
        let recursive_str = args
            .get_param("recursive")
            .and_then(|v| Some(v.clone()))
            .or_else(|| Some("false".to_owned()))
            .unwrap();
        let recursive: bool = FromStr::from_str(recursive_str.as_str())?;
        let filter = NamespaceFilter::from_args(&args)?;
        let ids = cb.lock()?.ec_list(recursive)?;
        let ids = get_array(&ids)?
            .iter()
            .filter(|v| {
                // 識別子として読めないものは絞り込むときだけ落とす
                v.as_str()
                    .and_then(|id| IdentifierStruct::try_from(id.to_owned()).ok())
                    .map_or(!filter.is_active(), |id| filter.matches(&id.namespace))
            })
            .cloned()
            .collect::<Vec<Value>>();
        Ok(value_to_capsule(Value::from(ids)))
    });
    ec_cbs.insert("get_state", |cb, args| {
        log::debug!("[READ  ] ec/get_state called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(value_to_capsule(cb.lock()?.ec_get_state(id)?))
    });
    ec_cbs
}

pub(crate) fn create_update_callbacks() -> CallbackContainerType {
    let mut ec_cbs = CallbackContainerType::new();
    ec_cbs.insert("start", |cb, args| {
        log::debug!("[UPDATE] ec/start called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(value_to_capsule(cb.lock_mut()?.ec_start(id)?))
    });
    ec_cbs.insert("stop", |cb, args| {
        log::debug!("[UPDATE] ec/stop called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(value_to_capsule(cb.lock_mut()?.ec_stop(id)?))
    });
    ec_cbs
}

pub(crate) fn create_delete_callbacks() -> CallbackContainerType {
    let mut ec_cbs = CallbackContainerType::new();
    ec_cbs.insert("destroy", |cb, args| {
        log::debug!("[DELETE] ec/destroy called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(value_to_capsule(cb.lock_mut()?.ec_destroy(id)?))
    });
    ec_cbs
}
