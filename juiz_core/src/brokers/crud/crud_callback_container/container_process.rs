use crate::brokers::ContainerProcessBrokerProxy;
use juiz_sdk::{
    anyhow::anyhow, manifests::ContainerIdentifier, manifests::ProcessProfile, prelude::*,
    process_identifier::ProcessIdentifier,
};
use std::str::FromStr;

use super::{rewrite_to_accessed_broker, CallbackContainerType, NamespaceFilter};
use crate::core::process_profile_value;

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
    let mut container_process_callbacks = CallbackContainerType::new();
    container_process_callbacks.insert("create", |cb, args| {
        log::debug!("[CREATE] container_process/create called");
        let container_id: ContainerIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        let manifest: ProcessManifest =
            serde_json::from_value(args.get("__manifest__")?.extract_value()?)?;
        let profile: ProcessProfile = cb
            .lock_mut()?
            .container_process_create(&container_id, &manifest)?;
        Ok(serde_json::to_value(profile)?.into())
    });
    container_process_callbacks
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut cpro_cbs = CallbackContainerType::new();
    cpro_cbs.insert("profile_full", |cb, args| {
        log::debug!("[READ  ] container_process/profile_full called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        let profile: ProcessProfile = cb.lock()?.container_process_profile_full(&id)?;
        Ok(process_profile_value(&profile)?.into())
    });
    cpro_cbs.insert("list", |cb, args| {
        log::debug!("[READ  ] container_process/list called");
        let recursive: bool = bool::from_str(
            args.get_param("recursive")
                .and_then(|v| Some(v.clone()))
                .or_else(|| Some("false".to_owned()))
                .unwrap()
                .as_str(),
        )?;
        let profiles: Vec<ProcessIdentifier> =
            cb.lock()?.container_process_list(recursive, None)?;
        let filter = NamespaceFilter::from_args(&args)?;
        Ok(profiles
            .into_iter()
            .filter(|id| filter.matches(&id.namespace))
            .map(|mut id| {
                rewrite_to_accessed_broker(&args, &mut id.broker_name, &mut id.broker_type_name);
                serde_json::to_value(id)
            })
            .collect::<serde_json::Result<Value>>()?
            .into())
    });
    cpro_cbs.insert("openapi_spec", |cb, args| {
        log::debug!("[READ  ] container_process/openapi_spec");
        let process_id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        Ok(cb
            .lock()?
            .container_process_openapi_spec(&process_id)?
            .into())
    });
    cpro_cbs
}

pub(crate) fn create_update_callbacks() -> CallbackContainerType {
    let mut cont_proc_cbs = CallbackContainerType::new();
    cont_proc_cbs.insert("call", |cb, args| {
        log::debug!("[UPDATE] container_process/call called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        cb.lock()?.container_process_call(&id, args)
    });
    cont_proc_cbs.insert("execute", |cb, args| {
        log::debug!("[UPDATE] container_process/execute called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        cb.lock()?.container_process_execute(&id)
    });
    cont_proc_cbs.insert("p_apply", |cb, args| {
        log::debug!("[UPDATE] container_process/p_apply called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        let value = args.get("value")?;
        let arg_name = args
            .get("arg_name")?
            .extract_value()?
            .as_str()
            .and_then(|s| Some(s.to_owned()))
            .ok_or(anyhow!(JuizError::ArgumentError {
                message: "arg_name value is required".to_owned()
            }))?;
        cb.lock_mut()?
            .container_process_p_apply(&id, arg_name.as_str(), value)
    });
    cont_proc_cbs
}

pub(crate) fn create_delete_callbacks() -> CallbackContainerType {
    let mut cont_proc_cbs = CallbackContainerType::new();
    cont_proc_cbs.insert("destroy", |cb, args| {
        log::debug!("[UPDATE] container_process/destroy called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        let v = cb.lock_mut()?.container_process_destroy(&id)?;
        Ok(serde_json::to_value(v)?.into())
    });
    cont_proc_cbs
}
