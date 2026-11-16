//! juiz_app と同じく qmp ブローカーを組み込み (qmp_broker::register) で使うテスト (issue #63)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。
//! - 同じプロセス内: プラグインを読み込まない設定でも qmp のブローカー / ブローカープロキシが使える。
//! - 別プロセス: プラグインを読み込まない設定で juiz コマンド (qmp を組み込んだ juiz_app) を起動し、
//!   組み込みの qmp の BrokerProxy で Operation を呼ぶ。
//! - プラグインも読み込む設定: 二重登録のエラーにならず、プラグインのほうが使われる。
//!
//! 組み込みの登録はプロセス全体に効き、api などプラグインを読むテストと同じバイナリで走る (issue #523)。
//! 両方あるときはプラグインのほうが使われる (builtin_with_plugin_uses_plugin) ので、そちらの確かめは変わらない。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn builtin_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_in_same_process(&QMP)
}

#[test]
fn builtin_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_between_processes(&QMP)
}

#[test]
fn builtin_with_plugin_uses_plugin() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_with_plugin(&QMP)
}
