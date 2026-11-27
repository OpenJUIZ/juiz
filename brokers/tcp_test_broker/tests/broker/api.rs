//! C ABI のブローカー経由で別の System の Process / Container を操作するテスト (同じプロセス内・別プロセス、画像と配列のフレーム)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。qmp・Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn broker_api_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::broker_api_in_same_process(&TCPTEST)
}

/// juiz コマンドに C ABI のプラグイン (libtcp_test_broker) を読ませてサーバーを起動する
#[test]
fn broker_api_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::broker_api_between_processes(&TCPTEST)
}

#[test]
fn sends_images_and_arrays_as_frames() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::sends_images_and_arrays(&TCPTEST)
}
