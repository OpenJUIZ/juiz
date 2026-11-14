//! zenoh ブローカーのエラーと待ち時間のテスト (docs/spec/zenoh-broker.md §6、§10)
//!
//! - 繋がらないとき、相手が juiz でないとき、待ち時間切れ、max_frame_bytes を超えるフレーム
//! - サーバーを再起動した後も、同じ BrokerProxy で呼べる
//! - HTTP の処理から zenoh の BrokerProxy を呼ぶ (HTTP で受けた要求を zenoh の副系へ転送する、§5.2)
//!
//! zenoh は組み込み (zenoh_broker::register) で使う。別プロセスのサーバーは juiz コマンドに
//! プラグイン (libzenoh_broker) を読ませる。どちらも ../../target/debug にビルドされている前提。

mod common;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_core::CRUDBrokerProxy;
use zenoh::Wait;
use zenoh_broker::ZenohBrokerProxy;

const UNUSED_PORT: i64 = 5240;
const NOT_JUIZ_PORT: i64 = 5241;
const TIMEOUT_PORT: i64 = 5242;
const RESTART_PORT: i64 = 5243;
const FORWARD_SUB_PORT: i64 = 5244;
const FORWARD_MASTER_PORT: i64 = 5245;
const FORWARD_HTTP_PORT: i64 = 8245;

fn setup() {
    let _ = env_logger::try_init();
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(zenoh_broker::register);
}

fn param(id: &str) -> HashMap<String, String> {
    HashMap::from([("identifier".to_owned(), id.to_owned())])
}

#[test]
fn zenoh_can_not_connect() {
    setup();
    let start = Instant::now();
    let e = ZenohBrokerProxy::new(&jvalue!({ "name": format!("127.0.0.1:{UNUSED_PORT}") }))
        .err()
        .expect("connecting to a closed port must fail");
    let message = format!("{e:#}");
    assert!(message.contains("can not connect"), "{message}");
    assert!(
        message.contains(&format!("tcp/127.0.0.1:{UNUSED_PORT}")),
        "{message}"
    );
    // connect/timeout_ms (5 秒) で諦める
    assert!(start.elapsed() < Duration::from_secs(15), "{:?}", start.elapsed());
}

/// juiz のブローカーでない Zenoh (queryable が無い) に繋ぐと、応答が無いことがすぐ分かる
#[test]
fn zenoh_peer_without_juiz() -> JuizResult<()> {
    setup();
    let mut config = zenoh::Config::default();
    for (k, v) in [
        ("mode", "\"peer\"".to_owned()),
        ("listen/endpoints", format!("[\"tcp/127.0.0.1:{NOT_JUIZ_PORT}\"]")),
        ("scouting/multicast/enabled", "false".to_owned()),
    ] {
        config.insert_json5(k, &v).unwrap();
    }
    let _peer = zenoh::open(config).wait().unwrap();
    let proxy = ZenohBrokerProxy::new(&jvalue!({ "name": format!("127.0.0.1:{NOT_JUIZ_PORT}") }))?;
    let start = Instant::now();
    let e = proxy.read("system", "uuid", HashMap::new()).unwrap_err();
    assert!(
        format!("{e:#}").contains("no juiz zenoh broker answered"),
        "{e:#}"
    );
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    Ok(())
}

fn slow(args: CapsuleMap) -> JuizResult<Capsule> {
    let ms = args.get("ms")?.lock_as_value(|v| v.as_u64().unwrap_or(0))?;
    std::thread::sleep(Duration::from_millis(ms));
    Ok(jvalue!(ms).into())
}

/// timeout_ms までに応答が無ければエラーになり、その後も同じ BrokerProxy で呼べる。
/// 送るフレームが max_frame_bytes を超えるときは送らずにエラーになる
#[test]
fn zenoh_timeout_and_frame_limit() -> JuizResult<()> {
    setup();
    let broker_name = format!("127.0.0.1:{TIMEOUT_PORT}");
    System::new(jvalue!({
        "name": "zenoh_timeout_test",
        "option": { "http_broker": { "start": false } },
        "brokers": [
            { "type_name": "zenoh", "host": "127.0.0.1", "port": TIMEOUT_PORT }
        ]
    }))?
    .setup()?
    .run_and_do_once(|system| {
        {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            let pm = ProcessManifest::new("zenoh_slow")
                .use_memo(false)
                .add_int_arg("ms", "ms", 0);
            let pf = process_factory_create(pm, slow)?;
            worker.store_mut().processes.register_factory("zenoh_slow", pf)?;
            worker.create_process_ref(&ProcessManifest::new("zenoh_slow").name("slow0").use_memo(false))?;
        }
        let proxy = ZenohBrokerProxy::new(&jvalue!({
            "name": broker_name,
            "timeout_ms": 300,
            "max_frame_bytes": 100_000
        }))?;
        let id = "core://core/process/slow0::zenoh_slow";
        let call = |ms: i64| proxy.update("process", "call", args("ms", ms), param(id));

        assert_eq!(call(0)?.extract_value()?, jvalue!(0));
        let start = Instant::now();
        let e = call(1500).unwrap_err();
        assert!(
            format!("{e:#}").contains("no response within 300 ms"),
            "{e:#}"
        );
        assert!(start.elapsed() < Duration::from_millis(1400), "{:?}", start.elapsed());
        // 遅れて届いた応答は捨てられ、次の要求に混ざらない
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(call(10)?.extract_value()?, jvalue!(10));

        // 100 KB を超える要求は送らない
        let big: Vec<i64> = (0..50_000).collect();
        let start = Instant::now();
        let e = proxy
            .update("process", "call", vec![("ms", jvalue!(big))].into(), param(id))
            .unwrap_err();
        assert!(format!("{e:#}").contains("too large"), "{e:#}");
        assert!(start.elapsed() < Duration::from_millis(300));
        assert_eq!(call(1)?.extract_value()?, jvalue!(1));
        Ok(())
    })
}

