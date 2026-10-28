//! Composite の中身を外から読めること (issue #170) のテスト
//!
//! - HTTP の API(管理画面が JSON のまま読む)と HTTP の BrokerProxy で、Composite の profile に
//!   `composition`、CompositeContainer の profile に `slots` が出る。qmp は brokers/qmp_broker のテスト。
//! - ContainerProxy の profile が相手のブローカーから取れる(`juiz container list --long` で使う)
//! - `trace: true` の Composite は、ノードを評価するたびに位置と戻り値をログに出す
//!
//! 仕様は docs/spec/composition.md の §5.4・§8.2。
//! プラグイン (increment_process、example_container とその ContainerProcess) は ../target/debug から読む。
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::common;

const PORT: i64 = 8019;

fn system_manifest() -> Value {
    jvalue!({
        "name": "composite_display_test",
        "option": { "http_broker": { "start": true, "port": PORT } },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" }
            },
            "container_factories": {
                "example_container": {
                    "path": "../target/debug",
                    "processes": {
                        "example_container_increment": { "path": "../target/debug" },
                        "example_container_get": { "path": "../target/debug" },
                    }
                }
            }
        },
        "composites": [
            {
                "type_name": "inc_twice",
                "body": {
                    "process": "increment_process",
                    "args": { "arg1": { "process": "increment_process", "args": { "arg1": { "expose": "x" } } } }
                }
            },
            {
                "type_name": "add_chain",
                "containers": { "a": "example_container", "b": "example_container" },
                "body": {
                    "process": "example_container_increment",
                    "container": "b",
                    "args": {
                        "arg1": { "process": "example_container_increment", "container": "a" }
                    }
                }
            }
        ],
        "processes": [
            { "type_name": "inc_twice", "name": "traced0", "trace": true },
            { "type_name": "inc_twice", "name": "quiet0" },
        ],
        "containers": [
            { "type_name": "example_container", "name": "ca" },
            { "type_name": "example_container", "name": "cb", "namespace": "robot1" },
        ],
        "composite_containers": [
            {
                "name": "cc0",
                "slots": { "a": "ca", "b": "/robot1/cb" },
                "processes": [ { "type_name": "add_chain", "name": "chain0" } ]
            }
        ]
    })
}

/// 正規化した inc_twice の式の木 (書かなかった引数も args を持つ)
fn inc_twice_body() -> Value {
    jvalue!({
        "process": "increment_process",
        "args": {
            "arg1": {
                "process": "increment_process",
                "args": { "arg1": { "expose": "x" } }
            }
        }
    })
}

const CA: &str = "core://core/container/ca::example_container";
const CB: &str = "core://core/container/robot1/cb::example_container";

/// trace の出力を確かめるため、ログは共通の logger に溜めておく
fn composite_logs() -> Vec<String> {
    common::logger::captured()
        .iter()
        .filter(|m| m.starts_with("Composite("))
        .cloned()
        .collect()
}

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

fn http_get(path: &str, identifier: Option<&str>) -> JuizResult<Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let mut req = client.get(format!("http://localhost:{PORT}/api/{path}"));
    if let Some(id) = identifier {
        req = req.query(&[("identifier", id)]);
    }
    let res = req.send()?;
    assert!(res.status().is_success(), "{path}: {:?}", res.text());
    Ok(res.json()?)
}

/// 管理画面が読む HTTP の JSON に composition と slots が出る
fn check_http_json() -> JuizResult<()> {
    let p = http_get(
        "process/profile_full",
        Some("core://core/process/traced0::inc_twice"),
    )?;
    assert_eq!(p["composition"]["body"], inc_twice_body(), "{p}");
    assert_eq!(p["composition"]["containers"], jvalue!({}), "{p}");

    let cp = http_get(
        "container_process/profile_full",
        Some("core://core/container_process/chain0::add_chain:cc0"),
    )?;
    let composition = &cp["composition"];
    assert_eq!(
        composition["containers"],
        jvalue!({ "a": "example_container", "b": "example_container" }),
        "{cp}"
    );
    assert_eq!(composition["body"]["container"], jvalue!("b"), "{cp}");
    assert_eq!(
        composition["body"]["args"]["arg1"]["container"],
        jvalue!("a"),
        "{cp}"
    );
    assert_eq!(
        composition["body"]["args"]["arg1"]["args"]["arg1"],
        jvalue!({ "expose": "arg1" }),
        "{cp}"
    );

    let cc = http_get(
        "container/profile_full",
        Some("core://core/container/cc0::composite_container"),
    )?;
    assert_eq!(cc["slots"], jvalue!({ "a": CA, "b": CB }), "{cc}");
    // 普通の Container と Process には出さない
    let ca = http_get("container/profile_full", Some(CA))?;
    assert!(ca.get("slots").is_none(), "{ca}");

    // 管理画面は system/profile_full?recursive=true の core_store から読む
    let sys = http_get("system/profile_full?recursive=true", None)?;
    let store = &sys["core_store"];
    let find = |list: &str, name: &str| -> Value {
        store[list]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == jvalue!(name))
            .unwrap_or_else(|| panic!("{name} not found in {list}: {sys}"))
            .clone()
    };
    assert_eq!(
        find("processes", "traced0")["composition"]["body"],
        inc_twice_body()
    );
    assert!(find("container_processes", "chain0")["composition"].is_object());
    assert_eq!(
        find("containers", "cc0")["slots"],
        jvalue!({ "a": CA, "b": CB })
    );
    Ok(())
}

