//! qmp ブローカーのテストで共通に使う設定と確認の関数
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{ImageBuffer, Luma, LumaA, Rgb, Rgba};
use juiz_sdk::process_identifier::ProcessIdentifier;

pub fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug")
}

pub fn plugin_path() -> String {
    target_dir().to_string_lossy().into_owned()
}

/// qmp のプラグインを読み込む設定。組み込みの qmp を使うときは空にする
fn broker_factories(use_plugin: bool) -> Value {
    if use_plugin {
        jvalue!({ "qmp_broker": { "path": plugin_path() } })
    } else {
        jvalue!({})
    }
}

/// qmp ブローカーを持ち、Process / Container / EC を読み込んだ System の設定
pub fn server_manifest(name: &str, port: i64, http_port: i64, use_plugin: bool) -> Value {
    let path = plugin_path();
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false, "port": http_port }
        },
        "plugins": {
            "broker_factories": broker_factories(use_plugin),
            "process_factories": {
                "increment_process": { "path": path }
            },
            "container_factories": {
                "example_container": {
                    "path": path,
                    "processes": {
                        "example_container_increment": { "path": path },
                        "example_container_get": { "path": path }
                    }
                }
            },
            "ec_factories": {
                "timer_ec": { "path": path }
            }
        },
        "brokers": [
            { "type_name": "qmp", "name": format!("127.0.0.1:{port}"), "host": "127.0.0.1", "port": port }
        ],
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "increment_process", "name": "inc1" }
        ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [
                  { "type_name": "example_container_increment", "name": "c0inc" },
                  { "type_name": "example_container_get", "name": "c0get" } ] }
        ],
        "ecs": [
            { "type_name": "TimerEC", "name": "timer0", "rate": 100.0 }
        ]
    })
}

/// qmp の BrokerProxy だけを使う (ブローカーを持たない) System の設定
pub fn client_manifest(name: &str, use_plugin: bool) -> Value {
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": false }
        },
        "plugins": {
            "broker_factories": broker_factories(use_plugin)
        }
    })
}

pub fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

pub fn find_process<'a>(ids: &'a [ProcessIdentifier], name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found in {ids:?}"))
}

pub fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// 結果のカプセルに、要求の経路 (class_name / function_name など) が option として残っていない (#277)
pub fn assert_no_options(c: JuizResult<CapsulePtr>) -> CapsulePtr {
    let c = c.expect("call failed");
    let options = c.lock_as_capsule(|c| c.get_options().clone()).unwrap();
    assert!(options.is_empty(), "options should be empty: {options:?}");
    c
}

/// qmp 経由で得た識別子は、アクセスした qmp ブローカーを指す
pub fn check_processes(bp: &mut dyn BrokerProxy, broker_name: &str) -> JuizResult<()> {
    assert!(bp.system_profile_full()?.is_object());
    assert!(bp.system_uuid()?.as_str().is_some());

    let ids = bp.process_list(false, None)?;
    assert_eq!(ids.len(), 2, "{ids:?}");
    for id in ids.iter() {
        assert_eq!(id.broker_type_name, "qmp", "{id:?}");
        assert_eq!(id.broker_name, broker_name, "{id:?}");
    }
    // 返された qmp:// の識別子をそのまま渡しても、サーバーが自分宛てに再接続せずに処理できる
    let inc0 = find_process(&ids, "inc0");
    assert_eq!(int_of(bp.process_call(inc0, args("arg1", 10))), 11);
    assert_eq!(int_of(bp.any_process_call(inc0, args("arg1", 20))), 21);
    assert_eq!(bp.process_profile_full(inc0)?.name, "inc0");
    bp.process_p_apply(inc0, "arg1", jvalue!(1).into())?;
    assert_eq!(int_of(bp.process_execute(inc0)), 2);
    // HTTP と同じく、結果の option は空 (#277)
    assert_no_options(bp.process_call(inc0, args("arg1", 1)));
    assert_no_options(bp.any_process_call(inc0, args("arg1", 1)));
    assert_no_options(bp.process_execute(inc0));
    Ok(())
}

