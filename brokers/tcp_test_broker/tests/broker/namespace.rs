//! C ABI のブローカーのサブシステム越しにネームスペース付きのオブジェクトを扱えること
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。qmp・Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn namespaced_objects_over_subsystem() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::namespaced_objects_over_subsystem(&TCPTEST)
}
