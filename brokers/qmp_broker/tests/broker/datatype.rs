//! qmp ブローカー経由で型の登録所を引けること (issue #193)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::type_api、issue #522)。Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn type_registry_can_be_read() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::type_api(&QMP)
}
