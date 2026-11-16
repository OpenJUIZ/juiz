//! zenoh ブローカー経由で別の System の Process / Container を操作するテスト
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。qmp と同じ確かめ (docs/spec/zenoh-broker.md §10)。
//! - 同じプロセス内: System 自身の zenoh ブローカーに zenoh の BrokerProxy で繋ぎ、
//!   Zenoh のクライアント → Zenoh のサーバー → CRUDBroker → CoreBroker の経路を通す。
//! - 別プロセス: juiz コマンドに zenoh のプラグインを読ませてサーバーを起動し、
//!   テスト側の System から zenoh の BrokerProxy で Operation を呼ぶ。
//!   プラグインは `cargo test -p juiz_zenoh_broker` が juiz_core の feature なしで作るので、
//!   既定の feature の juiz に feature なしのプラグインを読ませる確かめ (#450) も兼ねる。
//! - 画像・数の配列・Topic の push を生バイトのフレームで送る (#234)。
//!
//! プラグイン (zenoh_broker, increment_process, example_container, timer_ec) と juiz コマンドは
//! cargo build で ../../target/debug にビルドされている前提 (CI の test-*.yml と同じ)。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn broker_api_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::broker_api_in_same_process(&ZENOH)
}

#[test]
fn broker_api_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::broker_api_between_processes(&ZENOH)
}

#[test]
fn sends_images_and_arrays_as_frames() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::sends_images_and_arrays(&ZENOH)
}