/// サーバー (別プロセス) を止めて同じポートで起動し直すと、同じ BrokerProxy でまた呼べる
#[test]
fn zenoh_server_restart() -> JuizResult<()> {
    setup();
    let broker_name = format!("127.0.0.1:{RESTART_PORT}");
    let connect = |server: &mut ServerProcess| -> ZenohBrokerProxy {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = server.0.try_wait().unwrap() {
                panic!("juiz server exited ({status})");
            }
            match ZenohBrokerProxy::new(&jvalue!({ "name": broker_name })) {
                Ok(p) => break p,
                Err(e) => {
                    assert!(Instant::now() < deadline, "can not connect to server: {e:?}");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    };
    let list = |proxy: &ZenohBrokerProxy| -> JuizResult<usize> {
        let v = proxy.read("process", "list", HashMap::new())?.extract_value()?;
        Ok(v.as_array().map(|a| a.len()).unwrap_or(0))
    };

    let mut server = spawn_server(RESTART_PORT, 8243, true);
    let proxy = connect(&mut server);
    assert_eq!(list(&proxy)?, 2);
    drop(server);
    // 止まっている間はエラーになる (待ち続けない)
    let start = Instant::now();
    assert!(list(&proxy).is_err());
    assert!(start.elapsed() < Duration::from_secs(13), "{:?}", start.elapsed());

    let mut server = spawn_server(RESTART_PORT, 8243, true);
    // サーバーの起動を、別の BrokerProxy で待つ
    drop(connect(&mut server));
    // 元の BrokerProxy は Zenoh が繋ぎ直す
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match list(&proxy) {
            Ok(n) => {
                assert_eq!(n, 2);
                break;
            }
            Err(e) => {
                assert!(Instant::now() < deadline, "not reconnected: {e:#}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Ok(())
}

/// HTTP で受けた要求を、zenoh の副系へ転送する。HTTP の処理 (tokio) の中から zenoh の BrokerProxy を呼ぶ
#[test]
fn zenoh_forward_from_http() -> JuizResult<()> {
    setup();
    let mut sub = server_manifest("zenoh_forward_sub", FORWARD_SUB_PORT, 8246, false);
    sub["namespace"] = jvalue!("robot1");
    System::new(sub)?.setup()?.run_and_do_once(|_sub| {
        let mut master = client_manifest("zenoh_forward_master", false);
        master["option"] = jvalue!({ "http_broker": { "start": true, "port": FORWARD_HTTP_PORT } });
        master["brokers"] = jvalue!([
            { "type_name": "zenoh", "name": format!("127.0.0.1:{FORWARD_MASTER_PORT}"),
              "host": "127.0.0.1", "port": FORWARD_MASTER_PORT }
        ]);
        master["subsystems"] = jvalue!([
            { "type_name": "zenoh", "name": format!("127.0.0.1:{FORWARD_SUB_PORT}") }
        ]);
        System::new(master)?.setup()?.run_and_do_once(|master| {
            let ids = master.core_broker().lock()?.process_list(true, None)?;
            let inc0: String = ids
                .iter()
                .find(|id| id.broker_type_name == "zenoh" && id.name == "inc0")
                .unwrap_or_else(|| panic!("zenoh inc0 not listed: {ids:?}"))
                .clone()
                .into();
            // HTTP の BrokerProxy は identifier を core:// に直してしまうので、HTTP の要求を直接送る
            let client = reqwest::blocking::Client::new();
            let r = client
                .patch(format!("http://127.0.0.1:{FORWARD_HTTP_PORT}/api/process/call"))
                .query(&[("identifier", inc0.as_str())])
                .json(&jvalue!({ "arg1": 41 }))
                .send()?;
            assert!(r.status().is_success(), "{r:?}");
            assert_eq!(r.json::<Value>()?, jvalue!(42));
            Ok(())
        })
    })
}
