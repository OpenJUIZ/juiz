//! qmp ブローカー経由で実行中に Composite を定義できること (issue #171)
//!
//! System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、composite/create・list・profile_full・destroy と
//! composite_container/create を呼ぶ。HTTP は juiz_core の composite_api_test。仕様は docs/spec/composition.md の §9。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_sdk::{manifests::CompositeManifest, process_identifier::ProcessIdentifier};

const PORT: i64 = 5021;

fn composite(v: Value) -> CompositeManifest {
    serde_json::from_value(v).expect("invalid CompositeManifest in test")
}

#[test]
fn qmp_composite_api() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    let manifest = server_manifest("qmp_composite_api_test", PORT, 8038, true);
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "qmp",
            broker_name.as_str(),
            true,
        )?;
        let mut bp = juiz_write(&proxy)?;

        let m = bp.composite_create(&composite(jvalue!({
            "type_name": "inc_twice",
            "body": {
                "process": "increment_process",
                "args": { "arg1": { "process": "increment_process" } }
            }
        })))?;
        assert!(m.composition.is_some());
        assert_eq!(bp.composite_list()?, vec!["inc_twice"]);
        assert_eq!(
            bp.composite_profile_full("inc_twice")?.type_name,
            "inc_twice"
        );

        bp.process_create(&ProcessManifest::new("inc_twice").name("t0"))?;
        let t0: ProcessIdentifier =
            format!("qmp://{broker_name}/process/t0::inc_twice").try_into()?;
        assert_eq!(int_of(bp.process_call(&t0, args("arg1", 1))), 3);

        // 使われている Composite は消せず、理由が返る
        let e = format!("{:#}", bp.composite_destroy("inc_twice").unwrap_err());
        assert!(e.contains("t0"), "{e}");

        bp.composite_create(&composite(jvalue!({
            "type_name": "add_twice",
            "containers": { "c": "example_container" },
            "body": {
                "process": "example_container_increment",
                "container": "c",
                "args": { "arg1": { "process": "inc_twice" } }
            }
        })))?;
        let cc = bp.composite_container_create(&jvalue!({
            "name": "cc0",
            "slots": { "c": "c0" },
            "processes": [ { "type_name": "add_twice", "name": "a0" } ]
        }))?;
        assert_eq!(
            cc.slots.expect("slots").get("c").map(String::as_str),
            Some("core://core/container/c0::example_container")
        );
        let a0: ProcessIdentifier =
            format!("qmp://{broker_name}/container_process/a0::add_twice:cc0").try_into()?;
        let before = int_of(bp.container_process_call(&a0, args("arg1", 1)));
        let after = int_of(bp.container_process_call(&a0, args("arg1", 1)));
        assert_eq!(after - before, 3);

        let cc0: ContainerIdentifier =
            format!("qmp://{broker_name}/container/cc0::composite_container").try_into()?;
        bp.container_destroy(&cc0)?;
        bp.composite_destroy("add_twice")?;
        bp.process_destroy(&t0)?;
        bp.composite_destroy("inc_twice")?;
        assert!(bp.composite_list()?.is_empty());
        Ok(())
    })
}
