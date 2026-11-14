//! zenoh ブローカーの共有メモリのテスト (docs/spec/zenoh-broker.md §11)
//!
//! 同じプロセスの中で、System の zenoh ブローカーに ZenohBrokerProxy で繋ぎ、大きな画像を送り合う。
//! 両方が共有メモリを使うとき、片方だけ切ったとき、pool がフレームより小さいとき (TCP に戻る) のどれでも、
//! 中身が変わらずに届くことを確かめる。pool をフレーム 2 つ分にして何度も送り、領域が使い回されることも確かめる。

mod common;

use std::collections::HashMap;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_core::CRUDBrokerProxy;
use zenoh_broker::ZenohBrokerProxy;

const SHM_PORT: i64 = 5253;
const SERVER_OFF_PORT: i64 = 5254;

/// 1920x1080 の RGB (約 6 MB)
const W: u32 = 1920;
const H: u32 = 1080;

fn param(id: &str) -> HashMap<String, String> {
    HashMap::from([("identifier".to_owned(), id.to_owned())])
}

fn server(name: &str, port: i64, server_options: Value) -> JuizResult<System> {
    zenoh_broker::register();
    let mut broker = jvalue!({ "type_name": "zenoh", "name": format!("127.0.0.1:{port}"), "host": "127.0.0.1", "port": port });
    for (k, v) in server_options.as_object().unwrap() {
        broker[k] = v.clone();
    }
    System::new(jvalue!({
        "name": name,
        "option": { "http_broker": { "start": false } },
        "brokers": [ broker ]
    }))?
    .setup()
}

/// 画像を作らせて受け取る (応答が大きい) のと、画像を送って合計を返させる (要求が大きい) のを n 回
fn send_images(proxy: &ZenohBrokerProxy, n: usize) -> JuizResult<()> {
    let img = image_of_format("rgb8", W, H);
    let expected_sum: u64 = img.as_bytes().iter().map(|b| *b as u64).sum();
    for _ in 0..n {
        let args: CapsuleMap = vec![
            ("format", jvalue!("rgb8")),
            ("width", jvalue!(W)),
            ("height", jvalue!(H)),
        ]
        .into();
        let got = proxy.update(
            "process",
            "call",
            args,
            param("core://core/process/make0::zenoh_image_make"),
        )?;
        assert!(got.lock_as_image(|i| i.as_bytes() == img.as_bytes())?);

        let mut args = CapsuleMap::new();
        args.insert("img".to_owned(), img.clone().into());
        let got = proxy.update(
            "process",
            "call",
            args,
            param("core://core/process/sum0::zenoh_image_sum"),
        )?;
        assert_eq!(got.extract_value()?, jvalue!(expected_sum));
    }
    Ok(())
}

#[test]
fn zenoh_shared_memory_combinations() -> JuizResult<()> {
    let _ = env_logger::try_init();
    // サーバーの pool はフレーム (約 6 MB) 2 つ分。何度も送るので、返された領域を使い回さないと足りなくなる
    server(
        "zenoh_shm_test",
        SHM_PORT,
        jvalue!({ "shm_pool_bytes": 13 << 20 }),
    )?
    .run_and_do_once(|system| {
        setup_data_processes(system)?;
        let name = format!("127.0.0.1:{SHM_PORT}");

        // 両方が共有メモリを使う
        let proxy = ZenohBrokerProxy::new(&jvalue!({ "name": name, "shm_pool_bytes": 13 << 20 }))?;
        // feature があっても、pool を mlock できない環境 (RLIMIT_MEMLOCK が小さい) では TCP で送る
        let usable =
            cfg!(feature = "shared-memory") && zenoh_broker::memlock_allows(13 << 20).is_ok();
        assert_eq!(proxy.uses_shared_memory(), usable);
        send_images(&proxy, 10)?;
        drop(proxy);

        // クライアントだけ共有メモリを切る (Zenoh は TCP で送る)
        let proxy = ZenohBrokerProxy::new(&jvalue!({ "name": name, "shared_memory": false }))?;
        assert!(!proxy.uses_shared_memory());
        send_images(&proxy, 2)?;
        drop(proxy);

        // pool がフレームより小さいと、共有メモリを使わずに送る
        let proxy = ZenohBrokerProxy::new(&jvalue!({ "name": name, "shm_pool_bytes": 1 << 20 }))?;
        send_images(&proxy, 2)?;
        Ok(())
    })
}

#[test]
fn zenoh_shared_memory_off_on_server() -> JuizResult<()> {
    let _ = env_logger::try_init();
    server(
        "zenoh_shm_off_test",
        SERVER_OFF_PORT,
        jvalue!({ "shared_memory": false }),
    )?
    .run_and_do_once(|system| {
        setup_data_processes(system)?;
        let proxy =
            ZenohBrokerProxy::new(&jvalue!({ "name": format!("127.0.0.1:{SERVER_OFF_PORT}") }))?;
        send_images(&proxy, 2)?;
        Ok(())
    })
}
