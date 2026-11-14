//! 別プロセスへの画像の受け渡しを zenoh ブローカーで測るベンチ (docs/spec/zenoh-broker.md §10)
//!
//! juiz_core の image_transfer_bench (HTTP・IPC・qmp) と同じ大きさ (720p・4K の RGB) で、
//! (1) call の戻り値で画像を受け取る、(2) 画像を引数で送る、(3) Topic の push で画像を送る
//! (受け手の Process の実行が終わって応答が返るまで) の時間を測る。
//! 別プロセスの juiz は、このテストバイナリ自身を `zenoh_bench_serve` だけ実行するように起動して作る。
//!
//! juiz_core に zenoh を足さない (CI の時間のため、§2.1) ので、ここに置く。時間がかかるので
//! `#[ignore]` にしてある。次のように release で実行する。
//!
//! ```sh
//! cargo test --release -p juiz_zenoh_broker --test zenoh_bench_test -- --ignored --nocapture --test-threads=1
//! ```

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{DynamicImage, Rgb, RgbImage};

const SERVER_PORT: i64 = 5250;
const SERVE_ENV: &str = "JUIZ_ZENOH_BENCH_SERVE";
const READY: &str = "JUIZ_ZENOH_BENCH_READY";
const TOPIC: &str = "bench/zenoh_topic";

const SIZES: [(u32, u32, &str); 2] = [(1280, 720, "720p"), (3840, 2160, "4K")];
const WARMUP: usize = 2;
const ROUNDS: usize = 5;

/// 1 回の時間の平均を ROUNDS 回測って一番短いもの (image_transfer_bench と同じ測り方)
fn time_per_call(width: u32, mut f: impl FnMut() -> JuizResult<()>) -> JuizResult<Duration> {
    let n: usize = if width >= 3840 { 5 } else { 20 };
    for _ in 0..WARMUP {
        f()?;
    }
    let mut best = Duration::MAX;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        for _ in 0..n {
            f()?;
        }
        best = best.min(start.elapsed() / n as u32);
    }
    Ok(best)
}

/// カメラ画像に近い、なめらかな模様に小さなノイズを足した画像
fn camera_like_image(width: u32, height: u32) -> DynamicImage {
    let mut state: u32 = 12345;
    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = (state >> 28) as u8;
        Rgb([
            ((x * 255 / width) as u8).wrapping_add(noise),
            ((y * 255 / height) as u8).wrapping_add(noise),
            (((x + y) % 256) as u8) ^ noise,
        ])
    }))
}

fn cached_image(width: u32, height: u32) -> DynamicImage {
    static CACHE: OnceLock<std::sync::Mutex<Vec<DynamicImage>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let mut cache = cache.lock().unwrap();
    if let Some(img) = cache.iter().find(|i| i.width() == width && i.height() == height) {
        return img.clone();
    }
    let img = camera_like_image(width, height);
    cache.push(img.clone());
    img
}

fn bench_source(args: CapsuleMap) -> JuizResult<Capsule> {
    let w = args.get("width")?.lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    let h = args.get("height")?.lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    Ok(cached_image(w, h).into())
}

fn bench_sink(args: CapsuleMap) -> JuizResult<Capsule> {
    let len = args.get("img")?.lock_as_image(|img| img.as_bytes().len())?;
    Ok(jvalue!(len).into())
}

fn serve(port: i64) -> JuizResult<()> {
    zenoh_broker::register();
    System::new(jvalue!({
        "name": "zenoh_bench_server",
        "option": { "http_broker": { "start": false } },
        "brokers": [
            { "type_name": "zenoh", "name": format!("127.0.0.1:{port}"), "host": "127.0.0.1", "port": port }
        ]
    }))?
    .setup()?
    .run_and_do_once(|system| {
        {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            let source = ProcessManifest::new("bench_source")
                .use_memo(false)
                .add_int_arg("width", "width", 1)
                .add_int_arg("height", "height", 1);
            let sink = ProcessManifest::new("bench_sink")
                .use_memo(false)
                .add_image_arg("img", "image");
            for (m, f) in [
                (source, bench_source as juiz_sdk::processes::ProcessBodyFunctionType),
                (sink, bench_sink),
            ] {
                let type_name = m.type_name.clone();
                let pf = process_factory_create(m, f)?;
                worker.store_mut().processes.register_factory(&type_name, pf)?;
            }
            worker.create_process_ref(&ProcessManifest::new("bench_source").name("source0").use_memo(false))?;
            worker.create_process_ref(&ProcessManifest::new("bench_sink").name("sink0").use_memo(false))?;
            let topic_sink = worker
                .create_process_ref(&ProcessManifest::new("bench_sink").name("topic_sink0").use_memo(false))?;
            worker.process_subscribe_topic(topic_sink, &"img".to_owned(), TopicManifest::new(TOPIC))?;
        }
        println!("{READY}");
        // 親が子プロセスを止めるまで待つ
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    })
}

/// 子プロセスとして起動されたときだけ、サーバーとして動く
#[test]
fn zenoh_bench_serve() -> JuizResult<()> {
    match std::env::var(SERVE_ENV) {
        Ok(port) => serve(port.parse()?),
        Err(_) => Ok(()),
    }
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_server() -> Server {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["zenoh_bench_serve", "--exact", "--nocapture", "--test-threads=1"])
        .env(SERVE_ENV, SERVER_PORT.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start the bench server");
    let stdout = child.stdout.take().unwrap();
    let server = Server(child);
    for line in BufReader::new(stdout).lines() {
        if line.expect("reading the bench server output failed").contains(READY) {
            return server;
        }
    }
    panic!("the bench server exited before it was ready");
}

fn ms(d: Duration) -> String {
    format!("{:8.2} ms", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore]
fn zenoh_image_transfer_bench() -> JuizResult<()> {
    let _ = juiz_core::env_logger::try_init();
    let _server = spawn_server();
    let proxy = zenoh_broker::ZenohBrokerProxy::new(&jvalue!({ "name": format!("127.0.0.1:{SERVER_PORT}") }))?;
    use juiz_core::CRUDBrokerProxy;
    let param = |id: &str| std::collections::HashMap::from([("identifier".to_owned(), id.to_owned())]);
    let source = "core://core/process/source0::bench_source";
    let sink = "core://core/process/sink0::bench_sink";

    println!("zenoh (TCP, other process), best of {ROUNDS} rounds:");
    for (w, h, label) in SIZES {
        let img = cached_image(w, h);
        let len = img.as_bytes().len();
        let ret = time_per_call(w, || {
            let args: CapsuleMap = vec![("width", jvalue!(w)), ("height", jvalue!(h))].into();
            let got = proxy.update("process", "call", args, param(source))?;
            assert_eq!(got.lock_as_image(|i| i.as_bytes().len())?, len);
            Ok(())
        })?;
        let arg = time_per_call(w, || {
            let mut args = CapsuleMap::new();
            args.insert("img".to_owned(), img.clone().into());
            let got = proxy.update("process", "call", args, param(sink))?;
            assert_eq!(got.extract_value()?, jvalue!(len));
            Ok(())
        })?;
        let topic = time_per_call(w, || {
            let mut map = CapsuleMap::new();
            map.insert("input".to_owned(), img.clone().into());
            let mut p = std::collections::HashMap::new();
            p.insert("topic_name".to_owned(), TOPIC.to_owned());
            proxy.update("topic", "push", map, p)?;
            Ok(())
        })?;
        println!(
            "  {label:>5}: call return {}  call argument {}  topic push {}",
            ms(ret),
            ms(arg),
            ms(topic)
        );
    }
    Ok(())
}
