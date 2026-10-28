//! 実行中に Composite を定義する API (issue #171)
//!
//! - `composite/create` (ボディ `{"__manifest__": <Composite の定義>}`)、`composite/list`、
//!   `composite/profile_full?identifier=<型名>`、`composite/destroy?identifier=<型名>`
//! - `composite_container/create` (ボディ `{"__manifest__": <composite_containers: の 1 件>}`)。
//!   CompositeContainer の削除と一覧は container/destroy・container/list
use crate::brokers::CompositeBrokerProxy;
use juiz_sdk::{anyhow::anyhow, log, manifests::CompositeManifest, prelude::*};

use super::CallbackContainerType;
use crate::core::container_profile_value;

fn type_name_param(args: &CapsuleMap) -> JuizResult<String> {
    args.get_param("identifier").cloned().ok_or_else(|| {
        anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
            key_name: "identifier".to_owned()
        })
    })
}

fn manifest_arg(args: &CapsuleMap) -> JuizResult<Value> {
    args.get("__manifest__")?.extract_value()
}

pub(crate) fn create_create_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("create", |_crud, cb, args| {
        log::debug!("[CREATE] composite/create called");
        let value = manifest_arg(&args)?;
        let manifest: CompositeManifest = serde_json::from_value(value.clone()).map_err(|e| {
            anyhow!(JuizError::InvalidArgumentError {
                message: format!("Composite definition is invalid: {e} ({value})"),
            })
        })?;
        let created = cb.lock_mut()?.composite_create(&manifest)?;
        Ok(serde_json::to_value(created)?.into())
    });
    cbs
}

pub(crate) fn create_container_create_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("create", |_crud, cb, args| {
        log::debug!("[CREATE] composite_container/create called");
        let profile = cb
            .lock_mut()?
            .composite_container_create(&manifest_arg(&args)?)?;
        // container/profile_full と同じく identifier と class_name を添える
        Ok(container_profile_value(&profile)?.into())
    });
    cbs
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |_crud, cb, _args| {
        log::debug!("[READ  ] composite/list called");
        Ok(serde_json::to_value(cb.lock()?.composite_list()?)?.into())
    });
    cbs.insert("profile_full", |_crud, cb, args| {
        log::debug!("[READ  ] composite/profile_full called");
        let manifest = cb
            .lock()?
            .composite_profile_full(&type_name_param(&args)?)?;
        Ok(serde_json::to_value(manifest)?.into())
    });
    cbs
}

pub(crate) fn create_delete_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("destroy", |_crud, cb, args| {
        log::debug!("[DELETE] composite/destroy called");
        let manifest = cb.lock_mut()?.composite_destroy(&type_name_param(&args)?)?;
        Ok(serde_json::to_value(manifest)?.into())
    });
    cbs
}
