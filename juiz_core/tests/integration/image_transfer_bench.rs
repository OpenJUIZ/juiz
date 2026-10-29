//! 別プロセスへの画像の受け渡しのベンチマーク (issue #230)
//!
//! 同じマシンの別プロセスに、1280x720 / 1920x1080 / 3840x2160 の RGB 画像を渡す時間を測り、
//! 大きなデータの高速化 (#229) の基準にする。別プロセスの juiz は、このテストバイナリ自身を
//! `serve_for_image_transfer_bench` だけ実行するように起動して作る (HTTP ブローカーで待ち受ける)。
//!
//! 測るもの:
//! - 同じプロセス内: 画像の複製だけ、CoreBroker の process_call、local ブローカーの BrokerProxy
//! - HTTP (別プロセス): (1) call の戻り値で画像を受け取る (HttpBrokerProxy)、
//!   (2) 画像を引数で送る (multipart の PUT を reqwest で直接。HttpBrokerProxy も同じ形で送る)
//! - IPC (別プロセス、#233): (1) call の戻り値、(2) 引数で送る。画像は生バイトのフレーム (large-data.md §3)
//! - qmp (別プロセス、#234): (1) call の戻り値、(2) 引数で送る。IPC と同じ生バイトのフレームを QUIC で送る
//! - 内訳: PNG の圧縮と展開を同じ画像で別に測り、残りを「転送ほか」とする
//! - 比較: 同じ大きさの生バイトを Unix ソケットで受け取るだけの時間
//! - Topic: 画像を Topic に流せるかを確かめる (#233 で通るようになった)
//! - Topic (別プロセス、#235): IPC で subscribe している別プロセスへ、フレーム (A) と共有メモリ (B) で
//!   画像を publish する。publish が返るまでの時間と、受け手の Process に届くまでの時間を測る
//!
//! 画像はカメラ画像に近づけるため、なめらかな模様に小さなノイズを足したものにする
//! (単純な模様だと PNG がよく縮み、実際より速く見える)。画像は大きさごとに 1 度だけ作り、
//! Process は複製して返す (画像を作る時間を測りたいものから外す)。
//!
//! 時間がかかるので `#[ignore]` にしてある。次のように release で実行する。
//!
//! ```sh
//! cargo test --release -p juiz_core --test integration -- image_transfer_bench --ignored --nocapture --test-threads=1
//! ```
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use juiz_sdk::process_identifier::ProcessIdentifier;

/// 別プロセスの juiz が待ち受けるポート
const SERVER_PORT: i64 = 8050;
/// 別プロセスの juiz の qmp ブローカーのポート
const SERVER_QMP_PORT: i64 = 8051;
/// 別プロセスの juiz の IPC ブローカーの名前
const SERVER_IPC_NAME: &str = "image_transfer_bench";
/// 別プロセスの juiz として動くときに、このテストに渡す環境変数 (値はポート)
const SERVE_ENV: &str = "JUIZ_IMAGE_TRANSFER_BENCH_SERVE";
const READY: &str = "JUIZ_IMAGE_TRANSFER_BENCH_READY";

const SIZES: [(u32, u32, &str); 3] = [
    (1280, 720, "720p"),
    (1920, 1080, "1080p"),
    (3840, 2160, "4K"),
];
/// 別プロセスへの Topic。A はフレームで、B は共有メモリで送る (送り手の conf の topics: で決める)
const TOPIC_A: &str = "bench/topic_a";
const TOPIC_B: &str = "bench/topic_b";
/// B の枠の大きさ。4K RGB (24,883,200 バイト) が入る
const TOPIC_B_SLOT_BYTES: u64 = 24 << 20;
const WARMUP: usize = 2;
const ROUNDS: usize = 5;

/// 1 回の時間の平均を `ROUNDS` 回測って一番短いもの。4K の PNG は 1 回が長いので、回数は大きさで変える
fn time_per_call(width: u32, mut f: impl FnMut(i64) -> JuizResult<()>) -> JuizResult<Duration> {
    let n: usize = match width {
        w if w >= 3840 => 2,
        w if w >= 1920 => 5,
        _ => 10,
    };
    for i in 0..WARMUP {
        f(i as i64)?;
    }
    let mut best = Duration::MAX;
    for round in 0..ROUNDS {
        let start = Instant::now();
        for i in 0..n {
            f((WARMUP + round * n + i) as i64)?;
        }
        best = best.min(start.elapsed() / n as u32);
    }
    Ok(best)
}

