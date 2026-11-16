//! ブローカー(core・local・HTTP・IPC・メッセンジャー系)越しの統合テスト

pub(crate) mod broker_api;
mod core_api;
mod core_broker;
mod core_broker_factory;
mod http_api;
mod http_concurrency;
mod http_image;
mod http_router;
#[cfg(feature = "ipc-broker")]
pub(crate) mod ipc_api;
#[cfg(feature = "ipc-broker")]
mod ipc_shared_memory;
mod local_api;
mod messenger_proxy;
mod namespace;
mod profile_alias;
mod viewer_api;
mod webui;
