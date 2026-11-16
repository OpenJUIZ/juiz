//! Rust / Python / C++ のプロセス・コンテナを読み込んだ System に対して、
//! CoreBroker の BrokerProxy API を一通り呼び出すテスト
//!
//! API の確認はブローカー共通のスイート (juiz_broker_suite::api、issue #522)。
//! ここでは CoreBroker だけで成り立つ確認 (System の名前・件数) を足す。

use crate::brokers::broker_api;

use broker_api::*;
use juiz_core::prelude::*;

/// System の名前と、読み込んだものの件数
fn check_system_contents(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let profile = bp.system_profile()?;
    assert_eq!(obj_get_str(&profile, "name")?, "core");
    assert!(profile.get("core_store").is_some());
    assert!(bp
        .system_filesystem_list(std::path::PathBuf::from("."))?
        .is_array());

    assert_eq!(bp.process_list(false, None)?.len(), 3);
    assert_eq!(bp.container_list(false, None)?.len(), 3);
    assert_eq!(bp.container_process_list(false, None)?.len(), 6);
    assert_eq!(bp.any_process_list(false, None)?.len(), 9);
    assert_eq!(bp.ec_list(false)?.as_array().unwrap().len(), 1);
    assert!(bp.connection_list(false)?.is_empty());
    assert!(bp.topic_list()?.is_empty());
    // core と local
    let brokers = bp.broker_list(false)?;
    assert_eq!(brokers.len(), 2, "{brokers:?}");
    Ok(())
}

#[test]
fn core_broker_api_with_rust_python_cpp_plugins() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest("core_broker_api_test", 8011))?
        .setup()?
        .run_and_do_once(|system| {
            let mut cb = system.core_broker().lock_mut()?;
            check_system_contents(&mut *cb)?;
            suite::check_all(
                &mut *cb,
                &MULTI_LANGUAGE,
                &Expect {
                    broker_type_name: "core",
                    broker_name: Some("core"),
                },
            )
        })
}
