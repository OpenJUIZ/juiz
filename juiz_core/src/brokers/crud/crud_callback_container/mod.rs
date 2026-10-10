
mod process;
mod system;
mod container;
mod container_process;
mod topic;
mod ec;
mod connection;
mod broker;

use std::collections::HashMap;


use super::{super::core_broker::CoreBrokerPtr, CRUDBroker};
use crate::prelude::*;

pub type CBFnType = fn(&CRUDBroker, CoreBrokerPtr, CapsuleMap)->JuizResult<CapsulePtr>;
pub type CallbackContainerType = HashMap<&'static str, CBFnType>;
pub type ClassCallbackContainerType = HashMap<&'static str, CallbackContainerType>;



pub(crate) fn create_callback_container() -> ClassCallbackContainerType {

    fn _extract_connection_create_parameter(args: CapsuleMap) -> JuizResult<ConnectionManifest> {
        log::debug!("extract_connection_create_param({args:?})");
        let v = Into::<Value>::into(args);
        log::debug!(" - value: {v:?}");
        return v.try_into();
        //return args.get("map")?.try_into().or_else(|e|{Err(anyhow::Error::from(e))})
    }

    fn _extract_create_parameter(args: CapsuleMap) -> JuizResult<Value> {
        log::debug!("extract_create_param({args:?})");
        let v = args.into();
        log::debug!(" - value: {v:?}");
        return Ok(v);
        //return args.get("map")?.try_into().or_else(|e|{Err(anyhow::Error::from(e))})
    }

    let mut create_cb_container: HashMap<&str, HashMap<&str, CBFnType>> = ClassCallbackContainerType::new();

    create_cb_container.insert("process", process::create_create_callbacks());

    create_cb_container.insert("container", container::create_create_callbacks());
    
    create_cb_container.insert("container_process", container_process::create_create_callbacks());

    create_cb_container.insert("execution_context", ec::create_create_callbacks());
    
    create_cb_container.insert("connection", connection::create_create_callbacks());

    create_cb_container
}


pub(crate) fn read_callback_container() -> ClassCallbackContainerType {
    let mut read_cb_container = ClassCallbackContainerType::new();

    // システム関連
    read_cb_container.insert("system", system::create_read_callbacks());

    // プロセス関連
    read_cb_container.insert("process", process::create_read_callbacks());

    // コンテナ関連
    read_cb_container.insert("container", container::create_read_callbacks());

    // コンテナプロセス関連
    read_cb_container.insert("container_process", container_process::create_read_callbacks());
    
    read_cb_container.insert("broker", broker::create_read_callbacks());

    read_cb_container.insert("topic", topic::create_read_calbacks());

    // コネクション関連
    read_cb_container.insert("connection", connection::create_read_callbacks());
    
    read_cb_container.insert("execution_context", ec::create_read_callbacks());

    read_cb_container
}



pub(crate) fn update_callback_container() -> ClassCallbackContainerType {
    let mut update_cb_container = ClassCallbackContainerType::new();

    // システム関連
    update_cb_container.insert("system", system::create_update_callbacks());

    // プロセス関連
    update_cb_container.insert("process", process::create_update_callbacks());

    // コンテナプロセス関連
    update_cb_container.insert("container_process", container_process::create_update_callbacks());

    // EC関連
    update_cb_container.insert("execution_context", ec::create_update_callbacks());

    // トピック関連
    update_cb_container.insert("topic", topic::create_update_calbacks());

    update_cb_container
}


pub(crate) fn delete_callback_container() -> ClassCallbackContainerType {
    let mut delete_cb_container = ClassCallbackContainerType::new();

    delete_cb_container.insert("process", process::create_delete_callbacks());
    
    delete_cb_container.insert("container_process", container_process::create_delete_callbacks());

    delete_cb_container.insert("container", container::create_delete_callbacks());

    delete_cb_container
}