//! qmp ブローカー経由で実行中に Composite を定義できること (issue #171)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::composite_api、issue #522)。Zenoh も同じものを走らせる。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn composite_can_be_defined_while_running() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::composite_api(&QMP)
}
