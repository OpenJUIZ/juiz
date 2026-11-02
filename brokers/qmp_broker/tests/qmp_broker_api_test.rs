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
            let mut bp = juiz_write(&proxy)?;
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
            let mut bp = juiz_write(&proxy)?;
            check_processes(&mut *bp, broker_name.as_str())?;
            check_containers(&mut *bp)?;
            check_error_keeps_connection(&mut *bp, broker_name.as_str())?;
            check_connections(&mut *bp)?;
            Ok(())
        })
}

const DATA_PORT: i64 = 5023;

/// 画像・数の配列・Topic の push を、生バイトのフレーム (docs/spec/large-data.md §3) で送る (#234)。
/// Rust の関数で Process を登録するので同じプロセス内だけ。それでも QUIC のクライアントとサーバーの間は
/// フレームのバイト列で受け渡す
#[test]
fn qmp_broker_sends_images_and_arrays() -> JuizResult<()> {
    let _ = env_logger::try_init();
    qmp_broker::register();
    let broker_name = format!("127.0.0.1:{DATA_PORT}");
    System::new(jvalue!({
        "name": "qmp_broker_data_test",
        "option": { "http_broker": { "start": false } },
        "brokers": [
            { "type_name": "qmp", "name": broker_name, "host": "127.0.0.1", "port": DATA_PORT }
        ]
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let (publisher, sink) = setup_data_processes(system)?;
        {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            worker.process_subscribe_topic(
                sink.clone(),
                &"img".to_owned(),
                TopicManifest::new("qmp_image_topic"),
            )?;
            worker.process_publish_topic(publisher.clone(), TopicManifest::new("qmp_image_topic"))?;
        }
        let proxy = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("qmp", broker_name.as_str(), true)?;
        let mut bp = juiz_write(&proxy)?;
        check_remote_error(&mut *bp)?;
        check_images(&mut *bp)?;
        check_arrays(&mut *bp)?;
        check_topic_push_image(&mut *bp, "qmp_image_topic", &sink)?;
        // エラーの後も同じ接続で呼べる
        check_remote_error(&mut *bp)?;
        Ok(())
    })
}
