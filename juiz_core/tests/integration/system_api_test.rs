//! System の公開 API のテスト (issue #221)
//!
//! CLI (juiz_app) が使う System の API を、HTTP で繋いだ 2 つの System で確かめる。
//! - add_subsystem_by_id / add_systemproxy_by_id でもう一方の System を登録する
//! - process_list などの recursive = true でサブシステムの中身も数える
//! - start_http_broker、has_broker_factory、create_broker_proxy、profile_full
//! - run_and_do は SIGINT / SIGTERM(Windows では SIGBREAK も)を受けるまで spin し続け、受けたら止まる
//!
//! プラグイン (increment_process、example_container とその ContainerProcess、timer_ec) は ../target/debug から読む。

use juiz_core::prelude::*;

const MASTER_PORT: i64 = 8028;
const SUB_PORT: i64 = 8029;

fn sub_manifest() -> Value {
    jvalue!({
        "name": "system_api_test_sub",
        "option": { "http_broker": { "port": SUB_PORT } },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" }
            },
            "container_factories": {
                "example_container": {
                    "path": "../target/debug",
                    "processes": {
                        "example_container_get": { "path": "../target/debug" }
                    }
                }
            },
            "ec_factories": {
                "timer_ec": { "path": "../target/debug" }
            }
        },
        "processes": [ { "type_name": "increment_process", "name": "inc0" } ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [ { "type_name": "example_container_get", "name": "c0get" } ] }
        ],
        "ecs": [ { "type_name": "TimerEC", "name": "timer0", "rate": 10.0 } ]
    })
}

fn master_manifest() -> Value {
    jvalue!({
        "name": "system_api_test_master",
        "option": { "http_broker": { "port": MASTER_PORT } },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" }
            }
        },
        "processes": [ { "type_name": "increment_process", "name": "minc0" } ]
    })
}

fn names<T: Clone + Into<Identifier>>(ids: &[T]) -> Vec<String> {
    let mut names: Vec<String> = ids
        .iter()
        .map(|id| {
            IdentifierStruct::try_from(Into::<Identifier>::into(id.clone()))
                .unwrap()
                .object_name
        })
        .collect();
    names.sort();
    names
}

fn check_lists(master: &System) -> JuizResult<()> {
    assert_eq!(names(&master.process_list(false)?), vec!["minc0"]);
    assert_eq!(names(&master.process_list(true)?), vec!["inc0", "minc0"]);
    assert!(master.container_list(false)?.is_empty());
    assert_eq!(names(&master.container_list(true)?), vec!["c0"]);
    assert_eq!(names(&master.container_process_list(true)?), vec!["c0get"]);
    assert_eq!(
        names(&master.any_process_list(true)?),
        vec!["c0get", "inc0", "minc0"]
    );
    assert!(master.ec_list(false)?.is_empty());
    let ecs = master.ec_list(true)?;
    assert_eq!(ecs.len(), 1, "{ecs:?}");
    assert!(ecs[0].as_str().unwrap().contains("timer0"), "{ecs:?}");
    Ok(())
}

fn check_other_apis(master: &mut System) -> JuizResult<()> {
    assert!(master.has_broker_factory("http")?);
    assert!(!master.has_broker_factory("nothing")?);
    master.wait_brokers_started()?;

    let bp = master.create_broker_proxy(&jvalue!({
        "type_name": "http",
        "name": format!("localhost:{SUB_PORT}")
    }))?;
    assert_eq!(juiz_lock(&bp)?.process_list(false, None)?.len(), 1);

    let profile = master.profile_full()?;
    assert!(profile["core_broker"].is_object(), "{profile:?}");
    Ok(())
}

#[test]
fn system_api_with_subsystem() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(sub_manifest())?
        .start_http_broker(true)
        .setup()?
        .run_and_do_once(|_sub| {
            System::new(master_manifest())?
                .start_http_broker(true)
                .setup()?
                .add_subsystem_by_id(Some(format!("http://localhost:{SUB_PORT}")))?
                .add_systemproxy_by_id(Some(format!("http://localhost:{SUB_PORT}")))?
                // 何も渡さなければ何もしない
                .add_subsystem_by_id(None)?
                .add_systemproxy_by_id(None)?
                .run_and_do_once(|master| {
                    check_lists(master)?;
                    check_other_apis(master)?;
                    Ok(())
                })
        })
}

