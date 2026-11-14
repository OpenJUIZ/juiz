//! zenoh ブローカーを組み込み (zenoh_broker::register) で使うテスト (docs/spec/zenoh-broker.md §8)
//!
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

const PORT: i64 = 5214;
const PLUGIN_PORT: i64 = 5216;

fn setup() {
    let _ = env_logger::try_init();
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(zenoh_broker::register);
}

#[test]
fn zenoh_builtin_in_same_process() -> JuizResult<()> {
    setup();
    let broker_name = format!("localhost:{PORT}");
    System::new(server_manifest("zenoh_builtin_test", PORT, 8234, false))?
        .setup()?
        .run_and_do_once(|system| {
            let brokers = system.core_broker().lock()?.broker_list(false)?;
            assert!(
                brokers
                    .iter()
                    .any(|id| id.ends_with(&format!("/broker/127.0.0.1:{PORT}::zenoh"))),
                "{brokers:?}"
            );

            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "zenoh",
                broker_name.as_str(),
                true,
            )?;
            let mut bp = juiz_write(&proxy)?;
            check_processes(&mut *bp, broker_name.as_str())?;
            check_containers(&mut *bp)?;
            check_connections(&mut *bp)?;
            Ok(())
        })
}

/// 組み込みとプラグインの両方があるときは、プラグインのほうを使う
#[test]
fn zenoh_builtin_with_plugin() -> JuizResult<()> {
    setup();
    let broker_name = format!("localhost:{PLUGIN_PORT}");
    System::new(server_manifest(
        "zenoh_builtin_plugin_test",
        PLUGIN_PORT,
        8236,
        true,
    ))?
    .setup()?
    .run_and_do_once(|system| {
        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "zenoh",
            broker_name.as_str(),
            true,
        )?;
        let mut bp = juiz_write(&proxy)?;
        check_processes(&mut *bp, broker_name.as_str())?;
        Ok(())
    })
}
