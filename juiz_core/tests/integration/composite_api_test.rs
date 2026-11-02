//! 実行中に Composite を定義する API (issue #171) のテスト
//!
//! HTTP の BrokerProxy から composite/create・list・profile_full・destroy と composite_container/create を呼び、
//! 作った Composite でインスタンスを作って動かす。分かりやすいエラーを返すこと、消せないものを消さないことも確かめる。
//! qmp は brokers/qmp_broker/tests/qmp_composite_api_test.rs。
//!
//! 仕様は docs/spec/composition.md の §9。
//! プラグイン (increment_process、example_container とその ContainerProcess) は ../target/debug から読む。
use std::sync::{Arc, RwLock};

use juiz_core::prelude::*;
use juiz_sdk::{manifests::CompositeManifest, process_identifier::ProcessIdentifier};

const PORT: i64 = 8021;

fn system_manifest() -> Value {
    jvalue!({
        "name": "composite_api_test",
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
        "containers": [
            { "type_name": "example_container", "name": "ca" },
            { "type_name": "example_container", "name": "cb", "namespace": "robot1" },
        ],
    })
}

fn composite(v: Value) -> CompositeManifest {
    serde_json::from_value(v).expect("invalid CompositeManifest in test")
}

fn inc_twice() -> CompositeManifest {
    composite(jvalue!({
        "type_name": "inc_twice",
        "body": {
            "process": "increment_process",
            "args": { "arg1": { "process": "increment_process", "args": { "arg1": { "expose": "x" } } } }
        }
    }))
}

fn inc_four() -> CompositeManifest {
    composite(jvalue!({
        "type_name": "inc_four",
        "body": { "process": "inc_twice", "args": { "x": { "process": "inc_twice" } } }
    }))
}

fn add_chain() -> CompositeManifest {
    composite(jvalue!({
        "type_name": "add_chain",
        "containers": { "a": "example_container", "b": "example_container" },
        "body": {
            "process": "example_container_increment",
            "container": "b",
            "args": { "arg1": { "process": "example_container_increment", "container": "a" } }
        }
    }))
}

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

fn error_text<T: std::fmt::Debug>(r: JuizResult<T>) -> String {
    format!("{:#}", r.expect_err("must fail"))
}

fn http_proxy(system: &mut System) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
    let host = format!("localhost:{PORT}");
    system
        .core_broker()
        .lock()?
        .worker()
        .broker_proxy("http", host.as_str(), true)
}

/// Composite の Factory を作り、一覧・profile に出て、インスタンスを作って呼べる
fn check_create_and_use(system: &mut System, bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    assert!(bp.composite_list()?.is_empty());

    let m = bp.composite_create(&inc_twice())?;
    assert_eq!(m.type_name, "inc_twice");
    assert!(m.container_type.is_none());
    assert!(m.composition.is_some());
    assert!(m.arguments.iter().any(|a| a.name == "x"), "{m:?}");

    bp.composite_create(&inc_four())?;
    assert_eq!(bp.composite_list()?, vec!["inc_four", "inc_twice"]);
    let profile = bp.composite_profile_full("inc_four")?;
    assert_eq!(
        profile.composition.expect("composition").body.process,
        "inc_twice"
    );

    // 作った Composite は普通の Process の型として使える
    bp.process_create(&ProcessManifest::new("inc_twice").name("i0"))?;
    let i0: ProcessIdentifier = "core://core/process/i0::inc_twice".try_into()?;
    let args: CapsuleMap = vec![("x", jvalue!(3))].into();
    assert_eq!(
        int_of(system.core_broker().lock()?.process_call(&i0, args)),
        5
    );

    // HTTP の JSON でも読める
    let names: Value =
        reqwest::blocking::get(format!("http://localhost:{PORT}/api/composite/list"))?.json()?;
    assert_eq!(names, jvalue!(["inc_four", "inc_twice"]));
    Ok(())
}

/// 定義の誤りは、理由が分かるエラーになり、何も登録しない
fn check_create_errors(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    // 名前の衝突 (Composite 同士、プラグインの Factory と)
    let e = error_text(bp.composite_create(&inc_twice()));
    assert!(e.contains("inc_twice") && e.contains("already"), "{e}");
    let mut same_as_plugin = inc_twice();
    same_as_plugin.type_name = "increment_process".to_owned();
    let e = error_text(bp.composite_create(&same_as_plugin));
    assert!(e.contains("increment_process"), "{e}");

    // 子の Factory が無い
    let e = error_text(bp.composite_create(&composite(jvalue!({
        "type_name": "broken",
        "body": { "process": "no_such_process" }
    }))));
    assert!(e.contains("no_such_process"), "{e}");

    // 子の引数に無い名前
    let e = error_text(bp.composite_create(&composite(jvalue!({
        "type_name": "broken",
        "body": { "process": "increment_process", "args": { "no_such_arg": { "const": 1 } } }
    }))));
    assert!(e.contains("no_such_arg"), "{e}");

    assert_eq!(bp.composite_list()?, vec!["inc_four", "inc_twice"]);
    Ok(())
}

