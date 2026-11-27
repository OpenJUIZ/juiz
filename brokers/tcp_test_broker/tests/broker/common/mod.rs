//! 試験用の TCP ブローカーのテストで共通に使うもの
//!
//! 確認の中身はブローカー共通のスイート juiz_broker_suite にある (issue #522)。
//! ここでは tcptest を [`TCPTEST`] として渡す。ポートは qmp のテストの番号に 400 を足したもの。
#![allow(dead_code, unused_imports)]

pub use juiz_broker_suite::fixture::target_dir;
pub use juiz_broker_suite::remote::{self, RemoteBroker, ServerProcess};
pub use juiz_broker_suite::support::{args, int_of};

/// 組み込みにする (rlib としてリンクした入口を登録する、docs/spec/broker-plugin-abi.md §6.4 の 2)
fn register() {
    juiz_core::register_builtin_cabi_broker(tcp_test_broker::broker_factory_cabi);
}

pub const TCPTEST: RemoteBroker = RemoteBroker {
    type_name: "tcptest",
    plugin_name: "tcp_test_broker",
    port_offset: 400,
    register,
    new_crud_proxy: |manifest| {
        Ok(Box::new(juiz_core::cabi::CabiBrokerProxy::from_entry_fn(
            tcp_test_broker::broker_factory_cabi,
            manifest,
        )?))
    },
};