/// マニフェストの subsystems / mastersystem でも、起動時にもう一方の System を登録できる
#[test]
fn system_manifest_subsystems_and_mastersystem() -> JuizResult<()> {
    crate::common::logger::init();
    const PORT_A: i64 = 8030;
    const PORT_B: i64 = 8031;
    let with_port = |mut m: Value, name: &str, port: i64| {
        m["name"] = jvalue!(name);
        m["option"]["http_broker"]["port"] = jvalue!(port);
        m
    };
    System::new(with_port(sub_manifest(), "system_manifest_sub", PORT_A))?
        .start_http_broker(true)
        .setup()?
        .run_and_do_once(|_sub| {
            // subsystems: 起動したらサブシステムとして登録する
            let mut m = with_port(master_manifest(), "system_manifest_master", PORT_B);
            m["subsystems"] =
                jvalue!([{ "type_name": "http", "name": format!("localhost:{PORT_A}") }]);
            System::new(m)?
                .start_http_broker(true)
                .setup()?
                .run_and_do_once(|master| {
                    assert_eq!(names(&master.process_list(true)?), vec!["inc0", "minc0"]);
                    Ok(())
                })?;

            // mastersystem: 起動したら相手をマスターとして登録し、相手のサブシステムになる
            let mut m = with_port(master_manifest(), "system_manifest_child", PORT_B);
            m["mastersystem"] =
                jvalue!({ "broker_type": "http", "broker_name": format!("localhost:{PORT_A}") });
            System::new(m)?
                .start_http_broker(true)
                .setup()?
                .run_and_do_once(|child| {
                    let profile = child.core_broker().lock()?.system_profile_full()?;
                    assert!(profile["mastersystem"].is_object(), "{profile:?}");
                    Ok(())
                })
        })
}

#[test]
fn system_api_errors() -> JuizResult<()> {
    crate::common::logger::init();
    // マニフェストはオブジェクトでなければならない
    assert!(System::new(jvalue!([1, 2])).is_err());

    // 解析できない識別子
    assert!(System::new(jvalue!({ "name": "system_api_errors" }))?
        .add_subsystem_by_id(Some("invalid".to_owned()))
        .is_err());
    assert!(System::new(jvalue!({ "name": "system_api_errors" }))?
        .add_systemproxy_by_id(Some("invalid".to_owned()))
        .is_err());
    // http_broker の設定が無くても start_http_broker で足す
    System::new(jvalue!({ "name": "system_api_errors" }))?
        .start_http_broker(false)
        .setup()?
        .run_and_do_once(|_| Ok(()))
}

/// run_and_do は渡した関数を呼んだあと、SIGTERM を受けるまで spin を続ける
#[test]
fn system_run_and_do_until_signal() -> JuizResult<()> {
    crate::common::logger::init();
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let spins = Arc::new(AtomicUsize::new(0));
    let spins2 = spins.clone();
    let mut called = false;
    let mut system = System::new(jvalue!({ "name": "system_run_test" }))?.setup()?;
    system.set_spin_sleeptime(std::time::Duration::from_millis(1));
    // spin は SIGTERM の受け取りを登録したあとに呼ばれるので、ここで送ってもプロセスは止まらない
    system.set_spin_callback(Box::new(move || {
        if spins2.fetch_add(1, Ordering::SeqCst) == 2 {
            signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
        }
        Ok(())
    }));
    system.run_and_do(|_| {
        called = true;
        Ok(())
    })?;
    assert!(called);
    assert!(spins.load(Ordering::SeqCst) >= 3);
    Ok(())
}

/// Windows の Ctrl+Break(SIGBREAK)でも、SIGTERM と同じく止まる (issue #307)
#[cfg(windows)]
#[test]
fn system_run_and_do_until_sigbreak() -> JuizResult<()> {
    crate::common::logger::init();
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let spins = Arc::new(AtomicUsize::new(0));
    let spins2 = spins.clone();
    let mut system = System::new(jvalue!({ "name": "system_run_sigbreak_test" }))?.setup()?;
    system.set_spin_sleeptime(std::time::Duration::from_millis(1));
    system.set_spin_callback(Box::new(move || {
        if spins2.fetch_add(1, Ordering::SeqCst) == 2 {
            signal_hook::low_level::raise(signal_hook::consts::SIGBREAK).unwrap();
        }
        Ok(())
    }));
    system.run_and_do(|_| Ok(()))?;
    assert!(spins.load(Ordering::SeqCst) >= 3);
    Ok(())
}
