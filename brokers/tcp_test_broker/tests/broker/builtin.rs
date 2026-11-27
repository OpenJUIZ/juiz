//! C ABI のブローカーを組み込み (register_builtin_cabi_broker) で使うテスト (§6.4 の 2)
//!
//! 中身はブローカー共通のスイート (juiz_broker_suite::remote、issue #522)。qmp・Zenoh も同じものを走らせる。

use crate::common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

#[test]
fn builtin_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_in_same_process(&TCPTEST)
}

/// 組み込みとプラグインの両方があるときは、プラグインのほうを使う
#[test]
fn builtin_with_plugin_uses_plugin() -> JuizResult<()> {
    let _ = env_logger::try_init();
    remote::builtin_with_plugin(&TCPTEST)
}
