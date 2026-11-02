//! 共有メモリ (方式 B) で Topic を別プロセスへ流すテスト (issue #235、docs/spec/large-data.md §6)
//!
//! このテストバイナリ自身を `serve_for_shm_test` だけ実行するように起動して、受け手の juiz にする
//! (IPC ブローカーで待ち受け、Topic を subscribe する Process を持つ)。送り手は conf の `topics:` に
//! `shared_memory` を書き、受け手を `subsystems:` に IPC で登録する。
//!
//! - 720p と 4K の画像を間を空けて流し、欠けずに順に届くこと、画像が壊れていないことを確かめる。
//!   slot_bytes が auto のときに大きな画像が来たらフレームで送る (fallback) ことも確かめる。
//! - 受け手が遅いときは古いフレームを捨て、最後のフレームは届き、順序は崩れないこと
//!   (届いた数 + dropped + overwritten = 送った数)。
//! - 数え上げ (sent / fallback / dropped / overwritten) が Topic の profile に出ること。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{DynamicImage, RgbImage};
use juiz_sdk::process_identifier::ProcessIdentifier;

const SERVE_ENV: &str = "JUIZ_SHM_TEST_SERVE";
const READY: &str = "JUIZ_SHM_TEST_READY";

const TOPIC_HD: &str = "shm/hd";
const TOPIC_4K: &str = "shm/4k";
const TOPIC_SLOW: &str = "shm/slow";
/// 遅い受け手が 1 枚にかける時間
const SLOW_SINK_DELAY: Duration = Duration::from_millis(60);

/// 大きさごとの模様の画像 (作るのに時間がかかるので 1 度だけ作る)
fn base_image(width: u32, height: u32) -> DynamicImage {
    static CACHE: OnceLock<Mutex<HashMap<(u32, u32), DynamicImage>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap();
    cache
        .entry((width, height))
        .or_insert_with(|| {
            let mut bytes = vec![0u8; (width * height * 3) as usize];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = (i % 251) as u8;
            }
            DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, bytes).unwrap())
        })
        .clone()
}

/// seed を画像の先頭と末尾の 8 バイトに書いた画像。途中で上書きされた画像は先頭と末尾が合わなくなる
fn seeded_image(width: u32, height: u32, seed: u64) -> DynamicImage {
    let mut img = base_image(width, height);
    if let DynamicImage::ImageRgb8(buf) = &mut img {
        let raw: &mut [u8] = buf.as_mut();
        let n = raw.len();
        raw[..8].copy_from_slice(&seed.to_le_bytes());
        raw[n - 8..].copy_from_slice(&seed.to_le_bytes());
    }
    img
}

/// 画像の seed を読み、模様が壊れていないかを確かめる。壊れていたら None
fn check_image(img: &DynamicImage) -> Option<u64> {
    let raw = img.as_bytes();
    let n = raw.len();
    let head = u64::from_le_bytes(raw[..8].try_into().ok()?);
    let tail = u64::from_le_bytes(raw[n - 8..].try_into().ok()?);
    if head != tail {
        return None;
    }
    let ok = (8..n - 8).step_by(4093).all(|i| raw[i] == (i % 251) as u8);
    ok.then_some(head)
}

// ---- 受け手の Process ----

#[derive(Default)]
struct Received {
    /// (幅, seed)
    normal: Vec<(u32, u64)>,
    slow: Vec<u64>,
    broken: u64,
}

fn received() -> &'static Mutex<Received> {
    static R: OnceLock<Mutex<Received>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Received::default()))
}

fn shm_sink(args: CapsuleMap) -> JuizResult<Capsule> {
    let r = args.get("img")?.lock_as_image(|img| (img.width(), check_image(img)))?;
    let mut rec = received().lock().unwrap();
    match r {
        (w, Some(seed)) => rec.normal.push((w, seed)),
        (_, None) => rec.broken += 1,
    }
    Ok(jvalue!(0).into())
}

