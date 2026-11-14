//! zenoh ブローカー経由で別の System の Process / Container を操作するテスト
//! (qmp の qmp_broker_api_test.rs と同じ確かめ、docs/spec/zenoh-broker.md §10)
//!
//! - 同じプロセス内: System 自身の zenoh ブローカーに zenoh の BrokerProxy で繋ぎ、
//!   Zenoh のクライアント → Zenoh のサーバー → CRUDBroker → CoreBroker の経路を通す。
//! - 別プロセス: juiz コマンドに zenoh のプラグインを読ませてサーバーを起動し、
//!   テスト側の System から zenoh の BrokerProxy で Operation を呼ぶ。
//!   プラグインは `cargo test -p juiz_zenoh_broker` が juiz_core の feature なしで作るので、
//!   既定の feature の juiz に feature なしのプラグインを読ませる確かめ (#450) も兼ねる。
//!
//! juiz コマンドと、プラグイン (increment_process, example_container, timer_ec) は
//! `cargo build` で ../../target/debug にビルドされている前提。

mod common;

use std::time::{Duration, Instant};

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;

const PORT: i64 = 5212;
const SERVER_PORT: i64 = 5213;

#[test]
fn zenoh_broker_api_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    System::new(server_manifest("zenoh_broker_api_test", PORT, 8232, true))?
        .setup()?
        .run_and_do_once(|system| {
            // zenoh ブローカーは起動後、実際の住所を名前にする
            let brokers = system.core_broker().lock()?.broker_list(false)?;
            assert!(
                brokers
                    .iter()
                    .any(|id| id.ends_with(&format!("/broker/127.0.0.1:{PORT}::zenoh"))),
                "{brokers:?}"
            );

            let proxy = system
                .core_broker()
                .lock()?
                .worker()
                .broker_proxy("zenoh", broker_name.as_str(), true)?;
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
fn zenoh_broker_api_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let mut server = spawn_server(SERVER_PORT, 8233, true);
    let broker_name = format!("127.0.0.1:{SERVER_PORT}");
    System::new(client_manifest("zenoh_broker_test_client", true))?
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
                    .broker_proxy("zenoh", broker_name.as_str(), true)
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

const DATA_PORT: i64 = 5223;

/// 画像・数の配列・Topic の push を、生バイトのフレーム (docs/spec/large-data.md §3) で送る。
/// Rust の関数で Process を登録するので同じプロセス内だけ。それでも Zenoh のクライアントとサーバーの間は
/// フレームのバイト列で受け渡す
#[test]
fn zenoh_broker_sends_images_and_arrays() -> JuizResult<()> {
    let _ = env_logger::try_init();
    zenoh_broker::register();
    let broker_name = format!("127.0.0.1:{DATA_PORT}");
    System::new(jvalue!({
        "name": "zenoh_broker_data_test",
        "option": { "http_broker": { "start": false } },
        "brokers": [
            { "type_name": "zenoh", "name": broker_name, "host": "127.0.0.1", "port": DATA_PORT }
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
                TopicManifest::new("zenoh_image_topic"),
            )?;
            worker.process_publish_topic(publisher.clone(), TopicManifest::new("zenoh_image_topic"))?;
        }
        let proxy = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("zenoh", broker_name.as_str(), true)?;
        let mut bp = juiz_write(&proxy)?;
        check_remote_error(&mut *bp)?;
        check_images(&mut *bp)?;
        check_arrays(&mut *bp)?;
        check_topic_push_image(&mut *bp, "zenoh_image_topic", &sink)?;
        // エラーの後も同じ接続で呼べる
        check_remote_error(&mut *bp)?;
        Ok(())
    })
}
