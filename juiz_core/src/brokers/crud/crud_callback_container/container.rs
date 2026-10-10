use juiz_sdk::{anyhow::anyhow, manifests::ContainerIdentifier, log, manifests::ContainerProfile, prelude::*};
use std::str::FromStr;
use crate::brokers::ContainerBrokerProxy;

use super::CallbackContainerType;

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
    let mut container_callbacks = CallbackContainerType::new();
    container_callbacks.insert("create",  |_crud, cb, args| {
        log::debug!("[CREATE] container/create called");
        let manifest: ContainerManifest = serde_json::from_value(args.get("__manifest__")?.extract_value()?)?;
        let profile: ContainerProfile = cb.lock_mut()?.container_create(&manifest, args)?;
        Ok(serde_json::to_value(profile)?.into())}
    );
    container_callbacks
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut cont_cbs = CallbackContainerType::new();
    cont_cbs.insert("profile_full", |_crud,cb, args| {
        log::debug!("[READ  ] container/profile_full called");
        let container_id: ContainerIdentifier = args.get_param("identifier").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "identifier".to_owned() })})?.clone().try_into()?;
        let profile: ContainerProfile = cb.lock()?.container_profile_full(&container_id)?;
        Ok(serde_json::to_value(profile)?.into())
    });
    cont_cbs.insert("list", |_crud,cb, args| {
        log::debug!("[READ  ] container/list called");
        let recursive: bool = bool::from_str(args.get_param("recursive").and_then(|v|{Some(v.clone())}).or_else(||{Some("false".to_owned())}).unwrap().as_str())?;
        let profiles = cb.lock()?.container_list(recursive, None)?;
        Ok(profiles.into_iter().map(|id| { serde_json::to_value(id) }).collect::<serde_json::Result<Value>>()?.into())
    });
    cont_cbs
}

pub(crate) fn create_delete_callbacks() -> CallbackContainerType {
    let mut cont_cbs = CallbackContainerType::new();
    cont_cbs.insert("destroy", |_crud,cb, args| {
        log::debug!("[UPDATE] container/destroy called");
        let id: ContainerIdentifier = args.get_param("identifier").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "identifier".to_owned() })})?.clone().try_into()?;
        let v = cb.lock_mut()?.container_destroy(&id)?;
        Ok(serde_json::to_value(v)?.into())
    });
    cont_cbs
}