fn shm_slow_sink(args: CapsuleMap) -> JuizResult<Capsule> {
    std::thread::sleep(SLOW_SINK_DELAY);
    let r = args.get("img")?.lock_as_image(check_image)?;
    let mut rec = received().lock().unwrap();
    match r {
        Some(seed) => rec.slow.push(seed),
        None => rec.broken += 1,
    }
    Ok(jvalue!(0).into())
}

fn shm_report(_args: CapsuleMap) -> JuizResult<Capsule> {
    let rec = received().lock().unwrap();
    Ok(jvalue!({
        "normal": rec.normal.iter().map(|(w, s)| jvalue!([w, s])).collect::<Vec<Value>>(),
        "slow": rec.slow,
        "broken": rec.broken,
    })
    .into())
}

fn shm_source(args: CapsuleMap) -> JuizResult<Capsule> {
    let int = |name: &str| -> JuizResult<u64> {
        args.get(name)?.lock_as_value(|v| v.as_u64().unwrap_or(0))
    };
    Ok(seeded_image(int("width")? as u32, int("height")? as u32, int("seed")?).into())
}

fn register(
    worker: &mut CoreWorker,
    factories: Vec<(ProcessManifest, juiz_sdk::processes::ProcessBodyFunctionType)>,
) -> JuizResult<()> {
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        worker.store_mut().processes.register_factory(&type_name, pf)?;
    }
    Ok(())
}

fn ipc_name(role: &str) -> String {
    format!("shm_test_{role}_{}", std::process::id() % 100000)
}

/// 受け手の juiz として動く (テストの本体から起動される)。環境変数が無ければ何もしない
#[test]
#[ignore]
fn serve_for_shm_test() -> JuizResult<()> {
    let Ok(name) = std::env::var(SERVE_ENV) else {
        return Ok(());
    };
    crate::common::logger::init();
    System::new(jvalue!({
        "name": "shm_test_server",
        "option": { "http_broker": { "start": false } },
        "brokers": [ { "type_name": "ipc", "name": name } ],
    }))?
    .setup()?
    .run_and_do_once(|system| {
        {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            register(
                worker,
                vec![
                    (ProcessManifest::new("shm_sink").use_memo(false).add_image_arg("img", "image"), shm_sink),
                    (ProcessManifest::new("shm_slow_sink").use_memo(false).add_image_arg("img", "image"), shm_slow_sink),
                    (ProcessManifest::new("shm_report").use_memo(false), shm_report),
                ],
            )?;
            for (type_name, name, topic) in [
                ("shm_sink", "sink_hd", TOPIC_HD),
                ("shm_sink", "sink_4k", TOPIC_4K),
                ("shm_slow_sink", "sink_slow", TOPIC_SLOW),
            ] {
                let p = worker.create_process_ref(&ProcessManifest::new(type_name).name(name).use_memo(false))?;
                worker.process_subscribe_topic(p, &"img".to_owned(), TopicManifest::new(topic))?;
            }
            worker.create_process_ref(&ProcessManifest::new("shm_report").name("report0").use_memo(false))?;
        }
        println!("{READY}");
        std::io::stdout().flush()?;
        // 親が stdin を閉じる (終わるか落ちる) まで待ち受ける
        let mut buf = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut buf);
        Ok(())
    })
}

/// 受け手の juiz。drop で止める
struct Server {
    child: Child,
}

