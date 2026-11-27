//! zenoh ブローカーのテストで共通に使うもの
//!
//! 確認の中身はブローカー共通のスイート juiz_broker_suite にある (issue #522)。
//! ここでは zenoh を [`ZENOH`] として渡す。ポートは qmp のテストの番号に 200 を足したもの。
#![allow(dead_code, unused_imports)]

pub use juiz_broker_suite::data::{image_of_format, setup_data_processes};
pub use juiz_broker_suite::fixture::target_dir;
pub use juiz_broker_suite::remote::{self, RemoteBroker, ServerProcess};
pub use juiz_broker_suite::support::{args, int_of};

use juiz_core::cabi::CabiBrokerProxy;
use juiz_core::prelude::*;

/// 組み込みにする (rlib としてリンクした入口を登録する、docs/spec/broker-plugin-abi.md §6.4 の 2)。
/// 何度呼んでも 1 度だけ登録する
pub fn register() {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| juiz_core::register_builtin_cabi_broker(zenoh_broker::broker_entry));
}

/// zenoh のサーバーに繋ぐ CRUD のクライアント (System を通さずに呼ぶとき)
pub fn proxy(manifest: &Value) -> JuizResult<CabiBrokerProxy> {
    let mut m = manifest.clone();
    m["type_name"] = jvalue!("zenoh");
    CabiBrokerProxy::from_entry_fn(zenoh_broker::broker_entry, &m)
}

pub const ZENOH: RemoteBroker = RemoteBroker {
    type_name: "zenoh",
    plugin_name: "zenoh_broker",
    port_offset: 200,
    register,
    new_crud_proxy: |manifest| Ok(Box::new(proxy(manifest)?)),
};
