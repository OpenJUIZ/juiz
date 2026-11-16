//! QUIC (qmp) ブローカー経由で別の System の Process / Container を操作するテスト
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。
//! - 同じプロセス内: System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、
//!   QUIC クライアント → QUIC サーバー → CRUDBroker → CoreBroker の経路を通す。
//! - 別プロセス: juiz コマンドで qmp ブローカーを持つサーバーを起動し、
//!   テスト側の System から qmp の BrokerProxy で Operation を呼ぶ。
//! - 画像・数の配列・Topic の push を生バイトのフレームで送る (#234)。
//!
//! プラグイン (qmp_broker, increment_process, example_container, timer_ec) と juiz コマンドは
//! cargo build で ../../target/debug にビルドされている前提 (CI の test-*.yml と同じ)。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn broker_api_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::broker_api_in_same_process(&QMP)
}

#[test]
fn broker_api_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::broker_api_between_processes(&QMP)
}

#[test]
fn sends_images_and_arrays_as_frames() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::sends_images_and_arrays(&QMP)
}
