//! zenoh ブローカー経由で Composite の中身が読めること (issue #170) と、実行中に Composite を定義できること (issue #171)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::composite_profile と composite_api、issue #522)。qmp も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn composite_profile_shows_composition() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::composite_profile(&ZENOH)
}

#[test]
fn composite_can_be_defined_while_running() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::composite_api(&ZENOH)
}
