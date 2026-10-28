//! conf の `composites:` と `composite_containers:` (issue #169) のテスト
//!
//! 仕様は docs/spec/composition.md の §3.1・§3.3・§7.1。
//! プラグイン (increment_process、example_container とその ContainerProcess) は ../target/debug から読む。
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("execute failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

fn plugins() -> Value {
    jvalue!({
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
    })
}

/// increment を 2 回、それを 2 回 (入れ子) にする Composite の定義
fn inc_composites() -> Value {
    jvalue!([
        {
            "type_name": "inc_twice",
            "description": "arg1 に 2 を足す",
            "body": {
                "process": "increment_process",
                "args": { "arg1": { "process": "increment_process" } }
            }
        },
        {
            "type_name": "inc_four",
            "body": {
                "process": "inc_twice",
                "args": { "arg1": { "process": "inc_twice", "args": { "arg1": { "expose": "x" } } } }
            }
        },
    ])
}

/// 2 つの Container をまたぐ CompositeContainerProcess の定義。
/// a に arg1 を足し、その結果を b に足す
fn add_chain_composite() -> Value {
    jvalue!({
        "type_name": "add_chain",
        "containers": { "a": "example_container", "b": "example_container" },
        "body": {
            "process": "example_container_increment",
            "container": "b",
            "args": {
                "arg1": { "process": "example_container_increment", "container": "a" }
            }
        }
    })
}

fn system_manifest(objects: Value) -> Value {
    let mut manifest = jvalue!({
        "name": "test_system",
        "option": { "http_broker": { "start": false } },
        "plugins": plugins(),
    });
    obj_merge_mut(&mut manifest, &objects).unwrap();
    manifest
}

fn setup_error(objects: Value) -> String {
    crate::common::logger::init();
    match System::new(system_manifest(objects.clone()))
        .unwrap()
        .setup()
    {
        Ok(_) => panic!("setup must fail: {objects}"),
        Err(e) => format!("{e:?}"),
    }
}

/// conf だけで Composite Process を定義して、普通の Process と同じように作って呼べる
#[test]
fn composite_process_from_conf() -> JuizResult<()> {
    let manifest = system_manifest(jvalue!({
        "composites": inc_composites(),
        "processes": [
            { "type_name": "inc_twice", "name": "t0" },
            { "type_name": "inc_four", "name": "f0", "p_apply": { "x": 10 } },
        ],
    }));
    crate::common::logger::init();
    Ok(System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
            let t0: ProcessIdentifier = "core://core/process/t0::inc_twice".try_into()?;
            let f0: ProcessIdentifier = "core://core/process/f0::inc_four".try_into()?;
            let args = |name: &str, v: i64| -> CapsuleMap { vec![(name, jvalue!(v))].into() };
            assert_eq!(
                int_of(
                    system
                        .core_broker()
                        .lock()?
                        .process_call(&t0, args("arg1", 1))
                ),
                3
            );
            assert_eq!(
                int_of(system.core_broker().lock()?.process_call(&f0, args("x", 1))),
                5
            );
            // conf の p_apply は Composite の公開引数にも効く
            assert_eq!(
                int_of(system.core_broker().lock_mut()?.process_execute(&f0)),
                14
            );

            // profile に composition が出る
            let prof = system.core_broker().lock()?.process_profile_full(&t0)?;
            let prof = serde_json::to_value(prof)?;
            assert_eq!(
                prof["composition"]["body"]["process"],
                jvalue!("increment_process")
            );
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

/// conf だけで CompositeContainer と CompositeContainerProcess を作って呼べる。
/// スロットの名前は CompositeContainer のネームスペースで解決する
#[test]
fn composite_container_from_conf() -> JuizResult<()> {
    let manifest = system_manifest(jvalue!({
        "namespace": "robot1",
        "composites": [ add_chain_composite() ],
        "containers": [
            {
                "type_name": "example_container", "name": "c0", "initial_value": 10,
                "processes": [ { "type_name": "example_container_get", "name": "get0" } ]
            },
            {
                "type_name": "example_container", "name": "c1", "namespace": "/robot2", "initial_value": 100,
                "processes": [ { "type_name": "example_container_get", "name": "get1" } ]
            },
        ],
        "composite_containers": [
            {
                "name": "cc0",
                "slots": {
                    // 相対名 => robot1/c0
                    "a": "c0",
                    // { name, namespace } で絶対指定 => robot2/c1
                    "b": { "name": "c1", "namespace": "/robot2" },
                },
                "processes": [ { "type_name": "add_chain", "name": "chain0", "p_apply": { "arg1": 2 } } ]
            },
            {
                // ID でも参照できる。a と b に同じ Container を入れてもよい
                "name": "cc1",
                "namespace": "/tools",
                "slots": {
                    "a": "core://core/container/robot1/c0::example_container",
                    "b": "/robot1/c0",
                },
                "processes": [ { "type_name": "add_chain", "name": "chain1" } ]
            },
        ],
    }));
    crate::common::logger::init();
    Ok(System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
            let id = |s: &str| -> JuizResult<ProcessIdentifier> { s.to_owned().try_into() };
            let chain0 = id("core://core/container_process/robot1/chain0::add_chain:cc0")?;
            let chain1 = id("core://core/container_process/tools/chain1::add_chain:cc1")?;
            let get0 = id("core://core/container_process/robot1/get0::example_container_get:c0")?;
            let get1 = id("core://core/container_process/robot2/get1::example_container_get:c1")?;
            let cps = system
                .core_broker()
                .lock()?
                .container_process_list(false, None)?;
            assert!(cps.contains(&chain0) && cps.contains(&chain1), "{cps:?}");

            // p_apply の arg1=2: c0 = 10 + 2 = 12、c1 = 100 + 12 = 112
            let r = system
                .core_broker()
                .lock_mut()?
                .container_process_execute(&chain0);
            assert_eq!(int_of(r), 112);
            assert_eq!(
                int_of(
                    system
                        .core_broker()
                        .lock_mut()?
                        .container_process_execute(&get0)
                ),
                12
            );
            assert_eq!(
                int_of(
                    system
                        .core_broker()
                        .lock_mut()?
                        .container_process_execute(&get1)
                ),
                112
            );

            // cc1 は a・b とも c0 を指す。arg1 の既定値 1: c0 = 12 + 1 = 13、さらに c0 = 13 + 13 = 26
            let args: CapsuleMap = vec![("arg1", jvalue!(1))].into();
            let r = system
                .core_broker()
                .lock()?
                .container_process_call(&chain1, args);
            assert_eq!(int_of(r), 26);
            assert_eq!(
                int_of(
                    system
                        .core_broker()
                        .lock_mut()?
                        .container_process_execute(&get0)
                ),
                26
            );

            // profile の slots は参照先の ID
            let cc0 = system
                .core_broker()
                .lock()?
                .container_list(false, None)?
                .into_iter()
                .find(|c| c.name == "cc0")
                .expect("cc0 not found");
            assert_eq!(cc0.namespace, "robot1");
            let prof =
                serde_json::to_value(system.core_broker().lock()?.container_profile_full(&cc0)?)?;
            assert_eq!(
                prof["slots"]["a"],
                jvalue!("core://core/container/robot1/c0::example_container")
            );
            assert_eq!(
                prof["slots"]["b"],
                jvalue!("core://core/container/robot2/c1::example_container")
            );
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

/// 後ろで定義する Composite は子に使えない (配列の順に登録する)
#[test]
fn composite_referring_later_definition_is_error() {
    let e = setup_error(jvalue!({
        "composites": [
            { "type_name": "outer", "body": { "process": "inner" } },
            { "type_name": "inner", "body": { "process": "increment_process" } },
        ],
    }));
    assert!(e.contains("composites[0]"), "{e}");
    assert!(e.contains("inner"), "{e}");
}

/// 定義の形の誤り (知らないキー、引数の形) と、型名の重複
#[test]
fn invalid_composite_definition_is_error() {
    let e = setup_error(jvalue!({
        "composites": [ { "type_name": "c", "body": { "process": "increment_process" }, "unknown": 1 } ],
    }));
    assert!(e.contains("composites[0] is invalid"), "{e}");

    let e = setup_error(jvalue!({
        "composites": [ { "type_name": "c", "body": {
            "process": "increment_process", "args": { "arg1": { "const": 1, "expose": "x" } } } } ],
    }));
    assert!(e.contains("composites[0]"), "{e}");

    let e = setup_error(jvalue!({
        "composites": [ { "type_name": "increment_process", "body": { "process": "increment_process" } } ],
    }));
    assert!(e.contains("composites[0]"), "{e}");
}

/// スロットを持つ Composite を processes: に書く、スロットの不足・参照先なしはエラー
#[test]
fn invalid_composite_container_is_error() {
    let e = setup_error(jvalue!({
        "composites": [ add_chain_composite() ],
        "processes": [ { "type_name": "add_chain", "name": "p0" } ],
    }));
    assert!(e.contains("'add_chain' is a container process type"), "{e}");

    let containers =
        jvalue!([ { "type_name": "example_container", "name": "c0", "initial_value": 0 } ]);
    let e = setup_error(jvalue!({
        "composites": [ add_chain_composite() ],
        "containers": containers.clone(),
        "composite_containers": [
            { "name": "cc0", "slots": { "a": "c0" }, "processes": [ { "type_name": "add_chain", "name": "p0" } ] },
        ],
    }));
    assert!(e.contains("composite_containers[0]"), "{e}");

    let e = setup_error(jvalue!({
        "composites": [ add_chain_composite() ],
        "containers": containers.clone(),
        "composite_containers": [ { "name": "cc0", "slots": { "a": "c0", "b": "no_such_container" } } ],
    }));
    assert!(e.contains("no_such_container"), "{e}");

    let e = setup_error(jvalue!({
        "containers": containers,
        "composite_containers": [ { "name": "cc0" } ],
    }));
    assert!(e.contains("slots"), "{e}");
}
