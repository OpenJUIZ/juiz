//! qmp ブローカー経由で型の登録所を引けること (issue #193)
//!
//! System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、type/list・type/schema を呼ぶ。
//! HTTP は juiz_core の type_check_test。仕様は docs/spec/datatype.md の §19。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_sdk::types::{DataType, RecordField, RecordType, TypeSchema};

const PORT: i64 = 5022;

fn twist() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Twist")
            .field(RecordField::new("linear", DataType::Double))
            .field(RecordField::new("angular", DataType::Double)),
    ))
    .unwrap()
}

fn make_twist(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!({"linear": 1.0, "angular": 0.0}).into())
}

#[test]
fn qmp_type_api() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    let manifest = server_manifest("qmp_type_api_test", PORT, 8039, true);
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        let pf = process_factory_create(
            ProcessManifest::new("make_twist").set_output_schema(Some(twist()))?,
            make_twist,
        )?;
        system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .store_mut()
            .processes
            .register_factory("make_twist", pf)?;

        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "qmp",
            broker_name.as_str(),
            true,
        )?;
        let bp = juiz_lock(&proxy)?;
        let list = bp.type_list()?;
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].identifier, twist().identifier());
        assert_eq!(list[0].used_by, vec!["make_twist"]);
        assert_eq!(bp.type_schema(&twist().identifier())?, twist());
        assert_eq!(bp.type_schema("robot.geometry.Twist")?, twist());
        let e = format!("{:#}", bp.type_schema("robot.geometry.Pose2D").unwrap_err());
        assert!(e.contains("not found"), "{e}");
        Ok(())
    })
}
