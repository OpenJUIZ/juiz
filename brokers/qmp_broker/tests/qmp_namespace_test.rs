//! ネームスペース付きのオブジェクトを qmp ブローカー越しに扱うテスト (issue #150)
//!
//! - qmp のサブシステム越しに、ネームスペース付きの Process / ContainerProcess を呼べる
//! - list の param `namespace` で、そのネームスペースとその下だけに絞り込める
//!
//! プラグイン (qmp_broker, increment_process, example_container) は
//! cargo build で ../../target/debug にビルドされている前提 (CI の test.yml と同じ)。

mod common;

use std::collections::HashMap;

use common::*;
use juiz_core::prelude::*;
use juiz_core::CRUDBrokerProxy;
use juiz_sdk::process_identifier::ProcessIdentifier;
use qmp_broker::QuicBrokerProxy;

const PORT: i64 = 5017;
const MASTER_PORT: i64 = 5018;

/// qmp ブローカーを持ち、robot1 (既定)・robot1/arm・robot2・ルートに Process を置いた System
fn sub_manifest() -> Value {
    let mut manifest = server_manifest("qmp_namespace_sub", PORT, 8037, true);
    manifest["namespace"] = jvalue!("robot1");
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "inc0" },
        { "type_name": "increment_process", "name": "inc1", "namespace": "arm" },
        { "type_name": "increment_process", "name": "inc0", "namespace": "/robot2" },
        { "type_name": "increment_process", "name": "inc0", "namespace": "/" }
    ]);
    manifest["containers"] = jvalue!([
        { "type_name": "example_container", "name": "c0", "initial_value": 100,
          "processes": [ { "type_name": "example_container_increment", "name": "c0inc" } ] }
    ]);
    manifest["ecs"] = jvalue!([
        { "type_name": "TimerEC", "name": "timer0", "rate": 100.0 }
    ]);
    manifest
}

/// qmp のサブシステムとして sub を登録する System。
/// 登録するとき相手に自分の qmp ブローカーを教えるので、自分も qmp ブローカーを持つ
fn master_manifest() -> Value {
    let mut manifest = client_manifest("qmp_namespace_master", true);
    manifest["brokers"] = jvalue!([
        { "type_name": "qmp", "name": format!("127.0.0.1:{MASTER_PORT}"), "host": "127.0.0.1", "port": MASTER_PORT }
    ]);
    manifest["subsystems"] = jvalue!([
        { "type_name": "qmp", "name": format!("127.0.0.1:{PORT}") }
    ]);
    manifest
}

fn find<'a>(ids: &'a [ProcessIdentifier], full_name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.full_name() == full_name)
        .unwrap_or_else(|| panic!("{full_name} not found in {ids:?}"))
}

fn check_call_over_subsystem(master: &System) -> JuizResult<()> {
    let core = master.core_broker();
    let ids = core.lock()?.process_list(true, None)?;
    // サブシステムの Process は qmp://<住所>/process/<namespace>/<name>::<type> で見える
    let r1 = find(&ids, "robot1/inc0");
    assert_eq!(r1.broker_type_name, "qmp", "{r1:?}");
    assert_eq!(r1.namespace, "robot1");
    assert!(
        r1.to_string()
            .ends_with("/process/robot1/inc0::increment_process"),
        "{r1}"
    );
    for (full_name, arg) in [
        ("robot1/inc0", 10),
        ("robot1/arm/inc1", 20),
        ("robot2/inc0", 30),
        ("inc0", 40),
    ] {
        let id = find(&ids, full_name);
        assert_eq!(
            int_of(core.lock()?.process_call(id, args("arg1", arg))),
            arg + 1,
            "{id}"
        );
    }

    let cps = core.lock()?.container_process_list(true, None)?;
    let c0inc = find(&cps, "robot1/c0inc");
    assert_eq!(
        int_of(core.lock()?.container_process_call(c0inc, args("arg1", 1))),
        101
    );
    assert_eq!(
        int_of(core.lock()?.any_process_call(c0inc, args("arg1", 2))),
        103
    );
    Ok(())
}

/// QuicBrokerProxy の CRUD で list に namespace を渡し、`<namespace>/<name>` を並べ替えて返す
fn qmp_list(proxy: &QuicBrokerProxy, class_name: &str, namespace: &str) -> JuizResult<Vec<String>> {
    let mut param: HashMap<String, String> = HashMap::new();
    param.insert("namespace".to_owned(), namespace.to_owned());
    let v = proxy.read(class_name, "list", param)?.extract_value()?;
    let mut names: Vec<String> = v
        .as_array()
        .unwrap_or_else(|| panic!("{class_name} list is not an array: {v}"))
        .iter()
        .map(|v| match v.as_str() {
            Some(id) => {
                let s: IdentifierStruct = id.to_owned().try_into().expect("invalid identifier");
                s.full_name()
            }
            None => {
                let id: ProcessIdentifier =
                    serde_json::from_value(v.clone()).expect("invalid identifier");
                id.full_name()
            }
        })
        .collect();
    names.sort();
    Ok(names)
}

fn check_list_filter() -> JuizResult<()> {
    let proxy = QuicBrokerProxy::new(&jvalue!({ "name": format!("127.0.0.1:{PORT}") }))?;
    assert_eq!(
        qmp_list(&proxy, "process", "robot1")?,
        vec!["robot1/arm/inc1", "robot1/inc0"]
    );
    assert_eq!(
        qmp_list(&proxy, "process", "/robot1/arm")?,
        vec!["robot1/arm/inc1"]
    );
    assert_eq!(qmp_list(&proxy, "process", "robot2")?, vec!["robot2/inc0"]);
    assert!(qmp_list(&proxy, "process", "robot")?.is_empty());
    assert_eq!(qmp_list(&proxy, "process", "")?.len(), 4);
    assert_eq!(qmp_list(&proxy, "container", "robot1")?, vec!["robot1/c0"]);
    assert!(qmp_list(&proxy, "container", "robot2")?.is_empty());
    assert_eq!(
        qmp_list(&proxy, "container_process", "robot1")?,
        vec!["robot1/c0inc"]
    );
    assert_eq!(
        qmp_list(&proxy, "any_process", "robot1")?,
        vec!["robot1/arm/inc1", "robot1/c0inc", "robot1/inc0"]
    );
    assert_eq!(
        qmp_list(&proxy, "execution_context", "robot1")?,
        vec!["robot1/timer0"]
    );
    assert!(qmp_list(&proxy, "execution_context", "robot2")?.is_empty());
    // 不正なネームスペースはエラー
    assert!(qmp_list(&proxy, "process", "a|b").is_err());
    Ok(())
}

#[test]
fn qmp_namespaced_objects_over_subsystem() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(sub_manifest())?
        .setup()?
        .run_and_do_once(|_sub| {
            System::new(master_manifest())?
                .setup()?
                .run_and_do_once(|master| {
                    check_call_over_subsystem(master)?;
                    check_list_filter()?;
                    Ok(())
                })
        })
}
