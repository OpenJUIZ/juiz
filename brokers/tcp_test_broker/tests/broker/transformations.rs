//! C ABI のブローカー経由で Transformations の API を呼べること
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。qmp・Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn transformations_api_can_be_called() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::transformations_api(&TCPTEST)
}