pub fn check_containers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let names: Vec<String> = bp
        .container_list(false, None)?
        .iter()
        .map(|id| bp.container_profile_full(id).map(|p| p.name))
        .collect::<JuizResult<_>>()?;
    assert_eq!(names, vec!["c0"]);

    let ids = bp.container_process_list(false, None)?;
    assert_eq!(ids.len(), 2, "{ids:?}");
    let add = find_process(&ids, "c0inc");
    let get = find_process(&ids, "c0get");
    let before = int_of(bp.container_process_call(get, CapsuleMap::new()));
    assert_eq!(
        int_of(bp.container_process_call(add, args("arg1", 4))),
        before + 4
    );
    assert_eq!(
        int_of(bp.any_process_call(get, CapsuleMap::new())),
        before + 4
    );
    Ok(())
}

/// サーバー側の処理が失敗するとエラーが返り、同じ接続でその後も呼び出せる
pub fn check_error_keeps_connection(bp: &mut dyn BrokerProxy, broker_name: &str) -> JuizResult<()> {
    let missing: ProcessIdentifier =
        format!("qmp://{broker_name}/process/nothing::increment_process").try_into()?;
    let start = Instant::now();
    assert!(bp.process_call(&missing, args("arg1", 1)).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(bp.process_profile_full(&missing).is_err());
    assert_eq!(bp.process_list(false, None)?.len(), 2);
    Ok(())
}

pub fn check_connections(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let src = find_process(&ids, "inc0").clone();
    let dst = find_process(&ids, "inc1").clone();
    let cm = ConnectionManifest::new(
        ConnectionType::Pull,
        src.clone(),
        "arg1".to_owned(),
        dst.clone(),
    );
    assert_eq!(bp.connection_create(&cm)?.arg_name, "arg1");
    let cons = bp.connection_list(false)?;
    assert_eq!(cons.len(), 1, "{cons:?}");

    // inc1 を実行すると inc0 の出力を pull して使う
    let src_out = int_of(bp.process_execute(&src));
    assert_eq!(int_of(bp.process_execute(&dst)), src_out + 1);

    assert_eq!(bp.connection_destroy(&cons[0])?.arg_name, "arg1");
    assert!(bp.connection_list(false)?.is_empty());
    Ok(())
}

pub fn check_create_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let created = bp.process_create(&ProcessManifest::new("increment_process").name("inc_new"))?;
    assert_eq!(created.name, "inc_new");
    let ids = bp.process_list(false, None)?;
    assert_eq!(ids.len(), 3);
    assert_eq!(
        int_of(bp.process_call(find_process(&ids, "inc_new"), args("arg1", 1))),
        2
    );
    assert_eq!(bp.process_destroy(&created.identifier())?.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), 2);
    Ok(())
}

