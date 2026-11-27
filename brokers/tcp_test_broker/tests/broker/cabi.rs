//! C ABI のブローカーだけの確認 (docs/spec/broker-plugin-abi.md §5〜§7・§11)
//!
//! - 待ち受けに失敗したら、System の起動のエラーに理由が載る (§5.3)
//! - 繋がらない、待ち時間切れ、max_frame_bytes を超えるフレーム、サーバーでの Process の失敗と panic (§7)
//! - HTTP の処理 (tokio) の中から C ABI の BrokerProxy を呼ぶ (§5.4)
//! - conf の `abi: rust` では C ABI の入口を使わない (§11)
//!
//! ブローカーは組み込み (register_builtin_cabi_broker) で使う。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::common::*;
use juiz_core::cabi::CabiBrokerProxy;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_core::CRUDBrokerProxy;

const BUSY_PORT: i64 = 5440;
const UNUSED_PORT: i64 = 5441;
const TIMEOUT_PORT: i64 = 5442;
const FORWARD_SUB_PORT: i64 = 5443;
const FORWARD_MASTER_PORT: i64 = 5444;
const FORWARD_HTTP_PORT: i64 = 8444;
const SMALL_LIMIT_PORT: i64 = 5445;

fn setup() {
    let _ = env_logger::try_init();
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(TCPTEST.register);
}

fn param(id: &str) -> HashMap<String, String> {
    HashMap::from([("identifier".to_owned(), id.to_owned())])
}

fn proxy(manifest: Value) -> JuizResult<CabiBrokerProxy> {
    CabiBrokerProxy::from_entry_fn(tcp_test_broker::broker_factory_cabi, &manifest)
}

/// 待ち受けに失敗すると、待ち時間切れでなく、その理由で System の起動が失敗する
#[test]
fn listen_failure_is_reported() -> JuizResult<()> {
    setup();
    let _busy = std::net::TcpListener::bind(("127.0.0.1", BUSY_PORT as u16)).unwrap();
    let start = Instant::now();
    let r = System::new(jvalue!({
        "name": "tcptest_listen_failure",
        "option": { "http_broker": { "start": false } },
        "brokers": [ { "type_name": "tcptest", "host": "127.0.0.1", "port": BUSY_PORT } ]
    }))?
    .setup()
    .and_then(|mut s| s.run_and_do_once(|_| Ok(())));
    let e = r.expect_err("the port is in use");
    let m = format!("{e:#}");
    assert!(m.contains("can not listen on 127.0.0.1:5440"), "{m}");
    assert!(m.contains("broker_factory_cabi.server_start"), "{m}");
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    Ok(())
}

#[test]
fn can_not_connect() -> JuizResult<()> {
    setup();
    let e = proxy(jvalue!({ "name": format!("127.0.0.1:{UNUSED_PORT}") })).err().expect("nothing listens");
    let m = format!("{e:#}");
    assert!(m.contains("broker_factory_cabi.client_connect"), "{m}");
    assert!(m.starts_with(&format!("tcptest://127.0.0.1:{UNUSED_PORT}: can not connect")), "{m}");
    Ok(())
}

fn slow(args: CapsuleMap) -> JuizResult<Capsule> {
    let ms = args.get("ms")?.lock_as_value(|v| v.as_i64().unwrap_or(0))?;
    if ms < 0 {
        panic!("slow got a negative ms");
    }
    std::thread::sleep(Duration::from_millis(ms as u64));
    Ok(jvalue!(ms).into())
}

