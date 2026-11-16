//! ネームスペース付きのオブジェクトを qmp ブローカー越しに扱うテスト (issue #150)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::namespaced_objects_over_subsystem、issue #522)。Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn namespaced_objects_over_subsystem() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::namespaced_objects_over_subsystem(&QMP)
}
