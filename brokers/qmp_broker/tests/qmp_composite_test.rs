//! qmp ブローカー経由で Composite の中身が読めること (issue #170)
//!
//! System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、Composite Process の profile に
//! `composition`、CompositeContainer の profile に `slots` が出ることを確かめる。
//! HTTP は juiz_core の composite_display_test。仕様は docs/spec/composition.md の §8.2。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

const PORT: i64 = 5019;

fn manifest() -> Value {
    let mut m = server_manifest("qmp_composite_test", PORT, 8034, true);
    let obj = m.as_object_mut().unwrap();
    obj.insert(
        "composites".to_owned(),
        jvalue!([
            {
                "type_name": "inc_twice",
                "body": {
                    "process": "increment_process",
                    "args": { "arg1": { "process": "increment_process" } }
                }
            },
            {
                "type_name": "add_twice",
                "containers": { "c": "example_container" },
                "body": {
                    "process": "example_container_increment",
                    "container": "c",
                    "args": { "arg1": { "process": "increment_process", "args": { "arg1": { "expose": "x" } } } }
                }
            }
        ]),
    );
    obj["processes"]
        .as_array_mut()
        .unwrap()
        .push(jvalue!({ "type_name": "inc_twice", "name": "t0" }));
    obj.insert(
        "composite_containers".to_owned(),
        jvalue!([
            { "name": "cc0", "slots": { "c": "c0" },
              "processes": [ { "type_name": "add_twice", "name": "a0" } ] }
        ]),
    );
    m
}

#[test]
fn qmp_composite_profile() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    System::new(manifest())?.setup()?.run_and_do_once(|system| {
        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "qmp",
            broker_name.as_str(),
            true,
        )?;
        let bp = juiz_borrow(&proxy)?;

        let t0: ProcessIdentifier =
            format!("qmp://{broker_name}/process/t0::inc_twice").try_into()?;
        let composition = bp
            .process_profile_full(&t0)?
            .composition
            .expect("composition");
        assert_eq!(composition.body.process, "increment_process");
        assert!(composition.body.args.contains_key("arg1"));
        // 呼び出しも qmp 越しに通る
        assert_eq!(int_of(bp.process_call(&t0, args("arg1", 1))), 3);

        let a0: ProcessIdentifier =
            format!("qmp://{broker_name}/container_process/a0::add_twice:cc0").try_into()?;
        let composition = bp
            .container_process_profile_full(&a0)?
            .composition
            .expect("composition");
        assert_eq!(
            composition.containers.get("c").map(String::as_str),
            Some("example_container")
        );
        assert_eq!(composition.body.container.as_deref(), Some("c"));

        let cc0: ContainerIdentifier =
            format!("qmp://{broker_name}/container/cc0::composite_container").try_into()?;
        let slots = bp.container_profile_full(&cc0)?.slots.expect("slots");
        assert_eq!(
            slots.get("c").map(String::as_str),
            Some("core://core/container/c0::example_container")
        );
        // 普通の Container には slots を出さない
        let c0: ContainerIdentifier =
            format!("qmp://{broker_name}/container/c0::example_container").try_into()?;
        assert!(bp.container_profile_full(&c0)?.slots.is_none());
        Ok(())
    })
}
