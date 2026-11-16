//! zenoh ブローカー経由で Container の 3D モデルを取り出せること (issue #381、docs/spec/container-model.md §6.3)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote::container_model、issue #522)。qmp も同じものを走らせる。
//! HTTP とローカルブローカーは juiz_core の container_model::core、HTTP と比べたベンチは qmp の
//! juiz_qmp_broker の bench::container_model にある。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn container_model_can_be_read() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::container_model(&ZENOH)
}
