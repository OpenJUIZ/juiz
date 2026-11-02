//! HTTP ブローカーの HTTP の版と並行動作のベンチマーク (issue #357、親 #355)
//!
//! CRUDBroker のロック修正 (#356) の後に、次の組み合わせで時間を測る。
//!
//! - クライアントの形
//!   - `h1-conns`: HTTP/1.1。同時に投げる本数と同じ数の接続を張る (reqwest の接続プール)
//!   - `h2c-1conn`: HTTP/2 (h2c prior knowledge)。1 本の接続に全部を多重化する
//!   - `h2c-1conn core://`: `h2c-1conn` と同じだが、識別子をサーバーの中の形 (`core://core/...`) で渡す。
//!     HTTP/2 には Host ヘッダが無く、`http://<住所>/...` の識別子がサーバー自身のものと分からないため
//!     (サーバーが自分の HTTPBrokerProxy で自分を呼び直す)、その影響を外した値
//!   - `h2c-conns core://`: HTTP/2 (h2c prior knowledge)。同時に投げる本数と同じ数の接続を張る (識別子は core://)
//!   - `proxy`: juiz の `HTTPBrokerProxy` を System のストアから取り、同じものを全スレッドで使う
//!     (System の中の ProcessProxy と同じ使い方。`Arc<Mutex<dyn BrokerProxy>>` のロックを取って呼ぶ)
//!   - `blocking-conns`: `reqwest::blocking::Client` をスレッドごとに持つ (proxy のロックが無い場合の目安)
//! - 同時に投げる本数: 1, 4, 16, 64
//! - Process の重さ: すぐ返る (`fast`)、20ms 眠る (`sleep20`)、720p の画像を返す (`img720`)
//!
//! サーバー側の tokio ランタイムのワーカー数は `TOKIO_WORKER_THREADS` で変えられる
//! (HTTP ブローカーのランタイムは `CRUDBrokerHolder` が数を指定せずに作るので、この環境変数が効く)。
//! クライアント側のランタイムはワーカー数を 4 に固定してある。
//!
//! 時間がかかるので `#[ignore]` にしてある。次のように release で実行する。
//!
//! ```sh
//! cargo test --release -p juiz_core --test integration -- http_concurrency_bench --ignored --nocapture --test-threads=1
//! TOKIO_WORKER_THREADS=1 cargo test --release -p juiz_core --test integration -- http_concurrency_bench --ignored --nocapture --test-threads=1
//! ```
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{ImageBuffer, Rgb};

use crate::broker_api::find_process;

const PORT: i64 = 8064;
const CONCURRENCY: [usize; 4] = [1, 4, 16, 64];
const SLEEP: Duration = Duration::from_millis(20);

fn image720() -> &'static DynamicImage {
    static IMG: OnceLock<DynamicImage> = OnceLock::new();
    IMG.get_or_init(|| {
        // PNG がよく縮まないよう、なめらかな模様に小さなノイズを足す
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(1280, 720, |x, y| {
            let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
            h ^= h >> 15;
            h = h.wrapping_mul(0x2C1B_3C6D);
            let n = (h >> 24) as u8 & 0x0f;
            Rgb([
                (x / 5) as u8 ^ n,
                (y / 3) as u8 ^ n,
                ((x + y) / 8) as u8 ^ n,
            ])
        }))
    })
}

fn fast_process(_args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(0).into())
}

fn sleep_process(_args: CapsuleMap) -> JuizResult<Capsule> {
    std::thread::sleep(SLEEP);
    Ok(jvalue!(0).into())
}

fn image_process(_args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(image720().clone().into())
}

#[derive(Clone, Copy)]
struct Load {
    name: &'static str,
    process: &'static str,
    /// 1 本の流れ (同時に投げる 1 本) が続けて投げる回数
    per_stream: usize,
}

const LOADS: [Load; 3] = [
    Load {
        name: "fast",
        process: "bench_fast",
        per_stream: 200,
    },
    Load {
        name: "sleep20",
        process: "bench_sleep",
        per_stream: 10,
    },
    Load {
        name: "img720",
        process: "bench_image",
        per_stream: 4,
    },
];

