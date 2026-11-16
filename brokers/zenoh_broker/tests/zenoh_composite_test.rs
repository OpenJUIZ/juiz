//! zenoh ブローカー経由で Composite の中身が読めること (issue #170)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::composite_profile、issue #522)。qmp も同じものを走らせる。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn composite_profile_shows_composition() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::composite_profile(&ZENOH)
}
