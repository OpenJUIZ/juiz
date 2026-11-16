//! Rust / Python / C++ のプロセス・コンテナを読み込んだ System に、
//! ローカルブローカー経由の BrokerProxy でアクセスするテスト
//!
//! ローカルブローカーはメッセンジャー系ブローカー (MessengerBroker / MessengerBrokerProxy) の実装で、
//! MessengerBrokerProxy → チャンネル → MessengerBroker → CRUDBroker → CoreBroker の経路を通す。
//! IPC などのメッセンジャー系ブローカーのクライアント側はすべて MessengerBrokerProxy を使う。
//!
//! API の確認はブローカー共通のスイート (juiz_broker_suite::api、issue #522)。

use crate::brokers::broker_api;

use broker_api::*;
use juiz_core::prelude::*;

const PORT: i64 = 8013;

#[test]
fn local_broker_api_with_rust_python_cpp_plugins() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest("local_broker_api_test", PORT))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system
                .core_broker()
                .lock()?
                .worker()
                .broker_proxy("local", "local", true)?;
            let mut bp = juiz_write(&proxy)?;
            let brokers = bp.broker_list(false)?;
            assert!(
                brokers
                    .iter()
                    .any(|id| id.ends_with("/broker/local::local")),
                "{brokers:?}"
            );
            suite::check_all(
                &mut *bp,
                &MULTI_LANGUAGE,
                &Expect {
                    broker_type_name: "core",
                    broker_name: Some("core"),
                },
            )
        })
}
