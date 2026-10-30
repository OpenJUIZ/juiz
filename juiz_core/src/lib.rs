//! juizを使ったアプリケーション開発のための機能パッケージ。機能要素を開発するだけなら`juiz_sdk`で済む。
//!

mod core;
mod plugin;

mod brokers;
mod connections;
mod containers;
mod ecs;
mod processes;
mod topics;

pub mod types;

pub mod prelude;

// pub use crate::utils::yaml_conf_load;
pub use brokers::{
    create_broker_factory_impl, create_broker_proxy_factory_impl, CRUDBroker, CRUDBrokerHolder,
};
pub use brokers::builtin_broker_factories::{
    register_builtin_broker_factory, BrokerFactoryFunction, BrokerProxyFactoryFunction,
};
pub use brokers::{localize_identifier, CRUDBrokerProxy, CRUDBrokerProxyHolder};
/// 生バイトのフレーム形式 (docs/spec/large-data.md §3)。qmp ブローカーも使う
pub use brokers::frame;
/// IPC ブローカー (docs/spec/ipc-broker.md、feature "ipc-broker")
#[cfg(feature = "ipc-broker")]
pub use brokers::ipc;
pub use core::{SubsystemCheckFunctionType, SystemStore, SystemStorePtr};
// conf の plugins の path の {target} 置換とその置き換え先 (docs/spec/app-package.md §5、#284)
pub use plugin::{replace_target_in_plugin_paths, HOST_TARGET};
// juiz run が使う、埋め込みの Python の確認 (docs/spec/app-package.md §9、#285)
pub use plugin::{python_check, PythonCheck, PYTHON_ENABLED};
pub use ecs::{
    execution_context_core::ExecutionContextState, ExecutionContext, ExecutionContextCore,
    ExecutionContextFactory,
};

// Re export

// #[cfg(feature="opencv4")]
// pub use opencv;

pub use futures;
pub use juiz_sdk::anyhow;
pub use juiz_sdk::env_logger;
pub use juiz_sdk::log;
pub use juiz_sdk::utils;
pub use tokio;
