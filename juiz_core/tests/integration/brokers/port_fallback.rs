//! HTTP ブローカーの待ち受けポートが使用中のときの扱い (docs/spec/app-package.md §16、issue #392)
//!
//! - 相手が juiz 以外: `next` なら次の番号で立ち、相手を親にしない。`error` なら起動を止める
//! - 相手が juiz: `adopt_master: false` なら番号をずらすだけ。既定 (true) なら相手を mastersystem にする
//! - 使用中以外の bind の失敗は、3 秒待たずに理由付きのエラーになる
//! - 知らない値はエラー

use std::net::TcpListener;
use std::time::{Duration, Instant};

use juiz_core::prelude::*;

/// このテストで使う番号は 8600 番台 (他のテストと重ならない)
const OTHER_PORT: i64 = 8600;
const OTHER_ERROR_PORT: i64 = 8603;
const JUIZ_PORT: i64 = 8605;
const BAD_ADDR_PORT: i64 = 8610;

fn http_conf(name: &str, port: i64, extra: Value) -> Value {
    let mut http = jvalue!({ "start": true, "port": port });
    if let (Some(h), Some(e)) = (http.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            h.insert(k.clone(), v.clone());
        }
    }
    jvalue!({ "name": name, "option": { "http_broker": http } })
}

/// juiz 以外のプログラムのふり: 待ち受けるだけで何も返さない
fn occupy(port: i64) -> TcpListener {
    TcpListener::bind(format!("0.0.0.0:{port}")).expect("cannot occupy the port for the test")
}

fn http_broker_names(system: &System) -> JuizResult<Vec<String>> {
    Ok(system
        .core_broker()
        .lock()?
        .broker_list(false)?
        .into_iter()
        .filter(|id| id.ends_with("::http"))
        .collect())
}

fn has_mastersystem(system: &System) -> JuizResult<bool> {
    Ok(system
        .core_broker()
        .lock()?
        .worker()
        .manifest()
        .get("mastersystem")
        .map(|v| !v.is_null())
        .unwrap_or(false))
}

fn setup_error(conf: Value) -> String {
    match System::new(conf).and_then(|s| s.setup()) {
        Ok(_) => panic!("setup must fail"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn port_in_use_by_other_program() -> JuizResult<()> {
    crate::common::logger::init();
    let _occupied = occupy(OTHER_PORT);
    // next (既定): 次の番号で立ち、juiz ではない相手を親にしない
    System::new(http_conf("port_fallback_other", OTHER_PORT, jvalue!({})))?
        .setup()?
        .run_and_do_once(|system| {
            let names = http_broker_names(system)?;
            assert!(
                names
                    .iter()
                    .any(|id| id.contains(&format!("127.0.0.1:{}::http", OTHER_PORT + 1))),
                "{names:?}"
            );
            assert!(!has_mastersystem(system)?);
            Ok(())
        })?;
    Ok(())
}

#[test]
fn port_in_use_by_other_program_with_error() -> JuizResult<()> {
    crate::common::logger::init();
    let _occupied = occupy(OTHER_ERROR_PORT);
    let e = setup_error(http_conf(
        "port_fallback_other_error",
        OTHER_ERROR_PORT,
        jvalue!({ "port_fallback": "error" }),
    ));
    assert!(
        e.contains(&format!(
            "port {OTHER_ERROR_PORT} for the HTTP broker is in use (by another program). Use another port, or remove port_fallback: error / --no-port-fallback."
        )),
        "{e}"
    );
    Ok(())
}

#[test]
fn port_in_use_by_juiz() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(http_conf("port_fallback_juiz_a", JUIZ_PORT, jvalue!({})))?
        .setup()?
        .run_and_do_once(|_a| {
            // adopt_master: false: 番号をずらすだけで親にしない
            System::new(http_conf(
                "port_fallback_juiz_b",
                JUIZ_PORT,
                jvalue!({ "adopt_master": false }),
            ))?
            .setup()?
            .run_and_do_once(|b| {
                let names = http_broker_names(b)?;
                assert!(
                    names
                        .iter()
                        .any(|id| id.contains(&format!("127.0.0.1:{}::http", JUIZ_PORT + 1))),
                    "{names:?}"
                );
                assert!(!has_mastersystem(b)?);
                Ok(())
            })?;

            // port_fallback: error: 相手が juiz だと分かる文で止まる
            let e = setup_error(http_conf(
                "port_fallback_juiz_c",
                JUIZ_PORT,
                jvalue!({ "port_fallback": "error" }),
            ));
            assert!(
                e.contains(&format!(
                    "port {JUIZ_PORT} for the HTTP broker is in use (by a juiz)"
                )),
                "{e}"
            );

            // 既定: 相手を mastersystem にする
            System::new(http_conf("port_fallback_juiz_d", JUIZ_PORT, jvalue!({})))?
                .setup()?
                .run_and_do_once(|d| {
                    assert!(has_mastersystem(d)?);
                    let master =
                        d.core_broker().lock()?.worker().manifest()["mastersystem"].clone();
                    assert_eq!(master["broker_name"], format!("127.0.0.1:{JUIZ_PORT}"));
                    Ok(())
                })?;
            Ok(())
        })?;
    Ok(())
}

#[test]
fn bind_error_is_reported_without_timeout() -> JuizResult<()> {
    crate::common::logger::init();
    // この機械に無いアドレス (TEST-NET-3) では bind が AddrNotAvailable で失敗する
    let start = Instant::now();
    let e = setup_error(jvalue!({
        "name": "port_fallback_bad_addr",
        "option": { "http_broker": { "start": false } },
        "brokers": [{ "type_name": "http", "name": "bad_addr", "host": "203.0.113.1", "port": BAD_ADDR_PORT }],
    }));
    assert!(
        e.contains(&format!(
            "cannot start the HTTP broker on 203.0.113.1:{BAD_ADDR_PORT}: "
        )),
        "{e}"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    Ok(())
}

#[test]
fn unknown_policy_value_is_an_error() -> JuizResult<()> {
    crate::common::logger::init();
    let e = setup_error(http_conf(
        "port_fallback_unknown",
        BAD_ADDR_PORT + 1,
        jvalue!({ "port_fallback": "skip" }),
    ));
    assert!(
        e.contains("port_fallback must be 'next' or 'error', but 'skip'"),
        "{e}"
    );
    let e = setup_error(jvalue!({
        "name": "port_fallback_unknown_brokers",
        "option": { "http_broker": { "start": false } },
        "brokers": [{ "type_name": "http", "name": "unknown", "port": BAD_ADDR_PORT + 2, "adopt_master": "no" }],
    }));
    assert!(e.contains("adopt_master must be true or false"), "{e}");
    Ok(())
}