pub fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ecs = bp.ec_list(false)?;
    let id = ecs
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .find(|id| id.contains("timer0"))
        .expect("timer0 not listed")
        .to_owned();
    let state = |bp: &mut dyn BrokerProxy| -> JuizResult<Value> {
        Ok(bp.ec_get_state(&id)?["__value__"].clone())
    };
    assert_eq!(state(bp)?, jvalue!("STOPPED"));
    bp.ec_start(&id)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while state(bp)? != jvalue!("STARTED") {
        assert!(Instant::now() < deadline, "timer0 did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    bp.ec_stop(&id)?;
    Ok(())
}

/// テストが失敗しても juiz サーバーのプロセスを残さない
pub struct ServerProcess(pub Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn spawn_server(port: i64, http_port: i64, use_plugin: bool) -> ServerProcess {
    let juiz = target_dir().join(format!("juiz{}", std::env::consts::EXE_SUFFIX));
    assert!(
        juiz.exists(),
        "{juiz:?} not found. Run `cargo build` before this test."
    );
    let conf = std::env::temp_dir().join(format!(
        "qmp_broker_test_server_{}_{port}.conf",
        std::process::id()
    ));
    // JSON は YAML としても読める
    std::fs::write(
        &conf,
        server_manifest("qmp_broker_test_server", port, http_port, use_plugin).to_string(),
    )
    .unwrap();
    let child = Command::new(juiz)
        .arg("-f")
        .arg(&conf)
        .arg("-d")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start juiz server");
    ServerProcess(child)
}

// ---- 画像と配列を扱う Process (Rust の関数で登録する、#234) ----

pub fn image_of_format(format: &str, w: u32, h: u32) -> DynamicImage {
    match format {
        "l8" => DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| {
            Luma([(x * 3 + y * 7) as u8])
        })),
        "la8" => DynamicImage::ImageLumaA8(ImageBuffer::from_fn(w, h, |x, y| {
            LumaA([(x + y) as u8, 200])
        })),
        "rgb8" => DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8])
        })),
        "rgba8" => DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([x as u8, y as u8, 9, 255])
        })),
        "l16" => DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| {
            Luma([(x * 300 + y) as u16])
        })),
        "rgb16" => DynamicImage::ImageRgb16(ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([x as u16 * 100, y as u16 * 200, 65535])
        })),
        "rgba32f" => DynamicImage::ImageRgba32F(ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([x as f32 / 7.0, y as f32 * -0.5, 1e-3, 1.0])
        })),
        _ => panic!("unknown format {format}"),
    }
}

pub const FORMATS: [&str; 7] = ["l8", "la8", "rgb8", "rgba8", "l16", "rgb16", "rgba32f"];

/// 指定の形式と大きさの画像を返す
fn qmp_image_make(args: CapsuleMap) -> JuizResult<Capsule> {
    let format = args.get("format")?.lock_as_value(|v| v.as_str().unwrap_or("rgb8").to_owned())?;
    let w = args.get("width")?.lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    let h = args.get("height")?.lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    Ok(image_of_format(&format, w, h).into())
}

/// 受け取った画像をそのまま返す
fn qmp_image_echo(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("img")?.extract_image()?.into())
}

/// 受け取った画像の画素のバイトの合計
fn qmp_image_sum(args: CapsuleMap) -> JuizResult<Capsule> {
    let sum: u64 = args
        .get("img")?
        .lock_as_image(|img| img.as_bytes().iter().map(|b| *b as u64).sum())?;
    Ok(jvalue!(sum).into())
}

/// 受け取った値をそのまま返す
fn qmp_value_echo(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("value")?.extract_value()?.into())
}

fn qmp_always_fail(_args: CapsuleMap) -> JuizResult<Capsule> {
    Err(juiz_sdk::anyhow::anyhow!("qmp_always_fail fails on purpose"))
}

/// 画像と配列の Process を登録して作る。Topic の確かめに使う (publish する Process, subscribe する Process) を返す
pub fn setup_data_processes(system: &System) -> JuizResult<(ProcessPtr, ProcessPtr)> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let factories: Vec<(ProcessManifest, juiz_sdk::processes::ProcessBodyFunctionType)> = vec![
        (
            ProcessManifest::new("qmp_image_make")
                .use_memo(false)
                .add_string_arg("format", "format", "rgb8")
                .add_int_arg("width", "width", 1)
                .add_int_arg("height", "height", 1),
            qmp_image_make,
        ),
        (
            ProcessManifest::new("qmp_image_echo")
                .use_memo(false)
                .add_image_arg("img", "image"),
            qmp_image_echo,
        ),
        (
            ProcessManifest::new("qmp_image_sum")
                .use_memo(true)
                .add_image_arg("img", "image"),
            qmp_image_sum,
        ),
        (
            ProcessManifest::new("qmp_value_echo")
                .use_memo(false)
                .add_object_arg("value", "value", jvalue!({})),
            qmp_value_echo,
        ),
        (
            ProcessManifest::new("qmp_always_fail").use_memo(false),
            qmp_always_fail,
        ),
    ];
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        worker.store_mut().processes.register_factory(&type_name, pf)?;
    }
    for (type_name, name) in [
        ("qmp_image_make", "make0"),
        ("qmp_image_echo", "echo0"),
        ("qmp_value_echo", "vecho0"),
        ("qmp_always_fail", "fail0"),
    ] {
        worker.create_process_ref(&ProcessManifest::new(type_name).name(name).use_memo(false))?;
    }
    let sink = worker.create_process_ref(
        &ProcessManifest::new("qmp_image_sum")
            .name("sum0")
            // 届いた値を get_output で確かめるため、出力を覚える
            .use_memo(true),
    )?;
    let publisher = worker.create_process_ref(
        &ProcessManifest::new("qmp_image_echo")
            .name("pub0")
            .use_memo(false),
    )?;
    Ok((publisher, sink))
}

