//! 置き場所に入れた C ABI のブローカーを、conf の type_name と ID の頭だけで使う
//! (docs/spec/broker-plugin-abi.md §6.4 の 3・§10、issue #638)
//!
//! 組み込み (register_builtin_cabi_broker) にすると置き場所を見ないので、tests/broker とは別のテストの
//! バイナリにして、何も登録しない。置き場所は JUIZ_BROKER_HOME で一時ディレクトリにする (環境変数は
//! プロセスで共有なので、このバイナリのテストは 1 つだけ)。
//!
//! プラグインは target/debug の cdylib (先に `cargo build -p juiz_tcp_test_broker`。scripts/test.sh はそうしている)。

use std::path::PathBuf;

use juiz_broker_suite::fixture::target_dir;
use juiz_core::env_logger;
use juiz_core::installed::{BrokerPackageManifest, BROKER_HOME_ENV, BROKER_PACKAGE_FILE};
use juiz_core::prelude::*;
use juiz_core::HOST_TARGET;

const SERVER_PORT: i64 = 5446;

fn cdylib_file() -> String {
    format!(
        "{}tcp_test_broker{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

/// `<home>/tcptest/0.1.0/` に、`juiz broker install` が置くのと同じ並びでプラグインを置く
fn install_into(home: &PathBuf) {
    let src = target_dir().join(cdylib_file());
    assert!(src.is_file(), "{} is not built. Run 'cargo build -p juiz_tcp_test_broker' first", src.display());
    let dir = home.join("tcptest").join("0.1.0");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::copy(&src, dir.join("lib").join(cdylib_file())).unwrap();
    let m = BrokerPackageManifest {
        format: 1,
        type_name: "tcptest".into(),
        plugin_name: "juiz_tcp_test_broker".into(),
        version: "0.1.0".into(),
        abi_version: juiz_sdk::broker_cabi::BROKER_ABI_VERSION,
        target: HOST_TARGET.into(),
        library: format!("lib/{}", cdylib_file()),
    };
    std::fs::write(
        dir.join(BROKER_PACKAGE_FILE),
        juiz_sdk::serde_json::to_string_pretty(&m).unwrap(),
    )
    .unwrap();
}

#[test]
fn installed_broker_is_loaded_when_needed() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let home = std::env::temp_dir().join(format!("juiz_installed_broker_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::env::set_var(BROKER_HOME_ENV, &home);

    // 置き場所が空なら、juiz broker install を案内する
    let e = System::new(jvalue!({
        "name": "installed_missing",
        "option": { "http_broker": { "start": false } },
        "brokers": [ { "type_name": "tcptest", "host": "127.0.0.1", "port": SERVER_PORT } ]
    }))?
    .setup()
    .map(|_| ())
    .expect_err("tcptest is not installed");
    let m = format!("{e:#}");
    assert!(m.contains("'juiz broker install tcptest --from <URL>'"), "{m}");

    install_into(&home);
    // conf の brokers の type_name だけで、置き場所から読んで立てる
    System::new(jvalue!({
        "name": "installed_server",
        "option": { "http_broker": { "start": false } },
        "brokers": [ { "type_name": "tcptest", "host": "127.0.0.1", "port": SERVER_PORT } ]
    }))?
    .setup()?
    .run_and_do_once(|server| {
        assert!(server.has_broker_factory("tcptest")?);
        // 別の System から ID の頭 (tcptest://) でつなぐ。BrokerProxy のファクトリも置き場所から読む
        System::new(jvalue!({
            "name": "installed_client",
            "option": { "http_broker": { "start": false } }
        }))?
        .setup()?
        .run_and_do_once(|client| {
            assert!(!client.has_broker_factory("tcptest")?);
            let bp = client
                .core_broker()
                .lock_mut()?
                .worker_mut()
                .broker_proxy("tcptest", &format!("127.0.0.1:{SERVER_PORT}"), true)?;
            let profile = juiz_borrow(&bp)?.system_profile_full()?;
            // 相手 (installed_server) は置き場所の tcptest で待ち受けている
            assert_eq!(profile["identifier"], jvalue!(format!("tcptest://127.0.0.1:{SERVER_PORT}/broker_proxy/core::core")), "{profile}");
            assert!(profile["system_store"]["brokers"].as_array().unwrap().contains(&jvalue!("tcptest")), "{profile}");
            Ok(())
        })?;
        // conf の broker_proxies でも同じ
        System::new(jvalue!({
            "name": "installed_client2",
            "option": { "http_broker": { "start": false } },
            "broker_proxies": [ { "type_name": "tcptest", "name": format!("127.0.0.1:{SERVER_PORT}") } ]
        }))?
        .setup()?
        .run_and_do_once(|client| {
            assert!(client.has_broker_factory("tcptest")?);
            Ok(())
        })
    })?;
    let _ = std::fs::remove_dir_all(&home);
    Ok(())
}
