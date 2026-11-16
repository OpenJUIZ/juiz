//! qmp ブローカー経由で Transformations の API を呼べること (issue #370)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::transformations_api、issue #522)。Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn transformations_api_can_be_called() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::transformations_api(&QMP)
}
