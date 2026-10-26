mod any_process;
mod broker;
mod composite;
mod connection;
mod container;
mod container_process;
mod ec;
mod process;
mod system;
mod topic;

use std::collections::HashMap;

use super::{super::core_broker::CoreBrokerPtr, CRUDBroker};
use crate::prelude::*;

/// list で返す識別子のうち core を指すものを、呼び出しに使われたブローカー (http なら host:port) を指す形に書き換える。
///
/// HTTP などから呼ばれたときだけ accessed_broker_* が付くので、それ以外 (メッセンジャー系など) では書き換えない (issue #36)。
/// process / container / container_process の list でそろえる (issue #112)。
pub(crate) fn rewrite_to_accessed_broker(
    args: &CapsuleMap,
    broker_name: &mut String,
    broker_type_name: &mut String,
) {
    if let (Some(name), Some(type_name)) = (
        args.get_param("accessed_broker_name"),
        args.get_param("accessed_broker_type"),
    ) {
        if broker_name == "core" && broker_type_name == "core" {
            *broker_name = name.clone();
            *broker_type_name = type_name.clone();
        }
    }
}

/// list の `namespace` パラメータで絞り込む (issue #150)。
///
/// HTTP はクエリ `?namespace=robot1`、QMP などは param の `namespace`。指定したネームスペースとその下が残る。
/// 無い・空・`/` なら絞り込まない。不正なネームスペースはエラー。
pub(crate) struct NamespaceFilter(Option<String>);

impl NamespaceFilter {
    pub(crate) fn from_args(args: &CapsuleMap) -> JuizResult<Self> {
        let Some(ns) = args.get_param("namespace") else {
            return Ok(NamespaceFilter(None));
        };
        let ns = ns.strip_prefix('/').unwrap_or(ns);
        if ns.is_empty() {
            return Ok(NamespaceFilter(None));
        }
        juiz_sdk::identifier::validate_namespace(ns)?;
        Ok(NamespaceFilter(Some(ns.to_owned())))
    }

    pub(crate) fn is_active(&self) -> bool {
        self.0.is_some()
    }

    pub(crate) fn matches(&self, namespace: &str) -> bool {
        match &self.0 {
            None => true,
            Some(filter) => juiz_sdk::identifier::is_in_namespace(namespace, filter),
        }
    }
}

pub type CBFnType = fn(&CRUDBroker, CoreBrokerPtr, CapsuleMap) -> JuizResult<CapsulePtr>;
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

    let mut create_cb_container: HashMap<&str, HashMap<&str, CBFnType>> =
        ClassCallbackContainerType::new();

    create_cb_container.insert("process", process::create_create_callbacks());

    create_cb_container.insert("container", container::create_create_callbacks());

    create_cb_container.insert(
        "container_process",
        container_process::create_create_callbacks(),
    );

    create_cb_container.insert("execution_context", ec::create_create_callbacks());

    create_cb_container.insert("connection", connection::create_create_callbacks());

    // 実行中の Composite の定義 (issue #171)
    create_cb_container.insert("composite", composite::create_create_callbacks());
    create_cb_container.insert(
        "composite_container",
        composite::create_container_create_callbacks(),
    );

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
    read_cb_container.insert(
        "container_process",
        container_process::create_read_callbacks(),
    );

    // Process と ContainerProcess をまとめたもの
    read_cb_container.insert("any_process", any_process::create_read_callbacks());

    read_cb_container.insert("broker", broker::create_read_callbacks());

    read_cb_container.insert("topic", topic::create_read_calbacks());

    // コネクション関連
    read_cb_container.insert("connection", connection::create_read_callbacks());

    read_cb_container.insert("execution_context", ec::create_read_callbacks());

    read_cb_container.insert("composite", composite::create_read_callbacks());

    read_cb_container
}

pub(crate) fn update_callback_container() -> ClassCallbackContainerType {
    let mut update_cb_container = ClassCallbackContainerType::new();

    // システム関連
    update_cb_container.insert("system", system::create_update_callbacks());

    // プロセス関連
    update_cb_container.insert("process", process::create_update_callbacks());

    // コンテナプロセス関連
    update_cb_container.insert(
        "container_process",
        container_process::create_update_callbacks(),
    );

    // EC関連
    update_cb_container.insert("execution_context", ec::create_update_callbacks());

    // トピック関連
    update_cb_container.insert("topic", topic::create_update_calbacks());

    update_cb_container
}

pub(crate) fn delete_callback_container() -> ClassCallbackContainerType {
    let mut delete_cb_container = ClassCallbackContainerType::new();

    delete_cb_container.insert("process", process::create_delete_callbacks());

    delete_cb_container.insert(
        "container_process",
        container_process::create_delete_callbacks(),
    );

    delete_cb_container.insert("container", container::create_delete_callbacks());

    // EC関連
    delete_cb_container.insert("execution_context", ec::create_delete_callbacks());

    // コネクション関連
    delete_cb_container.insert("connection", connection::create_delete_callbacks());

    delete_cb_container.insert("composite", composite::create_delete_callbacks());

    delete_cb_container
}
