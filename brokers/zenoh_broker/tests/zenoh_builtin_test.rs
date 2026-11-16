//! zenoh ブローカーを組み込み (zenoh_broker::register) で使うテスト (docs/spec/zenoh-broker.md §8)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。
//! - 同じプロセス内: プラグインを読み込まない設定でも zenoh のブローカー / ブローカープロキシが使える。
//! - プラグインも読み込む設定: 二重登録のエラーにならず、プラグインのほうが使われる。
//!
//! juiz コマンドに zenoh を組み込むのは Step 2 (#491) なので、別プロセスの確かめはここでは
//! しない (zenoh_broker_api_test.rs がプラグインで確かめる)。
//!
//! zenoh_broker_api_test.rs と別のテストバイナリにして、組み込みの登録がそちらに影響しないようにしている。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn builtin_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_in_same_process(&ZENOH)
}

#[test]
fn builtin_with_plugin_uses_plugin() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_with_plugin(&ZENOH)
}
