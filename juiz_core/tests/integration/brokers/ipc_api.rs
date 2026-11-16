//! IPC ブローカー経由で別の System の Process / Container を操作するテスト (issue #233、docs/spec/ipc-broker.md §8)
//!
//! - 同じプロセス内: System 自身の IPC ブローカーに IPC の BrokerProxy で繋ぎ、
//!   Unix ソケット (Windows は名前付きパイプ) → IpcBroker → CRUDBroker → CoreBroker の経路を通す。
//!   API と、画像 (形式ごと) と数の配列の引数と戻り値、Topic の push で画像を送ること、エラー応答は
//!   ブローカー共通のスイート (juiz_broker_suite の api と data、issue #522) で確かめる。
//! - IPC だけの確認: サーバーを作り直した後の再接続、残ったソケットの片付け、名前の検証。
//! - 別プロセス: juiz -d --ipc で IPC ブローカーを持つサーバーを起動し、テスト側から呼ぶ。
//!
//! プラグイン (increment_process, example_container, timer_ec) と juiz コマンドは
//! cargo build で ../target/debug にビルドされている前提 (CI の test.yml と同じ)。

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use juiz_broker_suite::data;
use juiz_broker_suite::fixture::{rust_system_manifest, target_dir, RUST_ONLY};
use juiz_core::prelude::*;

use crate::brokers::broker_api::{args, find_process, int_of, suite, Expect};

/// IPC ブローカーを持ち、Process / Container / EC (juiz_broker_suite::fixture::RUST_ONLY) を読み込んだ System の設定
pub(crate) fn server_manifest(system_name: &str, ipc_name: &str) -> Value {
    rust_system_manifest(
        system_name,
        8000,
        jvalue!([ { "type_name": "ipc", "name": ipc_name } ]),
        jvalue!({}),
    )
}

/// 画像と配列を扱う Process (make0, echo0, vecho0, fail0, sum0, pub0) を作る
pub(crate) fn setup_test_processes(system: &System) -> JuizResult<(ProcessPtr, ProcessPtr)> {
    data::setup_data_processes(system)
}

fn ipc_proxy(system: &System, name: &str) -> JuizResult<std::sync::Arc<std::sync::RwLock<dyn BrokerProxy>>> {
    system.core_broker().lock()?.worker().broker_proxy("ipc", name, true)
}

#[test]
fn ipc_broker_api_in_same_process() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "ipc_api_test";
    let topic = "ipc_image_topic";
    System::new(server_manifest("ipc_broker_api_test", ipc_name))?
        .setup()?
        .run_and_do_once(|system| {
            let (_publisher, sink) = data::setup_data_processes_with_topic(system, topic)?;
            let proxy = ipc_proxy(system, ipc_name)?;
            let mut bp = juiz_write(&proxy)?;
            let brokers = bp.broker_list(false)?;
            assert!(
                brokers.iter().any(|id| id.ends_with(&format!("/broker/{ipc_name}::ipc"))),
                "{brokers:?}"
            );
            suite::check_all(
                &mut *bp,
                &RUST_ONLY,
                &Expect {
                    broker_type_name: "ipc",
                    broker_name: Some(ipc_name),
                },
            )?;
            data::check_remote_error(&mut *bp)?;
            data::check_images(&mut *bp)?;
            data::check_arrays(&mut *bp)?;
            data::check_topic_push_image(&mut *bp, topic, &sink)?;
            Ok(())
        })
}