fn ms(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64() * 1000.0)
}

/// なめらかな模様に、seed で変わる ±8 程度のノイズを足した画像
fn camera_like_image(width: u32, height: u32, seed: i64) -> DynamicImage {
    let seed = seed as u32;
    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        let mut h = x
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(y.wrapping_mul(0x85EB_CA77))
            .wrapping_add(seed.wrapping_mul(0xC2B2_AE3D));
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        let noise = |shift: u32| ((h >> shift) & 0x0F) as i32 - 8;
        let base = [
            (x * 255 / width) as i32,
            (y * 255 / height) as i32,
            (((x + y) / 4 + seed) % 256) as i32,
        ];
        Rgb([
            (base[0] + noise(0)).clamp(0, 255) as u8,
            (base[1] + noise(4)).clamp(0, 255) as u8,
            (base[2] + noise(8)).clamp(0, 255) as u8,
        ])
    }))
}

fn mean_of(img: &DynamicImage) -> f64 {
    let bytes = img.as_bytes();
    bytes.iter().map(|b| *b as u64).sum::<u64>() as f64 / bytes.len() as f64
}

/// HTTP ブローカーの応答と同じ書き方で PNG にする (http_router の capsule_ptr_to_response)
fn encode_png(img: &DynamicImage) -> JuizResult<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    img.write_to(&mut buffer, ImageFormat::Png)?;
    Ok(buffer.into_inner())
}

/// HTTP ブローカーの受け取りと同じ読み方で PNG を展開する (http_broker_proxy の image_png_response_to_capsule_ptr)
fn decode_png(png: &[u8]) -> JuizResult<DynamicImage> {
    Ok(juiz_sdk::image::load_from_memory(png)?)
}

// ---- 別プロセスと同じプロセスの両方で使う Process ----

/// 大きさごとに 1 度だけ作った画像。画像を作る時間 (カメラなら撮る時間) を測りたいものから外すため、
/// Process は毎回これを複製して返す
fn cached_image(width: u32, height: u32) -> DynamicImage {
    static CACHE: OnceLock<Mutex<HashMap<(u32, u32), DynamicImage>>> = OnceLock::new();
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry((width, height))
        .or_insert_with(|| camera_like_image(width, height, 1))
        .clone()
}

/// 指定の大きさの画像を返す (seed は呼び出しごとに変えるだけで、画像は同じ)
fn bench_source(args: CapsuleMap) -> JuizResult<Capsule> {
    let int = |name: &str| -> JuizResult<i64> {
        args.get(name)?.lock_as_value(|v| v.as_i64().unwrap_or(0))
    };
    Ok(cached_image(int("width")? as u32, int("height")? as u32).into())
}

/// 受け取った画像の画素の平均 (受け取れたことを確かめる)
fn bench_sink(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(args.get("img")?.lock_as_image(mean_of)?).into())
}

/// 別プロセスの Topic で届いた画像の数 (A, B)
static TOPIC_RECEIVED: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

fn bench_topic_sink_a(args: CapsuleMap) -> JuizResult<Capsule> {
    args.get("img")?.lock_as_image(|img| std::hint::black_box(img.width()))?;
    TOPIC_RECEIVED[0].fetch_add(1, Ordering::SeqCst);
    Ok(jvalue!(0).into())
}

fn bench_topic_sink_b(args: CapsuleMap) -> JuizResult<Capsule> {
    args.get("img")?.lock_as_image(|img| std::hint::black_box(img.width()))?;
    TOPIC_RECEIVED[1].fetch_add(1, Ordering::SeqCst);
    Ok(jvalue!(0).into())
}

fn bench_topic_count(_args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!([
        TOPIC_RECEIVED[0].load(Ordering::SeqCst),
        TOPIC_RECEIVED[1].load(Ordering::SeqCst)
    ])
    .into())
}

