//! qmp ブローカーのテストで共通に使う設定と確認の関数
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

pub fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug")
}

pub fn plugin_path() -> String {
    target_dir().to_string_lossy().into_owned()
}

/// qmp のプラグインを読み込む設定。組み込みの qmp を使うときは空にする
fn broker_factories(use_plugin: bool) -> Value {
    if use_plugin {
        jvalue!({ "qmp_broker": { "path": plugin_path() } })
    } else {
        jvalue!({})
    }
}

/// qmp ブローカーを持ち、Process / Container / EC を読み込んだ System の設定
pub fn server_manifest(name: &str, port: i64, http_port: i64, use_plugin: bool) -> Value {
    let path = plugin_path();
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false, "port": http_port }
        },
        "plugins": {
            "broker_factories": broker_factories(use_plugin),
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
pub fn client_manifest(name: &str, use_plugin: bool) -> Value {
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false }
        },
        "plugins": {
            "broker_factories": broker_factories(use_plugin)
        }
    })
}

pub fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

pub fn find_process<'a>(ids: &'a [ProcessIdentifier], name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found in {ids:?}"))
}

pub fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// qmp 経由で得た識別子は、アクセスした qmp ブローカーを指す
pub fn check_processes(bp: &mut dyn BrokerProxy, broker_name: &str) -> JuizResult<()> {
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

pub fn check_containers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
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
    assert_eq!(
        int_of(bp.any_process_call(get, CapsuleMap::new())),
        before + 4
    );
    Ok(())
}

/// サーバー側の処理が失敗するとエラーが返り、同じ接続でその後も呼び出せる
pub fn check_error_keeps_connection(bp: &mut dyn BrokerProxy, broker_name: &str) -> JuizResult<()> {
    let missing: ProcessIdentifier =
        format!("qmp://{broker_name}/process/nothing::increment_process").try_into()?;
    let start = Instant::now();
    assert!(bp.process_call(&missing, args("arg1", 1)).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(bp.process_profile_full(&missing).is_err());
    assert_eq!(bp.process_list(false, None)?.len(), 2);
    Ok(())
}

pub fn check_connections(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let src = find_process(&ids, "inc0").clone();
    let dst = find_process(&ids, "inc1").clone();
    let cm = ConnectionManifest::new(
        ConnectionType::Pull,
        src.clone(),
        "arg1".to_owned(),
        dst.clone(),
    );
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

pub fn check_create_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
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

pub fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
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

/// テストが失敗しても juiz サーバーのプロセスを残さない
pub struct ServerProcess(pub Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn spawn_server(port: i64, http_port: i64, use_plugin: bool) -> ServerProcess {
    let juiz = target_dir().join(format!("juiz{}", std::env::consts::EXE_SUFFIX));
    assert!(
        juiz.exists(),
        "{juiz:?} not found. Run `cargo build` before this test."
    );
    let conf = std::env::temp_dir().join(format!(
        "qmp_broker_test_server_{}_{port}.conf",
        std::process::id()
    ));
    // JSON は YAML としても読める
    std::fs::write(
        &conf,
        server_manifest("qmp_broker_test_server", port, http_port, use_plugin).to_string(),
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
