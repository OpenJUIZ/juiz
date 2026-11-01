//! HTTP ブローカーへの同時の呼び出しが並行に走ることを確かめるテスト (issue #356)
//!
//! 以前は HTTP ルータの各ハンドラが `Mutex<CRUDBroker>` のロックを握ったまま Process を実行していたので、
//! 別の Process を別の接続から同時に呼んでも 1 本ずつ処理された。
//!
//! - 眠る Process 2 つを別のスレッドから同時に呼ぶと、合計の時間が 1 つ分に近い

use std::time::{Duration, Instant};

use juiz_core::prelude::*;

use crate::broker_api::find_process;

const PORT: i64 = 8063;
/// 1 回の呼び出しで眠る時間
const SLEEP: Duration = Duration::from_millis(300);

fn sleep_process(_args: CapsuleMap) -> JuizResult<Capsule> {
    std::thread::sleep(SLEEP);
    Ok(jvalue!(0).into())
}

fn setup(system: &System) -> JuizResult<()> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let pf = process_factory_create(
        ProcessManifest::new("http_concurrency_sleep").use_memo(false),
        sleep_process,
    )?;
    worker
        .store_mut()
        .processes
        .register_factory("http_concurrency_sleep", pf)?;
    for name in ["sleep0", "sleep1"] {
        worker.create_process_ref(
            &ProcessManifest::new("http_concurrency_sleep")
                .name(name)
                .use_memo(false),
        )?;
    }
    Ok(())
}

/// identifier の Process を、スレッドごとに別の HTTP クライアント (別の接続) で同時に呼び、
/// 全部が返るまでの時間を返す
fn call_concurrently(identifiers: &[String]) -> JuizResult<Duration> {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(identifiers.len()));
    let start = Instant::now();
    let handles: Vec<_> = identifiers
        .iter()
        .cloned()
        .map(|id| {
            let barrier = barrier.clone();
            std::thread::spawn(move || -> Result<(), String> {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|e| e.to_string())?;
                barrier.wait();
                let res = client
                    .patch(format!("http://127.0.0.1:{PORT}/api/process/call"))
                    .query(&[("identifier", id.as_str())])
                    .json(&jvalue!({}))
                    .send()
                    .map_err(|e| e.to_string())?;
                if !res.status().is_success() {
                    return Err(format!("{id}: {:?}", res.text()));
                }
                Ok(())
            })
        })
        .collect();
    for h in handles {
        h.join().expect("thread panicked").expect("call failed");
    }
    Ok(start.elapsed())
}

#[test]
fn http_broker_runs_calls_to_different_processes_in_parallel() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(jvalue!({
        "name": "http_concurrency_test",
        "option": { "http_broker": { "start": true, "port": PORT } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        setup(system)?;
        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "http",
            format!("localhost:{PORT}").as_str(),
            true,
        )?;
        let ids = juiz_lock(&proxy)?.process_list(false, None)?;
        let sleep0 = find_process(&ids, "sleep0").to_string();
        let sleep1 = find_process(&ids, "sleep1").to_string();

        // 接続を温めておく (最初の呼び出しの準備の時間を測らないため)
        call_concurrently(&[sleep0.clone()])?;

        // 別の Process 2 つ: 並行に走れば 1 つ分 (300ms) に近い。直列なら 600ms 以上かかる
        let different = call_concurrently(&[sleep0.clone(), sleep1.clone()])?;
        println!("2 calls to different processes: {different:?}");
        assert!(
            different < SLEEP * 3 / 2,
            "calls to different processes must run in parallel ({different:?})"
        );

        Ok(())
    })
}