/// インスタンスや子として使う Composite があるうちは消せない。Composite でない Factory も消せない
fn check_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let e = error_text(bp.composite_destroy("inc_twice"));
    assert!(
        e.contains("core://core/process/i0::inc_twice") && e.contains("Composite(inc_four)"),
        "{e}"
    );
    let e = error_text(bp.composite_destroy("increment_process"));
    assert!(e.contains("not a Composite"), "{e}");
    let e = error_text(bp.composite_destroy("no_such_composite"));
    assert!(e.contains("no_such_composite"), "{e}");
    let e = error_text(bp.composite_profile_full("increment_process"));
    assert!(e.contains("not a Composite"), "{e}");

    bp.process_destroy(&"core://core/process/i0::inc_twice".try_into()?)?;
    assert_eq!(bp.composite_destroy("inc_four")?.type_name, "inc_four");
    assert_eq!(bp.composite_destroy("inc_twice")?.type_name, "inc_twice");
    assert!(bp.composite_list()?.is_empty());
    // 消した型ではインスタンスを作れない
    assert!(bp
        .process_create(&ProcessManifest::new("inc_twice").name("i1"))
        .is_err());
    Ok(())
}

/// スロットを持つ Composite と CompositeContainer を API で作り、動かして消す
fn check_composite_container(system: &mut System, bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let m = bp.composite_create(&add_chain())?;
    assert_eq!(
        m.container_type.as_deref(),
        Some("composite_container"),
        "{m:?}"
    );

    let profile = bp.composite_container_create(&jvalue!({
        "name": "cc0",
        "namespace": "/robot1",
        "slots": { "a": "/ca", "b": "cb" },
        "processes": [ { "type_name": "add_chain", "name": "chain0" } ]
    }))?;
    assert_eq!(profile.namespace, "robot1");
    let slots = profile.slots.expect("slots");
    assert_eq!(
        slots.get("a").map(String::as_str),
        Some("core://core/container/ca::example_container")
    );
    assert_eq!(
        slots.get("b").map(String::as_str),
        Some("core://core/container/robot1/cb::example_container")
    );

    let chain0: ProcessIdentifier =
        "core://core/container_process/robot1/chain0::add_chain:cc0".try_into()?;
    let before = int_of(
        system
            .core_broker()
            .lock()?
            .container_process_call(&chain0, vec![("arg1", jvalue!(1))].into()),
    );
    let after = int_of(
        system
            .core_broker()
            .lock()?
            .container_process_call(&chain0, vec![("arg1", jvalue!(1))].into()),
    );
    assert!(after > before, "{before} -> {after}");

    // 誤りがあれば何も残さない (ContainerProcess の作成に失敗したら CompositeContainer も消す)
    let e = error_text(bp.composite_container_create(&jvalue!({
        "name": "cc1",
        "slots": { "a": "no_such_container", "b": "ca" },
    })));
    assert!(e.contains("no_such_container"), "{e}");
    let e = error_text(bp.composite_container_create(&jvalue!({
        "name": "cc1",
        "slots": { "a": "ca", "b": "ca" },
        "processes": [ { "type_name": "no_such_composite", "name": "x0" } ]
    })));
    assert!(e.contains("no_such_composite"), "{e}");
    let e = error_text(bp.composite_container_create(&jvalue!({ "name": "cc1" })));
    assert!(e.contains("slots"), "{e}");
    let names = bp
        .container_list(false, None)?
        .into_iter()
        .map(|id| id.name)
        .collect::<Vec<_>>();
    assert!(!names.contains(&"cc1".to_owned()), "{names:?}");

    // インスタンスがあるうちは Composite を消せない。CompositeContainer は container/destroy で消す
    let e = error_text(bp.composite_destroy("add_chain"));
    assert!(e.contains("chain0"), "{e}");
    let cc0: ContainerIdentifier = "core://core/container/robot1/cc0::composite_container"
        .to_owned()
        .try_into()?;
    assert_eq!(bp.container_destroy(&cc0)?.name, "cc0");
    assert_eq!(bp.composite_destroy("add_chain")?.type_name, "add_chain");
    Ok(())
}

#[test]
fn composite_can_be_defined_at_runtime() -> JuizResult<()> {
    Ok(System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = http_proxy(system)?;
            let mut bp = juiz_write(&proxy)?;
            check_create_and_use(system, &mut *bp)?;
            check_create_errors(&mut *bp)?;
            check_destroy(&mut *bp)?;
            check_composite_container(system, &mut *bp)?;
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}