/// 同じ名前のサーバーが動いていれば起動はエラー。止めたサーバーのソケットは消え、
/// 作り直したサーバーには同じ BrokerProxy のまま繋ぎ直せる。異常終了で残ったソケットは片付けて起動する
#[test]
fn ipc_broker_restart_and_stale_socket() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "ipc_restart_test";
    let manifest = || {
        jvalue!({
            "name": "ipc_restart_test",
            "option": { "http_broker": { "start": false } },
            "brokers": [ { "type_name": "ipc", "name": ipc_name } ]
        })
    };
    #[cfg(unix)]
    let socket = juiz_core::ipc::ipc_name::ipc_socket_path(ipc_name)?;
    #[cfg(unix)]
    {
        // 前のサーバーが異常終了して残したソケット (繋がらない) を置く
        let _ = std::fs::remove_file(&socket);
        drop(std::os::unix::net::UnixListener::bind(&socket)?);
        assert!(socket.exists());
    }

    let client_manifest = jvalue!({
        "name": "ipc_restart_client",
        "option": { "http_broker": { "start": false } }
    });
    System::new(client_manifest)?.setup()?.run_and_do_once(|client| {
        let proxy = ipc_proxy(client, ipc_name)?;
        // サーバーが居なければエラーになる
        assert!(juiz_borrow(&proxy)?.system_uuid().is_err());

        let mut uuid1 = Value::Null;
        System::new(manifest())?.setup()?.run_and_do_once(|_first| {
            uuid1 = juiz_borrow(&proxy)?.system_uuid()?;
            // 同じ名前ではもう 1 つ起動できない
            let e = System::new(manifest())?.setup().err().expect("must fail");
            assert!(format!("{e:#}").contains("already in use"), "{e:#}");
            Ok(())
        })?;
        // 止めたサーバーのソケットは消える
        #[cfg(unix)]
        assert!(!socket.exists(), "the socket must be removed when the broker stops");
        assert!(juiz_borrow(&proxy)?.system_uuid().is_err());

        // 作り直したサーバーに、同じ BrokerProxy のまま繋ぎ直せる
        System::new(manifest())?.setup()?.run_and_do_once(|_second| {
            let uuid2 = juiz_borrow(&proxy)?.system_uuid()?;
            assert_ne!(uuid1, uuid2);
            Ok(())
        })
    })
}

#[test]
fn ipc_broker_rejects_invalid_names() -> JuizResult<()> {
    crate::common::logger::init();
    for (key, name) in [("name", "bad/name"), ("name", ".hidden"), ("namespace", "x")] {
        let mut broker = jvalue!({ "type_name": "ipc" });
        broker[key] = jvalue!(name);
        let r = System::new(jvalue!({
            "name": "ipc_invalid_name_test",
            "option": { "http_broker": { "start": false } },
            "brokers": [ broker ]
        }))?
        .setup();
        assert!(r.is_err(), "{key}={name} must be rejected");
    }
    Ok(())
}

/// juiz -d --ipc <name> で起動したサーバー。drop で止める
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn ipc_broker_api_between_processes() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "ipc_cli_test";
    let conf = std::env::temp_dir().join(format!("juiz_ipc_cli_test_{}.conf", std::process::id()));
    // YAML の二重引用符では Windows のパスの \ がエスケープとして読まれるので、一重引用符で書く
    let path = target_dir().to_string_lossy().replace('\'', "''");
    std::fs::write(
        &conf,
        format!(
            "name: ipc_cli_test_server\nplugins:\n  process_factories:\n    increment_process:\n      path: '{path}'\nprocesses:\n  - type_name: increment_process\n    name: inc0\n"
        ),
    )?;
    let juiz = target_dir().join(if cfg!(windows) { "juiz.exe" } else { "juiz" });
    let mut server = Server(
        Command::new(juiz)
            .args(["-d", "--no-http", "--ipc", ipc_name, "-f"])
            .arg(&conf)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()?,
    );
    let result = System::new(jvalue!({
        "name": "ipc_cli_test_client",
        "option": { "http_broker": { "start": false } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let proxy = ipc_proxy(system, ipc_name)?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let ids = loop {
            if let Some(status) = server.0.try_wait()? {
                panic!("juiz server exited ({status})");
            }
            match juiz_borrow(&proxy)?.process_list(false, None) {
                Ok(ids) if !ids.is_empty() => break ids,
                r => {
                    assert!(Instant::now() < deadline, "can not connect to server: {r:?}");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        };
        let inc0 = find_process(&ids, "inc0");
        assert_eq!(inc0.broker_type_name, "ipc");
        assert_eq!(int_of(juiz_borrow(&proxy)?.process_call(inc0, args("arg1", 41))), 42);
        Ok(())
    });
    drop(server);
    let _ = std::fs::remove_file(&conf);
    result
}
