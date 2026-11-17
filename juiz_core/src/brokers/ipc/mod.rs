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

/// その IPC 名で待ち受けている相手がいるか (docs/spec/app-package.md §15.2 の基礎の二重起動の確認)。
/// つながれば使用中とみなす。前のサーバーが残したソケットのファイルだけでは使用中としない
pub fn ipc_name_in_use(name: &str) -> bool {
    transport::connect(name).is_ok()
}