/// サーバーのエラーは BrokerProxyRemoteError で返り、Process の失敗の文も届く
pub fn check_remote_error(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let e = bp.process_call(find_process(&ids, "fail0"), CapsuleMap::new()).unwrap_err();
    assert!(
        matches!(e.downcast_ref::<JuizError>(), Some(JuizError::BrokerProxyRemoteError { .. })),
        "{e:?}"
    );
    assert!(format!("{e:#}").contains("fails on purpose"), "{e:#}");
    Ok(())
}

/// 画像は形式ごとに、画素を変えずに引数と戻り値で届く
pub fn check_images(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let make = find_process(&ids, "make0").clone();
    let echo = find_process(&ids, "echo0").clone();
    for format in FORMATS {
        let expected = image_of_format(format, 33, 17);
        let args: CapsuleMap = vec![
            ("format", jvalue!(format)),
            ("width", jvalue!(33)),
            ("height", jvalue!(17)),
        ]
        .into();
        let returned = bp.process_call(&make, args)?.extract_image()?;
        assert_eq!(returned, expected, "{format}: return value");

        let mut args = CapsuleMap::new();
        args.insert("img".to_owned(), expected.clone().into());
        let echoed = bp.process_call(&echo, args)?.extract_image()?;
        assert_eq!(echoed, expected, "{format}: argument and return value");
    }
    // 720p の rgb8 (QUIC のフロー制御の窓より大きい)
    let hd = image_of_format("rgb8", 1280, 720);
    let mut args = CapsuleMap::new();
    args.insert("img".to_owned(), hd.clone().into());
    assert_eq!(bp.process_call(&echo, args)?.extract_image()?, hd);
    Ok(())
}

/// 要素の多い数の配列は本体で送られ、整数と小数の区別が保たれる。混ざった配列は JSON のまま届く
pub fn check_arrays(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let vecho = find_process(&ids, "vecho0").clone();
    let ints: Vec<i64> = (0..5000).map(|i| i * 7 - 9000).collect();
    let floats: Vec<f64> = (0..5000).map(|i| i as f64 / 3.0).collect();
    let mut mixed: Vec<Value> = (0..2000).map(|i| jvalue!(i)).collect();
    mixed.push(jvalue!(1.5));
    for value in [jvalue!(ints), jvalue!(floats), jvalue!(mixed), jvalue!({"small": [1, 2.0]})] {
        let got = bp
            .process_call(&vecho, vec![("value", value.clone())].into())?
            .extract_value()?;
        assert_eq!(got, value);
        if let (Some(a), Some(b)) = (got.as_array(), value.as_array()) {
            for (x, y) in a.iter().zip(b.iter()) {
                assert_eq!(x.is_i64(), y.is_i64());
                assert_eq!(x.is_f64(), y.is_f64());
            }
        }
    }
    Ok(())
}

/// qmp の Topic の push で画像を送ると、subscribe 側に届く
pub fn check_topic_push_image(bp: &mut dyn BrokerProxy, topic: &str, sink: &ProcessPtr) -> JuizResult<()> {
    let img = image_of_format("rgb8", 320, 240);
    let expected: u64 = img.as_bytes().iter().map(|b| *b as u64).sum();
    bp.topic_push(topic, img.into(), Some(uuid::Uuid::new_v4()))?;
    let out = sink.lock()?.get_output();
    assert_eq!(out.extract_value()?, jvalue!(expected));
    Ok(())
}
