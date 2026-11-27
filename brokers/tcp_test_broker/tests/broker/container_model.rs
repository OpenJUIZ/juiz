//! C ABI のブローカー経由で Container の 3D モデルを取り出せること
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。qmp・Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn container_model_can_be_read() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::container_model(&TCPTEST)
}