/// 別プロセスの juiz だけが持つ、Topic を subscribe する Process
fn setup_server_topic_processes(system: &System) -> JuizResult<()> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let factories: Vec<(ProcessManifest, juiz_sdk::processes::ProcessBodyFunctionType)> = vec![
        (
            ProcessManifest::new("bench_topic_sink_a")
                .use_memo(false)
                .add_image_arg("img", "input image"),
            bench_topic_sink_a,
        ),
        (
            ProcessManifest::new("bench_topic_sink_b")
                .use_memo(false)
                .add_image_arg("img", "input image"),
            bench_topic_sink_b,
        ),
        (ProcessManifest::new("bench_topic_count").use_memo(false), bench_topic_count),
    ];
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        worker.store_mut().processes.register_factory(&type_name, pf)?;
    }
    for (type_name, name, topic) in [
        ("bench_topic_sink_a", "topic_sink_a", TOPIC_A),
        ("bench_topic_sink_b", "topic_sink_b", TOPIC_B),
    ] {
        let p = worker.create_process_ref(&ProcessManifest::new(type_name).name(name).use_memo(false))?;
        worker.process_subscribe_topic(p, &"img".to_owned(), TopicManifest::new(topic))?;
    }
    worker.create_process_ref(
        &ProcessManifest::new("bench_topic_count")
            .name("topic_count")
            .use_memo(false),
    )?;
    Ok(())
}

fn setup_bench_processes(system: &System) -> JuizResult<(ProcessPtr, ProcessPtr)> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let factories: Vec<(
        ProcessManifest,
        juiz_sdk::processes::ProcessBodyFunctionType,
    )> = vec![
        (
            ProcessManifest::new("bench_source")
                .use_memo(false)
                .add_int_arg("width", "width", 1280)
                .add_int_arg("height", "height", 720)
                .add_int_arg("seed", "seed", 0),
            bench_source,
        ),
        (
            // Topic で届いたかを get_output で確かめるため、出力を覚える
            ProcessManifest::new("bench_sink")
                .use_memo(true)
                .add_image_arg("img", "input image"),
            bench_sink,
        ),
    ];
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        worker
            .store_mut()
            .processes
            .register_factory(&type_name, pf)?;
    }
    let source = worker.create_process_ref(
        &ProcessManifest::new("bench_source")
            .name("source0")
            .use_memo(false),
    )?;
    let sink = worker.create_process_ref(
        &ProcessManifest::new("bench_sink")
            .name("sink0")
            .use_memo(true),
    )?;
    Ok((source, sink))
}

fn source_args(width: u32, height: u32, seed: i64) -> CapsuleMap {
    vec![
        ("width", jvalue!(width)),
        ("height", jvalue!(height)),
        ("seed", jvalue!(seed)),
    ]
    .into()
}

/// 別プロセスの juiz として動く (ベンチの本体から起動される)。環境変数が無ければ何もしない
#[test]
#[ignore]
fn serve_for_image_transfer_bench() -> JuizResult<()> {
    let Ok(port) = std::env::var(SERVE_ENV) else {
        return Ok(());
    };
    crate::common::logger::init();
    let port: i64 = port.parse()?;
    qmp_broker::register();
    System::new(jvalue!({
        "name": "image_transfer_bench_server",
        "option": { "http_broker": { "start": true, "port": port } },
        "brokers": [
            { "type_name": "ipc", "name": SERVER_IPC_NAME },
            { "type_name": "qmp", "name": format!("127.0.0.1:{SERVER_QMP_PORT}"), "host": "127.0.0.1", "port": SERVER_QMP_PORT },
        ],
    }))?
    .setup()?
    .run_and_do_once(|system| {
        setup_bench_processes(system)?;
        setup_server_topic_processes(system)?;
        println!("{READY}");
        std::io::stdout().flush()?;
        // 親が stdin を閉じる (終わるか落ちる) まで待ち受ける
        let mut buf = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut buf);
        Ok(())
    })
}

/// 別プロセスの juiz。drop で止める
struct Server {
    child: Child,
}