/// HTTP の BrokerProxy でも型に読み込んだ profile に composition と slots が残る
fn check_http_broker_proxy(system: &mut System) -> JuizResult<()> {
    let host = format!("localhost:{PORT}");
    let proxy = system
        .core_broker()
        .lock()?
        .worker()
        .broker_proxy("http", host.as_str(), true)?;
    let bp = juiz_lock(&proxy)?;
    let traced0: ProcessIdentifier =
        format!("http://{host}/process/traced0::inc_twice").try_into()?;
    let profile = bp.process_profile_full(&traced0)?;
    assert_eq!(
        serde_json::to_value(profile.composition.expect("composition"))?["body"],
        inc_twice_body()
    );

    let chain0: ProcessIdentifier =
        format!("http://{host}/container_process/chain0::add_chain:cc0").try_into()?;
    let profile = bp.container_process_profile_full(&chain0)?;
    let composition = profile.composition.expect("composition");
    assert_eq!(composition.containers.len(), 2);
    assert_eq!(composition.body.container.as_deref(), Some("b"));

    let cc0: ContainerIdentifier =
        format!("http://{host}/container/cc0::composite_container").try_into()?;
    let slots = bp.container_profile_full(&cc0)?.slots.expect("slots");
    assert_eq!(slots.get("a").map(String::as_str), Some(CA));
    assert_eq!(slots.get("b").map(String::as_str), Some(CB));
    Ok(())
}

/// CoreWorker が返す ContainerProxy の profile は相手のブローカーから取る(以前は todo! で落ちていた)
fn check_container_proxy_profile(system: &mut System) -> JuizResult<()> {
    let cc0: ContainerIdentifier =
        format!("http://localhost:{PORT}/container/cc0::composite_container").try_into()?;
    let container = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .container_from_identifier(&cc0)?;
    assert_eq!(container.identifier(), cc0);
    let profile = container.lock()?.profile()?;
    assert_eq!(profile.name, "cc0");
    assert_eq!(profile.slots.expect("slots").len(), 2);
    Ok(())
}

/// trace: true の Composite だけが、ノードごとに位置と戻り値をログに出す
fn check_trace(system: &mut System) -> JuizResult<()> {
    let args = || -> CapsuleMap { vec![("x", jvalue!(3))].into() };
    common::logger::clear_captured();
    let quiet0: ProcessIdentifier = "core://core/process/quiet0::inc_twice".try_into()?;
    assert_eq!(
        int_of(system.core_broker().lock()?.process_call(&quiet0, args())),
        5
    );
    assert!(composite_logs().is_empty(), "{:?}", composite_logs());

    let traced0: ProcessIdentifier = "core://core/process/traced0::inc_twice".try_into()?;
    assert_eq!(
        int_of(system.core_broker().lock()?.process_call(&traced0, args())),
        5
    );
    // 評価順 (深さ優先) に、根以外のノードも出る
    assert_eq!(
        composite_logs(),
        vec![
            "Composite(inc_twice) body.arg1 (increment_process) -> 4",
            "Composite(inc_twice) body (increment_process) -> 5",
        ]
    );
    Ok(())
}

#[test]
fn composite_structure_is_visible_from_outside() -> JuizResult<()> {
    common::logger::init();
    Ok(System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            check_trace(system)?;
            check_http_json()?;
            check_http_broker_proxy(system)?;
            check_container_proxy_profile(system)?;
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}
