use crate::brokers::ConnectionBrokerProxy;
use juiz_sdk::{
    anyhow::anyhow, connection_identifier::ConnectionIdentifier, connections::ConnectionProfile,
    log, prelude::*,
};
use std::str::FromStr;

use super::CallbackContainerType;

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
    let mut connection_callbacks = CallbackContainerType::new();
    connection_callbacks.insert("create", |_crud, cb, args| {
        log::debug!("[CREATE] connection/create called");
        // 識別子は struct でも文字列でも受け付ける
        let value = args.get("__manifest__")?.extract_value()?;
        let manifest: ConnectionManifest = match serde_json::from_value(value.clone()) {
            Ok(manifest) => manifest,
            Err(_) => value.try_into()?,
        };
        let profile: ConnectionProfile = cb.lock_mut()?.connection_create(&manifest)?;
        Ok(serde_json::to_value(profile)?.into())
    });
    connection_callbacks
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut con_cbs = CallbackContainerType::new();
    con_cbs.insert("profile_full", |_crud, cb, args| {
        log::debug!("[READ  ] connection/profile_full called");
        let id: ConnectionIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        let profile: ConnectionProfile = cb.lock()?.connection_profile_full(&id)?;
        Ok(serde_json::to_value(profile)?.into())
    });
    con_cbs.insert("list", |_crud, cb, args| {
        log::debug!("[READ  ] connection/list called");
        let recursive_str = args
            .get_param("recursive")
            .and_then(|v| Some(v.clone()))
            .or_else(|| Some("false".to_owned()))
            .unwrap();
        let recursive: bool = FromStr::from_str(recursive_str.as_str())?;
        Ok(value_to_capsule(
            cb.lock()?
                .connection_list(recursive)?
                .into_iter()
                .map(|v| -> String { v.into() })
                .collect::<Vec<String>>()
                .into(),
        ))
    });
    con_cbs
}

pub(crate) fn create_delete_callbacks() -> CallbackContainerType {
    let mut con_cbs = CallbackContainerType::new();
    con_cbs.insert("destroy", |_crud, cb, args| {
        log::debug!("[DELETE] connection/destroy called");
        let id: ConnectionIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        let profile: ConnectionProfile = cb.lock_mut()?.connection_destroy(&id)?;
        Ok(serde_json::to_value(profile)?.into())
    });
    con_cbs
}