/// timeout_ms までに応答が無ければエラーになり、その後も同じ BrokerProxy で呼べる。
/// 送るフレームが max_frame_bytes を超えるときは送らずにエラーになる。
/// サーバーの Process の panic はエラー応答になる (§7.2)
#[test]
fn timeout_frame_limit_and_panic() -> JuizResult<()> {
    setup();
    System::new(jvalue!({
        "name": "tcptest_timeout_test",
        "option": { "http_broker": { "start": false } },
        "brokers": [ { "type_name": "tcptest", "host": "127.0.0.1", "port": TIMEOUT_PORT } ]
    }))?
    .setup()?
    .run_and_do_once(|system| {
        {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            let pm = ProcessManifest::new("tcptest_slow").use_memo(false).add_int_arg("ms", "ms", 0);
            let pf = process_factory_create(pm, slow)?;
            worker.store_mut().processes.register_factory("tcptest_slow", pf)?;
            worker.create_process_ref(&ProcessManifest::new("tcptest_slow").name("slow0").use_memo(false))?;
        }
        let p = proxy(jvalue!({
            "name": format!("127.0.0.1:{TIMEOUT_PORT}"),
            "timeout_ms": 300,
            "max_frame_bytes": 100_000
        }))?;
        let id = "core://core/process/slow0::tcptest_slow";
        let call = |ms: i64| p.update("process", "call", args("ms", ms), param(id));

        assert_eq!(call(0)?.extract_value()?, jvalue!(0));
        let start = Instant::now();
        let e = call(1500).unwrap_err();
        assert!(format!("{e:#}").contains("no response within 300 ms"), "{e:#}");
        assert!(start.elapsed() < Duration::from_millis(1400), "{:?}", start.elapsed());
        // 遅れて届いた応答は捨てられ、次の要求に混ざらない
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(call(10)?.extract_value()?, jvalue!(10));

        // 100 KB を超える要求は送らない
        let big: Vec<i64> = (0..50_000).collect();
        let e = p
            .update("process", "call", vec![("ms", jvalue!(big.clone()))].into(), param(id))
            .unwrap_err();
        assert!(format!("{e:#}").contains("the request frame is too large"), "{e:#}");
        assert_eq!(call(1)?.extract_value()?, jvalue!(1));

        // サーバーの上限を超える要求は、サーバーがエラー応答を返す。その後も同じ BrokerProxy で呼べる
        // (System は同じ種類のブローカーを 1 つしか持たないので、上限の小さなサーバーは別の System にする)
        System::new(jvalue!({
            "name": "tcptest_small_limit",
            "option": { "http_broker": { "start": false } },
            "brokers": [ { "type_name": "tcptest", "host": "127.0.0.1", "port": SMALL_LIMIT_PORT, "max_frame_bytes": 1000 } ]
        }))?
        .setup()?
        .run_and_do_once(|_| {
            let small = proxy(jvalue!({ "name": format!("127.0.0.1:{SMALL_LIMIT_PORT}") }))?;
            let e = small
                .read("process", "list", HashMap::from([("pad".to_owned(), "x".repeat(2000))]))
                .unwrap_err();
            assert!(format!("{e:#}").contains("too large"), "{e:#}");
            assert!(small.read("process", "list", HashMap::new()).is_ok());
            Ok(())
        })?;

        // Process の panic はエラー応答になり、サーバーは動き続ける
        let e = call(-1).unwrap_err();
        assert!(format!("{e:#}").contains("slow got a negative ms"), "{e:#}");
        assert_eq!(call(3)?.extract_value()?, jvalue!(3));

        // 同じ BrokerProxy で並行に呼べる
        std::thread::scope(|s| {
            let hs: Vec<_> = (0..8).map(|i| s.spawn(move || call(20 + i))).collect();
            for (i, h) in hs.into_iter().enumerate() {
                assert_eq!(h.join().unwrap().unwrap().extract_value().unwrap(), jvalue!(20 + i as i64));
            }
        });
        Ok(())
    })
}

/// HTTP で受けた要求を、tcptest の副系へ転送する。HTTP の処理 (tokio) の中から C ABI の BrokerProxy を呼ぶ
#[test]
fn forward_from_http() -> JuizResult<()> {
    setup();
    let mut sub = TCPTEST.server_manifest("tcptest_forward_sub", FORWARD_SUB_PORT, 8443, false);
    sub["namespace"] = jvalue!("robot1");
    System::new(sub)?.setup()?.run_and_do_once(|_sub| {
        let mut master = TCPTEST.client_manifest("tcptest_forward_master", false);
        master["option"] = jvalue!({ "http_broker": { "start": true, "port": FORWARD_HTTP_PORT } });
        master["brokers"] = jvalue!([
            { "type_name": "tcptest", "name": format!("127.0.0.1:{FORWARD_MASTER_PORT}"),
              "host": "127.0.0.1", "port": FORWARD_MASTER_PORT }
        ]);
        master["subsystems"] = jvalue!([
            { "type_name": "tcptest", "name": format!("127.0.0.1:{FORWARD_SUB_PORT}") }
        ]);
        System::new(master)?.setup()?.run_and_do_once(|master| {
            let ids = master.core_broker().lock()?.process_list(true, None)?;
            let inc0: String = ids
                .iter()
                .find(|id| id.broker_type_name == "tcptest" && id.name == "inc0")
                .unwrap_or_else(|| panic!("tcptest inc0 not listed: {ids:?}"))
                .clone()
                .into();
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

/// conf の `abi: rust` では C ABI の入口を使わない。tcp_test_broker は Rust ABI の入口を持たないので読めない (§11)
#[test]
fn abi_rust_does_not_use_cabi_entry() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let path = target_dir().to_string_lossy().into_owned();
    let r = System::new(jvalue!({
        "name": "tcptest_abi_rust",
        "option": { "http_broker": { "start": false } },
        "plugins": { "broker_factories": { "tcp_test_broker": { "path": path, "abi": "rust" } } }
    }))?
    .setup();
    let m = format!("{:#}", r.err().expect("the plugin has no Rust ABI entry"));
    assert!(m.contains("broker_factory"), "{m}");
    // abi の知らない値はエラー
    let r = System::new(jvalue!({
        "name": "tcptest_abi_bad",
        "option": { "http_broker": { "start": false } },
        "plugins": { "broker_factories": { "tcp_test_broker": { "path": path, "abi": "go" } } }
    }))?
    .setup();
    let m = format!("{:#}", r.err().expect("abi: go is invalid"));
    assert!(m.contains("'abi'"), "{m}");
    Ok(())
}
