//! QUIC (qmp) ブローカー経由で別の System の Process / Container を操作するテスト
//!
//! - 同じプロセス内: System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、
//!   QUIC クライアント → QUIC サーバー → CRUDBroker → CoreBroker の経路を通す。
//! - 別プロセス: juiz コマンドで qmp ブローカーを持つサーバーを起動し、
//!   テスト側の System から qmp の BrokerProxy で Operation を呼ぶ。
//!
//! プラグイン (qmp_broker, increment_process, example_container, timer_ec) と juiz コマンドは
//! cargo build で ../../target/debug にビルドされている前提 (CI の test.yml と同じ)。

mod common;

use std::time::{Duration, Instant};

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

const PORT: i64 = 5012;
const SERVER_PORT: i64 = 5013;

#[test]
fn qmp_broker_api_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    System::new(server_manifest("qmp_broker_api_test", PORT, 8032, true))?
        .setup()?
        .run_and_do_once(|system| {
            // qmp ブローカーは起動後、実際の住所を名前にする
            let brokers = system.core_broker().lock()?.broker_list(false)?;
            assert!(
                brokers
                    .iter()
                    .any(|id| id.ends_with(&format!("/broker/127.0.0.1:{PORT}::qmp"))),
                "{brokers:?}"
            );

            let proxy = system
                .core_broker()
                .lock()?
                .worker()
                .broker_proxy("qmp", broker_name.as_str(), true)?;
            let mut bp = juiz_lock(&proxy)?;
            check_processes(&mut *bp, broker_name.as_str())?;
            check_containers(&mut *bp)?;
            check_error_keeps_connection(&mut *bp, broker_name.as_str())?;
            check_connections(&mut *bp)?;
            check_create_destroy(&mut *bp)?;
            check_ec(&mut *bp)?;
            Ok(())
        })
}

#[test]
fn qmp_broker_api_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let mut server = spawn_server(SERVER_PORT, 8033, true);
    let broker_name = format!("127.0.0.1:{SERVER_PORT}");
    System::new(client_manifest("qmp_broker_test_client", true))?
        .setup()?
        .run_and_do_once(|system| {
            // サーバーの起動を待つ。起動前は接続がタイムアウトしてエラーになる
            let deadline = Instant::now() + Duration::from_secs(60);
            let proxy = loop {
                if let Some(status) = server.0.try_wait()? {
                    panic!("juiz server exited ({status})");
                }
                match system
                    .core_broker()
                    .lock()?
                    .worker()
                    .broker_proxy("qmp", broker_name.as_str(), true)
                {
                    Ok(p) => break p,
                    Err(e) => {
                        assert!(Instant::now() < deadline, "can not connect to server: {e:?}");
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            };
            let mut bp = juiz_lock(&proxy)?;
            check_processes(&mut *bp, broker_name.as_str())?;
            check_containers(&mut *bp)?;
            check_error_keeps_connection(&mut *bp, broker_name.as_str())?;
            check_connections(&mut *bp)?;
            Ok(())
        })
}