impl Server {
    fn start(port: i64) -> JuizResult<Server> {
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "image_transfer_bench::serve_for_image_transfer_bench",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(SERVE_ENV, port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next() {
                // libtest の「test ... 」の行の後ろに続けて出るので、行の中を探す
                Some(Ok(line)) if line.contains(READY) => break,
                Some(Ok(_)) => continue,
                _ => {
                    let _ = child.kill();
                    panic!("the server process exited before it got ready");
                }
            }
        }
        // 残りの出力は読み捨てる (パイプが詰まらないように)
        std::thread::spawn(move || for _ in lines {});
        Ok(Server { child })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// multipart/form-data で画像 (PNG) を 1 つ送る本文
fn multipart_png_body(boundary: &str, field: &str, png: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(png.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{field}.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// 同じ大きさの生バイトを Unix ソケットで受け取るだけの時間 (送り手は別スレッド)。
/// 1 バイトの要求に、画像の大きさの応答を返す
#[cfg(unix)]
fn unix_socket_raw(width: u32, height: u32) -> JuizResult<Duration> {
    use std::os::unix::net::UnixStream;
    let len = (width * height * 3) as usize;
    let (mut client, mut server) = UnixStream::pair()?;
    let payload = camera_like_image(width, height, 0).into_bytes();
    let sender = std::thread::spawn(move || {
        let mut req = [0u8; 1];
        while server.read_exact(&mut req).is_ok() {
            if server.write_all(&payload).is_err() {
                break;
            }
        }
    });
    let t = time_per_call(width, |_| {
        client.write_all(&[1])?;
        let mut buf = vec![0u8; len];
        client.read_exact(&mut buf)?;
        let img = RgbImage::from_raw(width, height, buf).expect("size must match");
        std::hint::black_box(img);
        Ok(())
    })?;
    drop(client);
    let _ = sender.join();
    Ok(t)
}

#[cfg(not(unix))]
fn unix_socket_raw(_width: u32, _height: u32) -> JuizResult<Duration> {
    Ok(Duration::ZERO)
}

/// 別プロセスへの Topic (A, B) に publish する Process を作り、受け手に subscribe するかを問い合わせる
fn setup_topic_sources(system: &System) -> JuizResult<[ProcessPtr; 2]> {
    let mut cb = system.core_broker().lock_mut()?;
    let mut sources = Vec::new();
    for (name, topic) in [("topic_source_a", TOPIC_A), ("topic_source_b", TOPIC_B)] {
        let worker = cb.worker_mut();
        let p = worker.create_process_ref(&ProcessManifest::new("bench_source").name(name).use_memo(false))?;
        worker.process_publish_topic(p.clone(), TopicManifest::new(topic))?;
        sources.push(p);
    }
    // setup の後で publish を足したので、setup_topic_synchronization と同じ問い合わせをする
    let uuid = uuid::Uuid::parse_str(cb.system_uuid()?.as_str().unwrap())?;
    let bp: &mut dyn BrokerProxy = &mut *cb;
    for topic in [TOPIC_A, TOPIC_B] {
        assert_eq!(bp.topic_request_subscribe(topic, Some(uuid))?, jvalue!({"subscribe": true}));
    }
    Ok([sources.remove(0), sources.remove(0)])
}

/// 画像を出す Process を Topic に publish し、受け取る Process を subscribe して、届くかを確かめる
fn topic_result(system: &System, source: &ProcessPtr, sink: &ProcessPtr) -> JuizResult<String> {
    {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        worker.process_publish_topic(source.clone(), TopicManifest::new("bench_image_topic"))?;
        worker.process_subscribe_topic(
            sink.clone(),
            &"img".to_owned(),
            TopicManifest::new("bench_image_topic"),
        )?;
    }
    source.lock_mut()?.p_apply("width", jvalue!(64).into())?;
    source.lock_mut()?.p_apply("height", jvalue!(48).into())?;
    let r = source.lock()?.execute();
    let delivered = sink.lock()?.get_output().is_value().unwrap_or(false);
    Ok(match (r, delivered) {
        (Ok(_), true) => "届く".to_owned(),
        (Ok(_), _) => "publish はエラーにならないが、subscribe 側に届かない".to_owned(),
        (Err(e), _) => format!("送れない ({})", e.root_cause()),
    })
}

struct Row {
    label: &'static str,
    png_bytes: usize,
    raw_bytes: usize,
    make: Duration,
    core_call: Duration,
    local_call: Duration,
    encode: Duration,
    decode: Duration,
    http_return: Duration,
    http_arg: Result<Duration, String>,
    ipc_return: Duration,
    ipc_arg: Duration,
    qmp_return: Duration,
    qmp_arg: Duration,
    unix_raw: Duration,
    /// Topic (別プロセス) の [A, B]
    topic_push: [Duration; 2],
    topic_delivered: [Duration; 2],
    /// B の受け手の内訳 (#270)
    topic_b_receive: ReceiveBreakdown,
}

/// 共有メモリの Topic の受け手の 1 フレームあたりの内訳 (受け手の Topic の profile の数え上げの差から求める)
#[derive(Default, Clone, Copy)]
struct ReceiveBreakdown {
    copy: Duration,
    page_faults: f64,
    push: Duration,
    reused: f64,
}

/// profile の中から Topic の shared_memory の数え上げを探す (入れ子のどこにあってもよい)
fn find_shm_stats(v: &Value, topic: &str) -> Option<Value> {
    match v {
        Value::Object(obj) => {
            if let Some(Value::Array(topics)) = obj.get("topics") {
                for t in topics {
                    let id = t.get("identifier").and_then(|n| n.as_str()).unwrap_or("");
                    if id.contains(&format!("/{topic}::")) {
                        return t.get("shared_memory").cloned();
                    }
                }
            }
            obj.values().find_map(|c| find_shm_stats(c, topic))
        }
        Value::Array(a) => a.iter().find_map(|c| find_shm_stats(c, topic)),
        _ => None,
    }
}

impl ReceiveBreakdown {
    fn between(before: &Value, after: &Value) -> Self {
        let diff = |key: &str| {
            let get = |v: &Value| v.get(key).and_then(|x| x.as_u64()).unwrap_or(0);
            get(after).saturating_sub(get(before))
        };
        let n = diff("received").max(1);
        ReceiveBreakdown {
            copy: Duration::from_nanos(diff("receive_copy_ns") / n),
            page_faults: diff("receive_page_faults") as f64 / n as f64,
            push: Duration::from_nanos(diff("receive_push_ns") / n),
            reused: diff("reused") as f64 / n as f64,
        }
    }
}

#[test]
#[ignore]
fn bench_image_transfer() -> JuizResult<()> {
    crate::common::logger::init();
    let server = Server::start(SERVER_PORT)?;
    let mut rows = Vec::new();
    let mut topic = String::new();
    qmp_broker::register();
    System::new(jvalue!({
        "name": "image_transfer_bench_client",
        "option": { "http_broker": { "start": false } },
        // 別プロセスの juiz を IPC のサブシステムにし、Topic を IPC で流す
        "brokers": [ { "type_name": "ipc", "name": format!("{SERVER_IPC_NAME}_client") } ],
        "subsystems": [ { "type_name": "ipc", "name": SERVER_IPC_NAME } ],
        "topics": [
            { "name": TOPIC_B, "shared_memory": { "slot_bytes": TOPIC_B_SLOT_BYTES } },
        ],
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let (source, sink) = setup_bench_processes(system)?;

        let host = format!("127.0.0.1:{SERVER_PORT}");
        let http = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("http", &host, true)?;
        let local = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("local", "local", true)?;
        let find = |ids: &[ProcessIdentifier], name: &str| -> ProcessIdentifier {
            ids.iter()
                .find(|id| id.name == name)
                .unwrap_or_else(|| panic!("{name} not found in {ids:?}"))
                .clone()
        };
        let http_ids = juiz_lock(&http)?.process_list(false, None)?;
        let remote_source = find(&http_ids, "source0");
        let remote_sink = find(&http_ids, "sink0");
        let ipc = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("ipc", SERVER_IPC_NAME, true)?;
        let ipc_ids = juiz_lock(&ipc)?.process_list(false, None)?;
        let ipc_source = find(&ipc_ids, "source0");
        let ipc_sink = find(&ipc_ids, "sink0");
        let qmp = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("qmp", &format!("127.0.0.1:{SERVER_QMP_PORT}"), true)?;
        let qmp_ids = juiz_lock(&qmp)?.process_list(false, None)?;
        let qmp_source = find(&qmp_ids, "source0");
        let qmp_sink = find(&qmp_ids, "sink0");
        let topic_count = find(&ipc_ids, "topic_count");
        let topic_sources = setup_topic_sources(system)?;
        let local_ids = juiz_lock(&local)?.process_list(false, None)?;
        let local_source = find(&local_ids, "source0");
        let core_source = source.identifier();

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        let boundary = "juizImageTransferBenchBoundary";
        let put_url = format!("http://{host}/api/process/call");
        let remote_sink_id = remote_sink.to_string();

        for (width, height, label) in SIZES {
            let image = cached_image(width, height);
            let png = encode_png(&image)?;
            let expected = mean_of(&image);

            let make = time_per_call(width, |_| {
                std::hint::black_box(cached_image(width, height));
                Ok(())
            })?;
            let core_call = time_per_call(width, |seed| {
                let out = system
                    .core_broker()
                    .lock()?
                    .process_call(&core_source, source_args(width, height, seed))?;
                assert!(out.is_image()?);
                Ok(())
            })?;
            let local_call = time_per_call(width, |seed| {
                let out = juiz_lock(&local)?
                    .process_call(&local_source, source_args(width, height, seed))?;
                assert!(out.is_image()?);
                Ok(())
            })?;
            let encode = time_per_call(width, |_| {
                std::hint::black_box(encode_png(&image)?);
                Ok(())
            })?;
            let decode = time_per_call(width, |_| {
                std::hint::black_box(decode_png(&png)?);
                Ok(())
            })?;

            // (1) call の戻り値: 別プロセスが PNG にして、こちらで展開する
            let http_return = time_per_call(width, |seed| {
                let out = juiz_lock(&http)?
                    .process_call(&remote_source, source_args(width, height, seed))?;
                out.lock_as_image(|img| assert_eq!(img.width(), width))?;
                Ok(())
            })?;
            let returned = juiz_lock(&http)?
                .process_call(&remote_source, source_args(width, height, 1))?
                .lock_as_image(mean_of)?;
            assert_eq!(
                returned, expected,
                "{label}: PNG で画素が変わってはいけない"
            );

            // (2) 引数で送る: こちらで PNG にして multipart で送り、別プロセスが展開する
            let send_arg = |img: &DynamicImage| -> JuizResult<f64> {
                let body = multipart_png_body(boundary, "img", &encode_png(img)?);
                let res = client
                    .put(&put_url)
                    .query(&[("identifier", remote_sink_id.as_str())])
                    .header(
                        "Content-Type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(body)
                    .send()?;
                assert!(res.status().is_success(), "{:?}", res.text());
                Ok(res
                    .json::<Value>()?
                    .as_f64()
                    .expect("mean must be a number"))
            };
            // 送れなかったときは記録して続ける (#233 の前は、PNG が 2 MB を超えると axum の本文の上限に当たっていた)
            let http_arg = match send_arg(&image) {
                Ok(received) => {
                    // 平均は JSON で返るので、最後の桁は丸められる
                    assert!(
                        (received - expected).abs() < 1e-9,
                        "{label}: 引数の画像が届く ({received} != {expected})"
                    );
                    Ok(time_per_call(width, |_| send_arg(&image).map(|_| ()))?)
                }
                Err(e) => Err(format!("{}", e.root_cause())),
            };

            // IPC: 画像は PNG にせず、生バイトのフレームで送る
            let ipc_return = time_per_call(width, |seed| {
                let out = juiz_lock(&ipc)?
                    .process_call(&ipc_source, source_args(width, height, seed))?;
                out.lock_as_image(|img| assert_eq!(img.width(), width))?;
                Ok(())
            })?;
            let returned = juiz_lock(&ipc)?
                .process_call(&ipc_source, source_args(width, height, 1))?
                .lock_as_image(mean_of)?;
            assert_eq!(returned, expected, "{label}: IPC で画素が変わってはいけない");
            let ipc_send = |img: &DynamicImage| -> JuizResult<f64> {
                let mut args = CapsuleMap::new();
                args.insert("img".to_owned(), img.clone().into());
                let out = juiz_lock(&ipc)?.process_call(&ipc_sink, args)?;
                Ok(out.extract_value()?.as_f64().expect("mean must be a number"))
            };
            assert!((ipc_send(&image)? - expected).abs() < 1e-9, "{label}: IPC の引数の画像が届く");
            // 引数の画像の複製 (args に入れるため) も含む。HTTP の (2) も PNG にする前の画像を使う
            let ipc_arg = time_per_call(width, |_| ipc_send(&image).map(|_| ()))?;

            // qmp: IPC と同じフレームを QUIC (ループバック) で送る
            let qmp_return = time_per_call(width, |seed| {
                let out = juiz_lock(&qmp)?
                    .process_call(&qmp_source, source_args(width, height, seed))?;
                out.lock_as_image(|img| assert_eq!(img.width(), width))?;
                Ok(())
            })?;
            let returned = juiz_lock(&qmp)?
                .process_call(&qmp_source, source_args(width, height, 1))?
                .lock_as_image(mean_of)?;
            assert_eq!(returned, expected, "{label}: qmp で画素が変わってはいけない");
            let qmp_send = |img: &DynamicImage| -> JuizResult<f64> {
                let mut args = CapsuleMap::new();
                args.insert("img".to_owned(), img.clone().into());
                let out = juiz_lock(&qmp)?.process_call(&qmp_sink, args)?;
                Ok(out.extract_value()?.as_f64().expect("mean must be a number"))
            };
            assert!((qmp_send(&image)? - expected).abs() < 1e-9, "{label}: qmp の引数の画像が届く");
            let qmp_arg = time_per_call(width, |_| qmp_send(&image).map(|_| ()))?;

            let unix_raw = unix_socket_raw(width, height)?;

            // Topic (別プロセス): publish が返るまでと、受け手の Process に届くまで
            let received = |i: usize| -> JuizResult<u64> {
                let v = juiz_lock(&ipc)?
                    .process_call(&topic_count, CapsuleMap::new())?
                    .extract_value()?;
                Ok(v[i].as_u64().expect("count must be a number"))
            };
            let mut topic_push = [Duration::ZERO; 2];
            let mut topic_delivered = [Duration::ZERO; 2];
            let mut topic_b_receive = ReceiveBreakdown::default();
            let remote_shm_stats = || -> JuizResult<Value> {
                let profile = juiz_lock(&ipc)?.system_profile_full()?;
                Ok(find_shm_stats(&profile, TOPIC_B).unwrap_or(Value::Null))
            };
            for (i, source) in topic_sources.iter().enumerate() {
                let publish = |seed: i64| -> JuizResult<()> {
                    let mut p = source.lock_mut()?;
                    p.p_apply("width", jvalue!(width).into())?;
                    p.p_apply("height", jvalue!(height).into())?;
                    p.p_apply("seed", jvalue!(seed).into())?;
                    drop(p);
                    source.lock()?.execute()?;
                    Ok(())
                };
                let wait_for = |count: u64| -> JuizResult<()> {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    // 問い合わせ続けると受け手の CPU とロックを取り合うので、少し間を空ける
                    while received(i)? < count {
                        assert!(Instant::now() < deadline, "{label}: the topic frame is not delivered");
                        std::thread::sleep(Duration::from_micros(100));
                    }
                    Ok(())
                };
                let stats_before = remote_shm_stats()?;
                topic_delivered[i] = time_per_call(width, |seed| {
                    let before = received(i)?;
                    publish(seed)?;
                    wait_for(before + 1)
                })?;
                if i == 1 {
                    topic_b_receive = ReceiveBreakdown::between(&stats_before, &remote_shm_stats()?);
                }
                // B は受け手を待たないので、続けて publish すると受け手が古いフレームを捨てる。間を空けて測る
                let mut total = Duration::ZERO;
                let n = 5;
                for seed in 0..n {
                    let before = received(i)?;
                    let start = Instant::now();
                    publish(seed)?;
                    total += start.elapsed();
                    wait_for(before + 1)?;
                }
                topic_push[i] = total / n as u32;
            }

            rows.push(Row {
                label,
                png_bytes: png.len(),
                raw_bytes: image.as_bytes().len(),
                make,
                core_call,
                local_call,
                encode,
                decode,
                http_return,
                http_arg,
                ipc_return,
                ipc_arg,
                qmp_return,
                qmp_arg,
                unix_raw,
                topic_push,
                topic_delivered,
                topic_b_receive,
            });
        }

        topic = topic_result(system, &source, &sink)?;
        Ok(())
    })?;
    drop(server);

    let header = |cols: &[&str]| {
        println!("| {} |", cols.join(" | "));
        println!("|{}", "---|".repeat(cols.len()));
    };
    println!("\n## 別プロセスへの画像の受け渡し (RGB, {ROUNDS} 回中の最短の 1 回あたり [ms])");
    println!("\n### 大きさ");
    header(&["", "生 [MB]", "PNG [MB]", "PNG / 生"]);
    for r in rows.iter() {
        println!(
            "| {} | {:.2} | {:.2} | {:.0}% |",
            r.label,
            r.raw_bytes as f64 / 1e6,
            r.png_bytes as f64 / 1e6,
            r.png_bytes as f64 / r.raw_bytes as f64 * 100.0
        );
    }
    println!("\n### 同じプロセス内 (直列化なし)");
    header(&[
        "",
        "画像の複製だけ",
        "CoreBroker の call",
        "local ブローカーの call",
    ]);
    for r in rows.iter() {
        println!(
            "| {} | {} | {} | {} |",
            r.label,
            ms(r.make),
            ms(r.core_call),
            ms(r.local_call)
        );
    }
    println!("\n### HTTP (別プロセス)");
    header(&[
        "",
        "(1) call の戻り値",
        "(2) 引数で送る",
        "PNG 圧縮",
        "PNG 展開",
        "(1) の転送ほか",
        "(2) の転送ほか",
        "生バイトを Unix ソケットで",
    ]);
    for r in rows.iter() {
        // (1) は別プロセスで画像を複製する時間も含むので、それも引く
        let rest1 = r.http_return.saturating_sub(r.encode + r.decode + r.make);
        let (arg, rest2) = match &r.http_arg {
            Ok(d) => (ms(*d), ms(d.saturating_sub(r.encode + r.decode))),
            Err(_) => ("送れない".to_owned(), "-".to_owned()),
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            r.label,
            ms(r.http_return),
            arg,
            ms(r.encode),
            ms(r.decode),
            ms(rest1),
            rest2,
            ms(r.unix_raw)
        );
    }
    println!("\n### IPC (別プロセス、生バイトのフレーム、#233)");
    header(&[
        "",
        "(1) call の戻り値",
        "(2) 引数で送る",
        "HTTP (1) との比",
        "生バイトを Unix ソケットで",
    ]);
    for r in rows.iter() {
        println!(
            "| {} | {} | {} | {:.1} 倍速い | {} |",
            r.label,
            ms(r.ipc_return),
            ms(r.ipc_arg),
            r.http_return.as_secs_f64() / r.ipc_return.as_secs_f64(),
            ms(r.unix_raw)
        );
    }
    println!("\n### qmp (別プロセス、QUIC のループバック、生バイトのフレーム、#234)");
    header(&[
        "",
        "(1) call の戻り値",
        "(2) 引数で送る",
        "HTTP (1) との比",
        "IPC (1) との比",
    ]);
    for r in rows.iter() {
        println!(
            "| {} | {} | {} | {:.1} 倍速い | {:.1} 倍 |",
            r.label,
            ms(r.qmp_return),
            ms(r.qmp_arg),
            r.http_return.as_secs_f64() / r.qmp_return.as_secs_f64(),
            r.qmp_return.as_secs_f64() / r.ipc_return.as_secs_f64(),
        );
    }
    println!("\n### Topic (別プロセス、IPC で subscribe、#235)");
    header(&[
        "",
        "A: publish が返るまで",
        "A: 受け手に届くまで",
        "B: publish が返るまで",
        "B: 受け手に届くまで",
        "届くまでの A との比",
    ]);
    for r in rows.iter() {
        println!(
            "| {} | {} | {} | {} | {} | {:.1} 倍速い |",
            r.label,
            ms(r.topic_push[0]),
            ms(r.topic_delivered[0]),
            ms(r.topic_push[1]),
            ms(r.topic_delivered[1]),
            r.topic_delivered[0].as_secs_f64() / r.topic_delivered[1].as_secs_f64(),
        );
    }
    println!("\n### Topic B の受け手の内訳 (1 フレームあたりの平均、#270)");
    header(&[
        "",
        "共有メモリからのコピー [ms]",
        "コピー中のページフォールト [回]",
        "Topic への push [ms]",
        "領域を使い回せた割合",
    ]);
    for r in rows.iter() {
        let b = r.topic_b_receive;
        println!(
            "| {} | {} | {:.0} | {} | {:.0}% |",
            r.label,
            ms(b.copy),
            b.page_faults,
            ms(b.push),
            b.reused * 100.0,
        );
    }
    println!("\n### 送れないもの");
    for r in rows.iter() {
        if let Err(e) = &r.http_arg {
            println!(
                "- (2) 引数で送る {} (PNG {:.2} MB): {e}",
                r.label,
                r.png_bytes as f64 / 1e6
            );
        }
    }
    println!("- Topic (同じプロセス内): {topic}");
    Ok(())
}
