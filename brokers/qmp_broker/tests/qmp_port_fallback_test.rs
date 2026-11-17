//! qmp ブローカーの待ち受けポートが使用中のときの扱い (docs/spec/app-package.md §16、issue #392)
//!
//! - 相手が juiz 以外 (UDP を握っているだけ): `next` なら次の番号、`error` なら起動を止める
//! - 相手が qmp の juiz: `adopt_master: false` なら番号をずらすだけ。`error` なら相手が juiz だと分かる文で止まる

use std::net::UdpSocket;

use juiz_core::env_logger;
use juiz_core::prelude::*;

const OTHER_PORT: i64 = 5060;
const OTHER_ERROR_PORT: i64 = 5063;
const JUIZ_PORT: i64 = 5066;

fn setup() {
    let _ = env_logger::try_init();
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(qmp_broker::register);
}

fn conf(name: &str, port: i64, extra: Value) -> Value {
    let mut broker = jvalue!({ "type_name": "qmp", "name": format!("127.0.0.1:{port}"), "host": "127.0.0.1", "port": port });
    if let (Some(b), Some(e)) = (broker.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
    jvalue!({
        "name": name,
        "option": { "http_broker": { "start": false } },
        "brokers": [broker],
    })
}

fn qmp_broker_names(system: &System) -> JuizResult<Vec<String>> {
    Ok(system
        .core_broker()
        .lock()?
        .broker_list(false)?
        .into_iter()
        .filter(|id| id.ends_with("::qmp"))
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
fn qmp_port_in_use_by_other_program() -> JuizResult<()> {
    setup();
    let _occupied = UdpSocket::bind(format!("127.0.0.1:{OTHER_PORT}")).unwrap();
    System::new(conf("qmp_port_fallback_other", OTHER_PORT, jvalue!({})))?
        .setup()?
        .run_and_do_once(|system| {
            let names = qmp_broker_names(system)?;
            assert!(
                names
                    .iter()
                    .any(|id| id.ends_with(&format!("127.0.0.1:{}::qmp", OTHER_PORT + 1))),
                "{names:?}"
            );
            assert!(!has_mastersystem(system)?);
            Ok(())
        })
}

#[test]
fn qmp_port_in_use_by_other_program_with_error() -> JuizResult<()> {
    setup();
    let _occupied = UdpSocket::bind(format!("127.0.0.1:{OTHER_ERROR_PORT}")).unwrap();
    let e = setup_error(conf(
        "qmp_port_fallback_other_error",
        OTHER_ERROR_PORT,
        jvalue!({ "port_fallback": "error" }),
    ));
    assert!(
        e.contains(&format!(
            "port {OTHER_ERROR_PORT} for the qmp broker is in use (by another program)"
        )),
        "{e}"
    );
    Ok(())
}

#[test]
fn qmp_port_in_use_by_juiz() -> JuizResult<()> {
    setup();
    System::new(conf("qmp_port_fallback_a", JUIZ_PORT, jvalue!({})))?
        .setup()?
        .run_and_do_once(|_a| {
            System::new(conf(
                "qmp_port_fallback_b",
                JUIZ_PORT,
                jvalue!({ "adopt_master": false }),
            ))?
            .setup()?
            .run_and_do_once(|b| {
                let names = qmp_broker_names(b)?;
                assert!(
                    names
                        .iter()
                        .any(|id| id.ends_with(&format!("127.0.0.1:{}::qmp", JUIZ_PORT + 1))),
                    "{names:?}"
                );
                assert!(!has_mastersystem(b)?);
                Ok(())
            })?;

            let e = setup_error(conf(
                "qmp_port_fallback_c",
                JUIZ_PORT,
                jvalue!({ "port_fallback": "error" }),
            ));
            assert!(
                e.contains(&format!(
                    "port {JUIZ_PORT} for the qmp broker is in use (by a juiz)"
                )),
                "{e}"
            );

            // 既定: 相手の qmp の juiz を mastersystem にしてつなぐ
            System::new(conf("qmp_port_fallback_d", JUIZ_PORT, jvalue!({})))?
                .setup()?
                .run_and_do_once(|d| {
                    let master =
                        d.core_broker().lock()?.worker().manifest()["mastersystem"].clone();
                    assert_eq!(master["broker_type"], "qmp");
                    assert_eq!(master["broker_name"], format!("127.0.0.1:{JUIZ_PORT}"));
                    Ok(())
                })?;
            Ok(())
        })
}
