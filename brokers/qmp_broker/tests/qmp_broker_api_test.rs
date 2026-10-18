//! QUIC (qmp) ブローカー経由で別の System の Process / Container を操作するテスト
//!
//! - 同じプロセス内: System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、
//!   QUIC クライアント → QUIC サーバー → CRUDBroker → CoreBroker の経路を通す。
//! - 別プロセス: juiz コマンドで qmp ブローカーを持つサーバーを起動し、
//!   テスト側の System から qmp の BrokerProxy で Operation を呼ぶ。
//!
//! プラグイン (qmp_broker, increment_process, example_container, timer_ec) と juiz コマンドは
//! cargo build で ../../target/debug にビルドされている前提 (CI の test.yml と同じ)。

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_core::env_logger;
use juiz_sdk::process_identifier::ProcessIdentifier;

const PORT: i64 = 5012;
const SERVER_PORT: i64 = 5013;

fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug")
}

fn plugin_path() -> String {
    target_dir().to_string_lossy().into_owned()
}

/// qmp ブローカーを持ち、Process / Container / EC を読み込んだ System の設定
fn server_manifest(name: &str, port: i64, http_port: i64) -> Value {
    let path = plugin_path();
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false, "port": http_port }
        },
        "plugins": {
            "broker_factories": {
                "qmp_broker": { "path": path }
            },
            "process_factories": {
                "increment_process": { "path": path }
            },
            "container_factories": {
                "example_container": {
                    "path": path,
                    "processes": {
                        "example_container_increment": { "path": path },
                        "example_container_get": { "path": path }
                    }
                }
            },
            "ec_factories": {
                "timer_ec": { "path": path }
            }
        },
        "brokers": [
            { "type_name": "qmp", "name": format!("127.0.0.1:{port}"), "host": "127.0.0.1", "port": port }
        ],
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "increment_process", "name": "inc1" }
        ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [
                  { "type_name": "example_container_increment", "name": "c0inc" },
                  { "type_name": "example_container_get", "name": "c0get" } ] }
        ],
        "ecs": [
            { "type_name": "TimerEC", "name": "timer0", "rate": 100.0 }
        ]
    })
}

/// qmp の BrokerProxy だけを使う (ブローカーを持たない) System の設定
fn client_manifest(name: &str) -> Value {
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false }
        },
        "plugins": {
            "broker_factories": {
                "qmp_broker": { "path": plugin_path() }
            }
        }
    })
}

fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

fn find_process<'a>(ids: &'a [ProcessIdentifier], name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found in {ids:?}"))
}

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// qmp 経由で得た識別子は、アクセスした qmp ブローカーを指す
fn check_processes(bp: &mut dyn BrokerProxy, broker_name: &str) -> JuizResult<()> {
    assert!(bp.system_profile_full()?.is_object());
    assert!(bp.system_uuid()?.as_str().is_some());

    let ids = bp.process_list(false, None)?;
    assert_eq!(ids.len(), 2, "{ids:?}");
    for id in ids.iter() {
        assert_eq!(id.broker_type_name, "qmp", "{id:?}");
        assert_eq!(id.broker_name, broker_name, "{id:?}");
    }
    // 返された qmp:// の識別子をそのまま渡しても、サーバーが自分宛てに再接続せずに処理できる
    let inc0 = find_process(&ids, "inc0");
    assert_eq!(int_of(bp.process_call(inc0, args("arg1", 10))), 11);
    assert_eq!(int_of(bp.any_process_call(inc0, args("arg1", 20))), 21);
    assert_eq!(bp.process_profile_full(inc0)?.name, "inc0");
    bp.process_p_apply(inc0, "arg1", jvalue!(1).into())?;
    assert_eq!(int_of(bp.process_execute(inc0)), 2);
    Ok(())
}

