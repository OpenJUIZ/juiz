//! juiz_app と同じく qmp ブローカーを組み込み (qmp_broker::register) で使うテスト (issue #63)
//!
//! - 同じプロセス内: プラグインを読み込まない設定でも qmp のブローカー / ブローカープロキシが使える。
//! - 別プロセス: プラグインを読み込まない設定で juiz コマンド (qmp を組み込んだ juiz_app) を起動し、
//!   組み込みの qmp の BrokerProxy で Operation を呼ぶ。
//! - プラグインも読み込む設定: 二重登録のエラーにならず、プラグインのほうが使われる。
//!
//! qmp_broker_api_test.rs と別のテストバイナリにして、組み込みの登録がそちらに影響しないようにしている。

mod common;

use std::time::{Duration, Instant};

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

const PORT: i64 = 5014;
const SERVER_PORT: i64 = 5015;
const PLUGIN_PORT: i64 = 5016;

fn setup() {
    let _ = env_logger::try_init();
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(qmp_broker::register);
}

#[test]
fn qmp_builtin_in_same_process() -> JuizResult<()> {
    setup();
    let broker_name = format!("localhost:{PORT}");
    System::new(server_manifest("qmp_builtin_test", PORT, 8034, false))?
        .setup()?
        .run_and_do_once(|system| {
            let brokers = system.core_broker().lock()?.broker_list(false)?;
            assert!(
                brokers
                    .iter()
                    .any(|id| id.ends_with(&format!("/broker/127.0.0.1:{PORT}::qmp"))),
                "{brokers:?}"
            );

            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "qmp",
                broker_name.as_str(),
                true,
            )?;
            let mut bp = juiz_lock(&proxy)?;
            check_processes(&mut *bp, broker_name.as_str())?;
            check_containers(&mut *bp)?;
            check_connections(&mut *bp)?;
            Ok(())
        })
}

#[test]
fn qmp_builtin_between_processes() -> JuizResult<()> {
    setup();
    let mut server = spawn_server(SERVER_PORT, 8035, false);
    let broker_name = format!("127.0.0.1:{SERVER_PORT}");
    System::new(client_manifest("qmp_builtin_test_client", false))?
        .setup()?
        .run_and_do_once(|system| {
            let deadline = Instant::now() + Duration::from_secs(60);
            let proxy = loop {
                if let Some(status) = server.0.try_wait()? {
                    panic!("juiz server exited ({status})");
                }
                match system.core_broker().lock()?.worker().broker_proxy(
                    "qmp",
                    broker_name.as_str(),
                    true,
                ) {
                    Ok(p) => break p,
                    Err(e) => {
                        assert!(
                            Instant::now() < deadline,
                            "can not connect to server: {e:?}"
                        );
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            };
            let mut bp = juiz_lock(&proxy)?;
            check_processes(&mut *bp, broker_name.as_str())?;
            check_containers(&mut *bp)?;
            Ok(())
        })
}

/// 組み込みとプラグインの両方があるときは、プラグインのほうを使う
#[test]
fn qmp_builtin_with_plugin() -> JuizResult<()> {
    setup();
    let broker_name = format!("localhost:{PLUGIN_PORT}");
    System::new(server_manifest(
        "qmp_builtin_plugin_test",
        PLUGIN_PORT,
        8036,
        true,
    ))?
    .setup()?
    .run_and_do_once(|system| {
        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "qmp",
            broker_name.as_str(),
            true,
        )?;
        let mut bp = juiz_lock(&proxy)?;
        check_processes(&mut *bp, broker_name.as_str())?;
        Ok(())
    })
}