impl Server {
    fn start(name: &str) -> JuizResult<Server> {
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "ipc_shared_memory_test::serve_for_shm_test",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(SERVE_ENV, name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next() {
                Some(Ok(line)) if line.contains(READY) => break,
                Some(Ok(_)) => continue,
                _ => {
                    let _ = child.kill();
                    panic!("the server process exited before it got ready");
                }
            }
        }
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

/// profile の中から Topic の profile を探す (入れ子のどこにあってもよい)
fn find_topic_profile(v: &Value, topic: &str) -> Option<Value> {
    match v {
        Value::Object(obj) => {
            if let Some(Value::Array(topics)) = obj.get("topics") {
                for t in topics {
                    // Topic の Process の ID はネームスペース付き (…/shm/hd::topic)
                    let id = t.get("identifier").and_then(|n| n.as_str()).unwrap_or("");
                    if id.contains(&format!("/{topic}::")) {
                        return Some(t.clone());
                    }
                }
            }
            obj.values().find_map(|c| find_topic_profile(c, topic))
        }
        Value::Array(a) => a.iter().find_map(|c| find_topic_profile(c, topic)),
        _ => None,
    }
}

fn shm_stats(profile: &Value, topic: &str) -> Value {
    find_topic_profile(profile, topic)
        .unwrap_or_else(|| panic!("topic {topic} not found in {profile}"))
        .get("shared_memory")
        .cloned()
        .unwrap_or_else(|| panic!("topic {topic} has no shared_memory in its profile"))
}

fn stat(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or_else(|| panic!("{key} not in {v}"))
}

#[test]
fn shm_topic_between_processes() -> JuizResult<()> {
    crate::common::logger::init();
    let server_name = ipc_name("server");
    let server = Server::start(&server_name)?;
    let client_name = ipc_name("client");
    let r = System::new(jvalue!({
        "name": "shm_test_client",
        "option": { "http_broker": { "start": false } },
        "brokers": [ { "type_name": "ipc", "name": client_name } ],
        "subsystems": [ { "type_name": "ipc", "name": server_name } ],
        "topics": [
            // slot_bytes は auto (最初の 720p で 3 MiB に決まる)
            { "name": TOPIC_HD, "shared_memory": {} },
            // 4K RGB は 24,883,200 バイト
            { "name": TOPIC_4K, "shared_memory": { "slots": 3, "slot_bytes": 25165824 } },
            { "name": TOPIC_SLOW, "shared_memory": { "slots": 4 } },
        ],
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let sources = {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            register(
                worker,
                vec![(
                    ProcessManifest::new("shm_source")
                        .use_memo(false)
                        .add_int_arg("width", "width", 1280)
                        .add_int_arg("height", "height", 720)
                        .add_int_arg("seed", "seed", 0),
                    shm_source,
                )],
            )?;
            let mut sources = HashMap::new();
            for topic in [TOPIC_HD, TOPIC_4K, TOPIC_SLOW] {
                let name = format!("source_{}", topic.replace('/', "_"));
                let p = worker.create_process_ref(&ProcessManifest::new("shm_source").name(&name).use_memo(false))?;
                worker.process_publish_topic(p.clone(), TopicManifest::new(topic))?;
                sources.insert(topic, p);
            }
            sources
        };
        // setup の後で publish を足したので、受け手に subscribe するかを問い合わせる (setup_topic_synchronization と同じ)
        {
            let mut cb = system.core_broker().lock_mut()?;
            let uuid = uuid::Uuid::parse_str(cb.system_uuid()?.as_str().unwrap())?;
            let bp: &mut dyn BrokerProxy = &mut *cb;
            for topic in [TOPIC_HD, TOPIC_4K, TOPIC_SLOW] {
                assert_eq!(bp.topic_request_subscribe(topic, Some(uuid))?, jvalue!({"subscribe": true}));
            }
        }
        let publish = |topic: &str, width: u32, height: u32, seed: u64| -> JuizResult<()> {
            let p = &sources[topic];
            p.lock_mut()?.p_apply("width", jvalue!(width).into())?;
            p.lock_mut()?.p_apply("height", jvalue!(height).into())?;
            p.lock_mut()?.p_apply("seed", jvalue!(seed).into())?;
            p.lock()?.execute()?;
            Ok(())
        };

        let proxy = system.core_broker().lock()?.worker().broker_proxy("ipc", &server_name, true)?;
        let report_id: ProcessIdentifier = juiz_borrow(&proxy)?
            .process_list(false, None)?
            .into_iter()
            .find(|id| id.name == "report0")
            .expect("report0");
        let report = || -> JuizResult<Value> {
            juiz_borrow(&proxy)?.process_call(&report_id, CapsuleMap::new())?.extract_value()
        };
        let wait_until = |what: &str, f: &dyn Fn(&Value) -> bool| -> JuizResult<Value> {
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                let r = report()?;
                if f(&r) {
                    return Ok(r);
                }
                assert!(Instant::now() < deadline, "timeout waiting for {what}: {r}");
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let normal_of = |r: &Value, width: u64| -> Vec<u64> {
            r["normal"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e[0].as_u64() == Some(width))
                .map(|e| e[1].as_u64().unwrap())
                .collect()
        };

        // 720p: 間を空けて流せば、欠けずに順に届く
        let n_hd = 20u64;
        for seed in 1..=n_hd {
            publish(TOPIC_HD, 1280, 720, seed)?;
            let r = wait_until("720p frame", &|r| normal_of(r, 1280).len() as u64 >= seed)?;
            assert_eq!(normal_of(&r, 1280), (1..=seed).collect::<Vec<_>>());
        }
        // 1080p は auto で決まった枠 (3 MiB) に入らないので、フレームで送る (fallback)。それでも届く
        publish(TOPIC_HD, 1920, 1080, 100)?;
        wait_until("1080p frame", &|r| normal_of(r, 1920) == vec![100])?;

        // 4K
        let n_4k = 6u64;
        for seed in 1..=n_4k {
            publish(TOPIC_4K, 3840, 2160, seed)?;
            let r = wait_until("4K frame", &|r| normal_of(r, 3840).len() as u64 >= seed)?;
            assert_eq!(normal_of(&r, 3840), (1..=seed).collect::<Vec<_>>());
        }

        // 遅い受け手: 待たずに流すと古いフレームは捨てられるが、最後のフレームは届き、順序は崩れない
        let n_slow = 30u64;
        for seed in 1..=n_slow {
            publish(TOPIC_SLOW, 1280, 720, seed)?;
        }
        let r = wait_until("the last slow frame", &|r| {
            r["slow"].as_array().unwrap().last().and_then(|v| v.as_u64()) == Some(n_slow)
        })?;
        let slow: Vec<u64> = r["slow"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert!(slow.windows(2).all(|w| w[0] < w[1]), "frames must keep their order: {slow:?}");
        assert!((slow.len() as u64) < n_slow, "a slow subscriber must drop old frames: {slow:?}");
        assert_eq!(stat(&r, "broken"), 0, "no frame may be broken");

        // 数え上げ: 送り手
        let profile = system.core_broker().lock()?.system_profile_full()?;
        let hd = shm_stats(&profile, TOPIC_HD);
        assert_eq!((stat(&hd, "sent"), stat(&hd, "fallback")), (n_hd, 1), "{hd}");
        assert_eq!((stat(&hd, "slots"), stat(&hd, "slot_bytes")), (4, 3 << 20), "{hd}");
        let k4 = shm_stats(&profile, TOPIC_4K);
        assert_eq!((stat(&k4, "sent"), stat(&k4, "fallback")), (n_4k, 0), "{k4}");
        assert_eq!(stat(&shm_stats(&profile, TOPIC_SLOW), "sent"), n_slow);

        // 数え上げ: 受け手。届いた数 + 捨てた数 = 送った数
        let remote = juiz_borrow(&proxy)?.system_profile_full()?;
        let slow_stats = shm_stats(&remote, TOPIC_SLOW);
        assert_eq!(
            slow.len() as u64 + stat(&slow_stats, "dropped") + stat(&slow_stats, "overwritten"),
            n_slow,
            "{slow_stats}"
        );
        let hd_remote = shm_stats(&remote, TOPIC_HD);
        assert_eq!((stat(&hd_remote, "dropped"), stat(&hd_remote, "overwritten")), (0, 0), "{hd_remote}");
        assert_eq!(stat(&hd_remote, "slot_bytes"), 3 << 20, "{hd_remote}");
        Ok(())
    });
    drop(server);
    r
}