fn setup(system: &System) -> JuizResult<()> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let factories: [(&str, fn(CapsuleMap) -> JuizResult<Capsule>); 3] = [
        ("bench_fast_t", fast_process),
        ("bench_sleep_t", sleep_process),
        ("bench_image_t", image_process),
    ];
    for (type_name, f) in factories {
        let pf = process_factory_create(ProcessManifest::new(type_name).use_memo(false), f)?;
        worker
            .store_mut()
            .processes
            .register_factory(type_name, pf)?;
    }
    for (type_name, name) in [
        ("bench_fast_t", "bench_fast"),
        ("bench_sleep_t", "bench_sleep"),
        ("bench_image_t", "bench_image"),
    ] {
        worker.create_process_ref(&ProcessManifest::new(type_name).name(name).use_memo(false))?;
    }
    Ok(())
}

fn call_url() -> String {
    format!("http://127.0.0.1:{PORT}/api/process/call")
}

/// 結果: 全部が返るまでの時間と、1 回あたりの平均の往復時間
struct Measured {
    total: Duration,
    mean_latency: Duration,
}

fn summarize(total: Duration, latencies: Vec<Duration>) -> Measured {
    let n = latencies.len().max(1) as u32;
    Measured {
        total,
        mean_latency: latencies.iter().sum::<Duration>() / n,
    }
}

/// 非同期の reqwest で、`concurrency` 本の流れを同時に走らせる
async fn run_async(
    clients: Vec<reqwest::Client>,
    id: String,
    concurrency: usize,
    per_stream: usize,
    expect_image: bool,
) -> Measured {
    let start = Instant::now();
    let mut tasks = Vec::new();
    for i in 0..concurrency {
        let client = clients[i % clients.len()].clone();
        let id = id.clone();
        tasks.push(tokio::spawn(async move {
            let mut lat = Vec::with_capacity(per_stream);
            for _ in 0..per_stream {
                let t = Instant::now();
                let res = client
                    .patch(call_url())
                    .query(&[("identifier", id.as_str())])
                    .json(&jvalue!({}))
                    .send()
                    .await
                    .expect("send");
                assert!(res.status().is_success(), "status {}", res.status());
                let body = res.bytes().await.expect("body");
                if expect_image {
                    assert!(body.len() > 100_000, "image body too small: {}", body.len());
                }
                lat.push(t.elapsed());
            }
            lat
        }));
    }
    let mut all = Vec::new();
    for t in tasks {
        all.extend(t.await.expect("task"));
    }
    summarize(start.elapsed(), all)
}

/// スレッドを `concurrency` 本立て、それぞれが `f` を `per_stream` 回呼ぶ
fn run_threads(
    concurrency: usize,
    per_stream: usize,
    f: Arc<dyn Fn(usize) + Send + Sync>,
) -> Measured {
    let barrier = Arc::new(Barrier::new(concurrency + 1));
    let handles: Vec<_> = (0..concurrency)
        .map(|i| {
            let barrier = barrier.clone();
            let f = f.clone();
            std::thread::spawn(move || {
                let mut lat = Vec::with_capacity(per_stream);
                barrier.wait();
                for _ in 0..per_stream {
                    let t = Instant::now();
                    f(i);
                    lat.push(t.elapsed());
                }
                lat
            })
        })
        .collect();
    barrier.wait();
    let start = Instant::now();
    let mut all = Vec::new();
    for h in handles {
        all.extend(h.join().expect("thread"));
    }
    summarize(start.elapsed(), all)
}

fn print_row(client: &str, load: &Load, concurrency: usize, m: &Measured) {
    let n = (concurrency * load.per_stream) as f64;
    let rps = n / m.total.as_secs_f64();
    println!(
        "| {client} | {} | {concurrency} | {} | {:.1} | {:.2} | {:.0} |",
        load.name,
        concurrency * load.per_stream,
        m.total.as_secs_f64() * 1000.0,
        m.mean_latency.as_secs_f64() * 1000.0,
        rps
    );
}

