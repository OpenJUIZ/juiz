//! IPC ブローカー: 同じマシンの別プロセスの juiz を、ポートを開けずに速く呼び出す (docs/spec/ipc-broker.md)
//!
//! feature "ipc-broker" のときだけ組み込む。

pub mod ipc_broker;
pub mod ipc_broker_proxy;
pub mod ipc_name;
pub mod shm;
mod shm_delivery;
mod transport;

pub use ipc_broker::{ipc_broker_factory, IpcBroker, IPC_TYPE_NAME};
pub use ipc_broker_proxy::{ipc_broker_proxy_factory, IpcBrokerProxy};
pub use ipc_name::validate_ipc_name;
