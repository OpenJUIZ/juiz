use crate::brokers::{BrokerProxy, ProcessBrokerProxy};
use juiz_sdk::{
    anyhow::anyhow, log, manifests::ProcessProfile, prelude::*,
    process_identifier::ProcessIdentifier, serde_json,
};
use std::str::FromStr;

use super::{rewrite_to_accessed_broker, CallbackContainerType, NamespaceFilter};
use crate::core::process_profile_value;

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
    let mut process_callbacks = CallbackContainerType::new();
    process_callbacks.insert("create", |cb, args| {
        log::debug!("[CREATE] process/create called");
        let manifest: ProcessManifest =
            serde_json::from_value(args.get("__manifest__")?.extract_value()?)?;
        let profile: ProcessProfile = cb.lock_mut()?.process_create(&manifest)?;
        Ok(serde_json::to_value(profile)?.into())
    });
    // create_cb_container.insert("process", process_callbacks);
    process_callbacks
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut proc_cbs = CallbackContainerType::new();
    proc_cbs.insert("profile_full", |cb, args| {
        log::debug!("[READ  ] process/profile_full called");
        let process_id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        let profile: ProcessProfile = cb.lock()?.process_profile_full(&process_id)?;
        Ok(process_profile_value(&profile)?.into())
    });
    proc_cbs.insert("is_in_charge", |cb, args| {
        log::debug!("[READ  ] process/is_in_charge called");
        let id = args.get_param("identifier").ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            })
        })?;
        Ok(jvalue!(cb.lock()?.is_in_charge_for_process(id)?).into())
    });
    proc_cbs.insert("list", |cb, args| {
        let accessed_broker_id = args.get_param("accessed_broker_id");
        log::debug!(
            "[READ  ] process/listが呼ばれました。accessed_broker_idは{accessed_broker_id:?}です。"
        );
        let caller_broker_profile = accessed_broker_id.and_then(|id| {
            Some(jvalue!(
                {"identifier": id,
                "type_name": args.get_param("accessed_broker_type").unwrap(),
                "name": args.get_param("accessed_broker_name").unwrap(),
            }))
        });
        let recursive: bool = bool::from_str(
            args.get_param("recursive")
                .and_then(|v| Some(v.clone()))
                .or_else(|| Some("false".to_owned()))
                .unwrap()
                .as_str(),
        )?;
        let filter = NamespaceFilter::from_args(&args)?;
        let profiles: Vec<ProcessIdentifier> =
            cb.lock()?.process_list(recursive, caller_broker_profile)?;
        Ok(Value::from(
            profiles
                .into_iter()
                .filter(|id| filter.matches(&id.namespace))
                .map(|mut id| {
                    rewrite_to_accessed_broker(
                        &args,
                        &mut id.broker_name,
                        &mut id.broker_type_name,
                    );
                    serde_json::to_value(id)
                })
                .collect::<serde_json::Result<Value>>()?,
        )
        .into())

        // Ok(Value::from(profiles.into_iter().map(|id| { serde_json::to_value(id) }).collect::<serde_json::Result<Value>>()?).into())
    });
    proc_cbs.insert("openapi_spec", |cb, args| {
        log::debug!("[READ  ] process/openapi_spec");
        let process_id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        Ok(cb.lock()?.process_openapi_spec(&process_id)?.into())
    });
    proc_cbs
}

pub(crate) fn create_update_callbacks() -> CallbackContainerType {
    let mut proc_cbs = CallbackContainerType::new();
    proc_cbs.insert("call", |cb, args| {
        log::debug!("[UPDATE] process/call called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        cb.lock()?.process_call(&id, args)
    });
    proc_cbs.insert("execute", |cb, args| {
        log::debug!("[UPDATE] process/execute called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        cb.lock()?.process_execute(&id)
    });
    proc_cbs.insert("p_apply", |cb, args| {
        log::debug!("[UPDATE] process/p_apply called");
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
            .process_p_apply(&id, arg_name.as_str(), value)
    });
    proc_cbs.insert("try_connect_to", |cb, args| {
        log::debug!("[UPDATE] process/try_connect_toが呼ばれました");
        let cm: ConnectionManifest = match args.try_into() {
            Ok(cm) => Ok(cm),
            Err(e) =>  {
                log::error!("【try_connect_to】ConnectionManifestへのデータ変換時にエラーが起こりました。エラー({e:?})");
                Err(e)
            }
        }?;
        let cm2 = cb.lock_mut()?.process_try_connect_to(&cm)?;
        Ok(serde_json::to_value(cm2)?.into())
    });
    proc_cbs.insert("notify_connected_from", |cb, args| {
        log::debug!("【呼出】process/notify_connected_from({args:})");
        let cm: ConnectionManifest = match args.try_into() {
            Ok(cm) => Ok(cm),
            Err(e) => {
                log::error!(
                    "callback for 'notify_connected_from' failed in data conversion. Error({e:?})"
                );
                Err(e)
            }
        }?;
        log::debug!("【notify_connect_from】ConnectionManifestの変換成功。({cm})");
        let cp = cb.lock_mut()?.process_notify_connected_from(&cm)?;
        Ok(serde_json::to_value(cp)?.into())
    });
    proc_cbs.insert("push_by", |cb, args| {
        log::debug!("[UPDATE] process/push_by called");
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
            .unwrap()
            .to_owned();
        Ok(cb.lock_mut()?.process_push_by(&id, arg_name, value)?)
    });
    proc_cbs
}

pub(crate) fn create_delete_callbacks() -> CallbackContainerType {
    let mut proc_cbs = CallbackContainerType::new();
    proc_cbs.insert("destroy", |cb, args| {
        log::debug!("[UPDATE] process/destroy called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .as_str()
            .try_into()?;
        let v = cb.lock_mut()?.process_destroy(&id)?;
        Ok(serde_json::to_value(v)?.into())
    });
    proc_cbs
}
