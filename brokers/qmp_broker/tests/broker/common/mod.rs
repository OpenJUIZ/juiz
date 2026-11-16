//! qmp ブローカーのテストで共通に使うもの
//!
//! 確認の中身はブローカー共通のスイート juiz_broker_suite にある (issue #522)。
//! ここでは qmp を [`QMP`] として渡す。
#![allow(dead_code, unused_imports)]

pub use juiz_broker_suite::fixture::target_dir;
pub use juiz_broker_suite::remote::{self, RemoteBroker};

pub const QMP: RemoteBroker = RemoteBroker {
    type_name: "qmp",
    plugin_name: "qmp_broker",
    port_offset: 0,
    register: qmp_broker::register,
    new_crud_proxy: |manifest| Ok(Box::new(qmp_broker::QuicBrokerProxy::new(manifest)?)),
};
