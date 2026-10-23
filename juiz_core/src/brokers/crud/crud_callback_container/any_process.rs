//! Process と ContainerProcess をまとめて扱う any_process (issue #112)。
//! OpenAPI には載っていたが、CRUDBroker にコールバックが無く 500 を返していた。
use crate::brokers::BrokerProxy;
use juiz_sdk::{
    anyhow::anyhow, manifests::ProcessProfile, prelude::*, process_identifier::ProcessIdentifier,
};
use std::str::FromStr;

use super::{rewrite_to_accessed_broker, CallbackContainerType, NamespaceFilter};
use crate::core::process_profile_value;

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut any_cbs = CallbackContainerType::new();
    any_cbs.insert("profile_full", |_crud, cb, args| {
        log::debug!("[READ  ] any_process/profile_full called");
        let id: ProcessIdentifier = args
            .get_param("identifier")
            .ok_or_else(|| {
                anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                    key_name: "identifier".to_owned()
                })
            })?
            .clone()
            .try_into()?;
        let profile: ProcessProfile = cb.lock()?.any_process_profile_full(&id)?;
        Ok(process_profile_value(&profile)?.into())
    });
    any_cbs.insert("list", |_crud, cb, args| {
        log::debug!("[READ  ] any_process/list called");
        let recursive: bool = bool::from_str(
            args.get_param("recursive")
                .and_then(|v| Some(v.clone()))
                .or_else(|| Some("false".to_owned()))
                .unwrap()
                .as_str(),
        )?;
        let filter = NamespaceFilter::from_args(&args)?;
        let ids: Vec<ProcessIdentifier> = cb.lock()?.any_process_list(recursive, None)?;
        Ok(ids
            .into_iter()
            .filter(|id| filter.matches(&id.namespace))
            .map(|mut id| {
                rewrite_to_accessed_broker(&args, &mut id.broker_name, &mut id.broker_type_name);
                serde_json::to_value(id)
            })
            .collect::<serde_json::Result<Value>>()?
            .into())
    });
    any_cbs
}