#[test]
#[ignore]
fn http_concurrency_bench() -> JuizResult<()> {
    crate::common::logger::init();
    let workers =
        std::env::var("TOKIO_WORKER_THREADS").unwrap_or_else(|_| "既定 (コア数)".to_owned());
    println!(
        "server tokio worker threads: {workers}, cpus: {}",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
    System::new(jvalue!({
        "name": "http_concurrency_bench",
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

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()?;
        let h1 = reqwest::Client::builder()
            .http1_only()
            .pool_max_idle_per_host(256)
            .build()?;
        let h2 = reqwest::Client::builder().http2_prior_knowledge().build()?;

        // h2c で本当に HTTP/2 で応答しているかを確かめる
        let version = rt.block_on(async {
            h2.get(format!("http://127.0.0.1:{PORT}/api/system/profile_full"))
                .send()
                .await
                .map(|r| r.version())
        })?;
        println!("h2c prior knowledge response version: {version:?}");
        assert_eq!(version, reqwest::Version::HTTP_2);

        println!("| client | load | concurrency | requests | total ms | mean latency ms | req/s |");
        println!("|---|---|---|---|---|---|---|");
        // JUIZ_HTTP_BENCH_LOADS=fast,sleep20 のように測るものを絞れる
        let only = std::env::var("JUIZ_HTTP_BENCH_LOADS").ok();
        for load in LOADS {
            if let Some(only) = &only {
                if !only.split(',').any(|n| n == load.name) {
                    continue;
                }
            }
            let pid = find_process(&ids, load.process).clone();
            let id = pid.to_string();
            let is_image = load.name == "img720";
            // サーバーの中の識別子 (core://core/...) で呼ぶ場合
            let core_id = format!("core://core/process/{}::{}", pid.name, pid.type_name);
            // 温める
            rt.block_on(run_async(vec![h2.clone()], core_id.clone(), 4, 2, is_image));
            rt.block_on(run_async(vec![h1.clone()], id.clone(), 4, 2, is_image));
            rt.block_on(run_async(vec![h2.clone()], id.clone(), 4, 2, is_image));
            for c in CONCURRENCY {
                let m = rt.block_on(run_async(
                    vec![h1.clone()],
                    id.clone(),
                    c,
                    load.per_stream,
                    is_image,
                ));
                print_row("h1-conns", &load, c, &m);
                let m = rt.block_on(run_async(
                    vec![h2.clone()],
                    id.clone(),
                    c,
                    load.per_stream,
                    is_image,
                ));
                print_row("h2c-1conn", &load, c, &m);
                let h2s: Vec<_> = (0..c)
                    .map(|_| {
                        reqwest::Client::builder()
                            .http2_prior_knowledge()
                            .build()
                            .unwrap()
                    })
                    .collect();
                let m = rt.block_on(run_async(
                    vec![h2.clone()],
                    core_id.clone(),
                    c,
                    load.per_stream,
                    is_image,
                ));
                print_row("h2c-1conn core://", &load, c, &m);
                let m = rt.block_on(run_async(
                    h2s,
                    core_id.clone(),
                    c,
                    load.per_stream,
                    is_image,
                ));
                print_row("h2c-conns core://", &load, c, &m);

                let proxy_c = proxy.clone();
                let pid_c = pid.clone();
                let m = run_threads(
                    c,
                    load.per_stream,
                    Arc::new(move |_| {
                        let r = juiz_lock(&proxy_c)
                            .unwrap()
                            .any_process_call(&pid_c, CapsuleMap::new())
                            .expect("proxy call");
                        drop(r);
                    }),
                );
                print_row("proxy", &load, c, &m);

                let clients: Arc<Vec<reqwest::blocking::Client>> =
                    Arc::new((0..c).map(|_| reqwest::blocking::Client::new()).collect());
                let id_c = id.clone();
                let m = run_threads(
                    c,
                    load.per_stream,
                    Arc::new(move |i| {
                        let res = clients[i]
                            .patch(call_url())
                            .query(&[("identifier", id_c.as_str())])
                            .json(&jvalue!({}))
                            .send()
                            .expect("send");
                        assert!(res.status().is_success());
                        let _ = res.bytes().expect("body");
                    }),
                );
                print_row("blocking-conns", &load, c, &m);
            }
        }
        Ok(())
    })
}

/// 手で調べるため (curl など)、ベンチと同じ Process を置いたサーバーを
/// `JUIZ_HTTP_BENCH_SERVE_SECS` 秒 (既定 60) 動かすだけのもの
#[test]
#[ignore]
fn http_concurrency_bench_serve() -> JuizResult<()> {
    crate::common::logger::init();
    let secs: u64 = std::env::var("JUIZ_HTTP_BENCH_SERVE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    System::new(jvalue!({
        "name": "http_concurrency_bench_serve",
        "option": { "http_broker": { "start": true, "port": PORT } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        setup(system)?;
        println!("serving on {PORT} for {secs}s");
        std::thread::sleep(Duration::from_secs(secs));
        Ok(())
    })
}
