//! zenoh ブローカーのテストで共通に使うもの
//!
//! 確認の中身はブローカー共通のスイート juiz_broker_suite にある (issue #522)。
//! ここでは zenoh を [`ZENOH`] として渡す。ポートは qmp のテストの番号に 200 を足したもの。
#![allow(dead_code, unused_imports)]

pub use juiz_broker_suite::data::{image_of_format, setup_data_processes};
pub use juiz_broker_suite::fixture::target_dir;
pub use juiz_broker_suite::remote::{self, RemoteBroker, ServerProcess};
pub use juiz_broker_suite::support::{args, int_of};

pub const ZENOH: RemoteBroker = RemoteBroker {
    type_name: "zenoh",
    plugin_name: "zenoh_broker",
    port_offset: 200,
    register: zenoh_broker::register,
    new_crud_proxy: |manifest| Ok(Box::new(zenoh_broker::ZenohBrokerProxy::new(manifest)?)),
};