fn check_containers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let names: Vec<String> = bp
        .container_list(false, None)?
        .iter()
        .map(|id| bp.container_profile_full(id).map(|p| p.name))
        .collect::<JuizResult<_>>()?;
    assert_eq!(names, vec!["c0"]);

    let ids = bp.container_process_list(false, None)?;
    assert_eq!(ids.len(), 2, "{ids:?}");
    let add = find_process(&ids, "c0inc");
    let get = find_process(&ids, "c0get");
    let before = int_of(bp.container_process_call(get, CapsuleMap::new()));
    assert_eq!(
        int_of(bp.container_process_call(add, args("arg1", 4))),
        before + 4
    );
    assert_eq!(int_of(bp.any_process_call(get, CapsuleMap::new())), before + 4);
    Ok(())
}

/// サーバー側の処理が失敗するとエラーが返り、同じ接続でその後も呼び出せる
fn check_error_keeps_connection(bp: &mut dyn BrokerProxy, broker_name: &str) -> JuizResult<()> {
    let missing: ProcessIdentifier =
        format!("qmp://{broker_name}/process/nothing::increment_process").try_into()?;
    let start = Instant::now();
    assert!(bp.process_call(&missing, args("arg1", 1)).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(bp.process_profile_full(&missing).is_err());
    assert_eq!(bp.process_list(false, None)?.len(), 2);
    Ok(())
}

fn check_connections(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let src = find_process(&ids, "inc0").clone();
    let dst = find_process(&ids, "inc1").clone();
    let cm = ConnectionManifest::new(ConnectionType::Pull, src.clone(), "arg1".to_owned(), dst.clone());
    assert_eq!(bp.connection_create(&cm)?.arg_name, "arg1");
    let cons = bp.connection_list(false)?;
    assert_eq!(cons.len(), 1, "{cons:?}");

    // inc1 を実行すると inc0 の出力を pull して使う
    let src_out = int_of(bp.process_execute(&src));
    assert_eq!(int_of(bp.process_execute(&dst)), src_out + 1);

    assert_eq!(bp.connection_destroy(&cons[0])?.arg_name, "arg1");
    assert!(bp.connection_list(false)?.is_empty());
    Ok(())
}

fn check_create_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let created = bp.process_create(&ProcessManifest::new("increment_process").name("inc_new"))?;
    assert_eq!(created.name, "inc_new");
    let ids = bp.process_list(false, None)?;
    assert_eq!(ids.len(), 3);
    assert_eq!(
        int_of(bp.process_call(find_process(&ids, "inc_new"), args("arg1", 1))),
        2
    );
    assert_eq!(bp.process_destroy(&created.identifier())?.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), 2);
    Ok(())
}

fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ecs = bp.ec_list(false)?;
    let id = ecs
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .find(|id| id.contains("timer0"))
        .expect("timer0 not listed")
        .to_owned();
    let state = |bp: &mut dyn BrokerProxy| -> JuizResult<Value> {
        Ok(bp.ec_get_state(&id)?["__value__"].clone())
    };
    assert_eq!(state(bp)?, jvalue!("STOPPED"));
    bp.ec_start(&id)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while state(bp)? != jvalue!("STARTED") {
        assert!(Instant::now() < deadline, "timer0 did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    bp.ec_stop(&id)?;
    Ok(())
}

#[test]
fn qmp_broker_api_in_same_process() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    System::new(server_manifest("qmp_broker_api_test", PORT, 8032))?
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

/// テストが失敗しても juiz サーバーのプロセスを残さない
struct ServerProcess(Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_server() -> ServerProcess {
    let juiz = target_dir().join(format!("juiz{}", std::env::consts::EXE_SUFFIX));
    assert!(
        juiz.exists(),
        "{juiz:?} not found. Run `cargo build` before this test."
    );
    let conf = std::env::temp_dir().join(format!("qmp_broker_test_server_{}.conf", std::process::id()));
    // JSON は YAML としても読める
    std::fs::write(
        &conf,
        server_manifest("qmp_broker_test_server", SERVER_PORT, 8033).to_string(),
    )
    .unwrap();
    let child = Command::new(juiz)
        .arg("-f")
        .arg(&conf)
        .arg("-d")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start juiz server");
    ServerProcess(child)
}

#[test]
fn qmp_broker_api_between_processes() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let mut server = spawn_server();
    let broker_name = format!("127.0.0.1:{SERVER_PORT}");
    System::new(client_manifest("qmp_broker_test_client"))?
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
