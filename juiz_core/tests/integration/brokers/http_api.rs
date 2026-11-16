//! Rust / Python / C++ のプロセス・コンテナを読み込んだ System に、
//! HTTP ブローカー経由の BrokerProxy でアクセスするテスト
//!
//! System 自身の HTTP サーバに CRUDBrokerProxy で繋ぎ、
//! HTTP クライアント → HTTP ルータ → CRUDBroker → CoreBroker の経路を通す。
//!
//! API の確認はブローカー共通のスイート (juiz_broker_suite::api、issue #522)。
//! ここでは HTTP だけの確認 (ブローカーの識別子と住所、http:// の識別子での呼び出し、EC の proxy) を足す。

use crate::brokers::broker_api;

use broker_api::*;
use juiz_core::prelude::*;

const PORT: i64 = 8012;

/// HTTP 経由で得たブローカーの識別子は HTTP ブローカーを指す (issue #35)
fn check_broker_identifiers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let broker_ids = bp.broker_list(false)?;
    for id in broker_ids.iter() {
        let id_struct = IdentifierStruct::try_from(id.clone())?;
        assert_eq!(id_struct.broker_type_name, "http", "{id}");
        let profile = bp.broker_profile(id)?;
        assert_eq!(Into::<Identifier>::into(profile.clone()), *id);
    }
    // バインド後のブローカー名は実際の住所 127.0.0.1:{PORT} になる (issue #55)
    assert!(broker_ids
        .iter()
        .any(|id| id.ends_with(&format!("/broker/127.0.0.1:{PORT}::http"))));
    assert!(broker_ids
        .iter()
        .any(|id| id.ends_with("/broker/local::local")));
    // 見つからないブローカーはエラーになり、HTTP サーバは止まらない
    assert!(bp
        .broker_profile(&format!("http://localhost:{PORT}/broker/nothing::http"))
        .is_err());
    assert!(!bp.broker_list(false)?.is_empty());
    Ok(())
}

/// process/list が返す http:// 形式の識別子をそのまま同じサーバーの
/// /api/process/call に渡しても、自分宛てに再リクエストせずに呼べる (issue #43)
fn check_call_with_own_http_identifier(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    for host in [format!("localhost:{PORT}"), format!("127.0.0.1:{PORT}")] {
        for (name, _type_name) in PROCESSES {
            let id = find_process(&ids, name).to_string();
            assert!(id.starts_with("http://"), "{id}");
            let res = client
                .patch(format!("http://{host}/api/process/call"))
                .query(&[("identifier", id.as_str())])
                .json(&jvalue!({"arg1": 10}))
                .send()?;
            assert!(res.status().is_success(), "{host} {id}: {:?}", res.text());
            assert_eq!(res.json::<Value>()?, jvalue!(11), "{host} {id}");
        }
    }
    Ok(())
}

/// 別の System から EC を操作するときと同じく、CoreWorker::ec_proxy_from_identifier で得た
/// ExecutionContextProxy 経由で HTTP 越しに EC を start / stop できる (issue #50)
fn check_ec_proxy(system: &mut System) -> JuizResult<()> {
    let id = format!("http://localhost:{PORT}/execution_context/timer0::TimerEC");
    let state = |system: &mut System| -> JuizResult<Value> {
        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "http",
            format!("localhost:{PORT}").as_str(),
            false,
        )?;
        let v = juiz_borrow(&proxy)?.ec_get_state(&id)?;
        Ok(v["__value__"].clone())
    };
    let wait_for = |system: &mut System, expected: &str| -> JuizResult<bool> {
        for _ in 0..200 {
            if state(system)? == jvalue!(expected) {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(false)
    };

    // 作成済みの BrokerProxy を create_when_not_found = false でも取り出せる
    let ec = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .ec_proxy_from_identifier(&id)?;
    assert!(wait_for(system, "STOPPED")?);
    juiz_lock(&ec)?.start()?;
    assert!(wait_for(system, "STARTED")?);
    juiz_lock(&ec)?.stop()?;
    assert!(wait_for(system, "STOPPED")?);

    // 存在しない EC の start は HTTP の 500 をエラーとして返す
    let missing = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .ec_proxy_from_identifier(&format!(
            "http://localhost:{PORT}/execution_context/nothing::TimerEC"
        ))?;
    assert!(juiz_lock(&missing)?.start().is_err());
    Ok(())
}

#[test]
fn http_broker_api_with_rust_python_cpp_plugins() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest("http_broker_api_test", PORT))?
        .setup()?
        .run_and_do_once(|system| {
            let broker_name = format!("localhost:{PORT}");
            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "http",
                broker_name.as_str(),
                true,
            )?;
            let mut bp = juiz_write(&proxy)?;
            check_broker_identifiers(&mut *bp)?;
            suite::check_all(
                &mut *bp,
                &MULTI_LANGUAGE,
                &Expect {
                    broker_type_name: "http",
                    broker_name: Some(&broker_name),
                },
            )?;
            check_call_with_own_http_identifier(&mut *bp)?;
            drop(bp);
            check_ec_proxy(system)?;
            Ok(())
        })
}
