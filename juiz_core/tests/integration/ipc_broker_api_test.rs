//! IPC ブローカー経由で別の System の Process / Container を操作するテスト (issue #233、docs/spec/ipc-broker.md §8)
//!
//! - 同じプロセス内: System 自身の IPC ブローカーに IPC の BrokerProxy で繋ぎ、
//!   Unix ソケット (Windows は名前付きパイプ) → IpcBroker → CRUDBroker → CoreBroker の経路を通す。
//!   画像 (形式ごと) と数の配列の引数と戻り値、Topic の push で画像を送ること、エラー応答、
//!   サーバーを作り直した後の再接続、残ったソケットの片付け、名前の検証を確かめる。
//! - 別プロセス: juiz -d --ipc で IPC ブローカーを持つサーバーを起動し、テスト側から呼ぶ。
//!
//! プラグイン (increment_process, example_container, timer_ec) と juiz コマンドは
//! cargo build で ../target/debug にビルドされている前提 (CI の test.yml と同じ)。

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{ImageBuffer, Luma, LumaA, Rgb, Rgba};
use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::broker_api::{assert_no_options, find_process, int_of};

fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/debug")
}

fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

/// IPC ブローカーを持ち、Process / Container / EC を読み込んだ System の設定
pub(crate) fn server_manifest(system_name: &str, ipc_name: &str) -> Value {
    let path = target_dir().to_string_lossy().into_owned();
    jvalue!({
        "name": system_name,
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "process_factories": { "increment_process": { "path": path } },
            "container_factories": {
                "example_container": {
                    "path": path,
                    "processes": {
                        "example_container_increment": { "path": path },
                        "example_container_get": { "path": path }
                    }
                }
            },
            "ec_factories": { "timer_ec": { "path": path } }
        },
        "brokers": [ { "type_name": "ipc", "name": ipc_name } ],
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
        "ecs": [ { "type_name": "TimerEC", "name": "timer0", "rate": 100.0 } ]
    })
}

// ---- 画像と配列を扱う Process (Rust の関数で登録する) ----

fn image_of_format(format: &str, w: u32, h: u32) -> DynamicImage {
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

const FORMATS: [&str; 7] = ["l8", "la8", "rgb8", "rgba8", "l16", "rgb16", "rgba32f"];

/// 指定の形式と大きさの画像を返す
fn ipc_image_make(args: CapsuleMap) -> JuizResult<Capsule> {
    let format = args.get("format")?.lock_as_value(|v| v.as_str().unwrap_or("rgb8").to_owned())?;
    let w = args.get("width")?.lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    let h = args.get("height")?.lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    Ok(image_of_format(&format, w, h).into())
}

/// 受け取った画像をそのまま返す
fn ipc_image_echo(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("img")?.extract_image()?.into())
}

/// 受け取った画像の画素のバイトの合計
fn ipc_image_sum(args: CapsuleMap) -> JuizResult<Capsule> {
    let sum: u64 = args
        .get("img")?
        .lock_as_image(|img| img.as_bytes().iter().map(|b| *b as u64).sum())?;
    Ok(jvalue!(sum).into())
}

/// 受け取った値をそのまま返す
fn ipc_value_echo(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("value")?.extract_value()?.into())
}

fn ipc_always_fail(_args: CapsuleMap) -> JuizResult<Capsule> {
    Err(juiz_sdk::anyhow::anyhow!("ipc_always_fail fails on purpose"))
}

pub(crate) fn setup_test_processes(system: &System) -> JuizResult<(ProcessPtr, ProcessPtr)> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let factories: Vec<(ProcessManifest, juiz_sdk::processes::ProcessBodyFunctionType)> = vec![
        (
            ProcessManifest::new("ipc_image_make")
                .use_memo(false)
                .add_string_arg("format", "format", "rgb8")
                .add_int_arg("width", "width", 1)
                .add_int_arg("height", "height", 1),
            ipc_image_make,
        ),
        (
            ProcessManifest::new("ipc_image_echo")
                .use_memo(false)
                .add_image_arg("img", "image"),
            ipc_image_echo,
        ),
        (
            ProcessManifest::new("ipc_image_sum")
                .use_memo(true)
                .add_image_arg("img", "image"),
            ipc_image_sum,
        ),
        (
            ProcessManifest::new("ipc_value_echo")
                .use_memo(false)
                .add_object_arg("value", "value", jvalue!({})),
            ipc_value_echo,
        ),
        (
            ProcessManifest::new("ipc_always_fail").use_memo(false),
            ipc_always_fail,
        ),
    ];
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        worker.store_mut().processes.register_factory(&type_name, pf)?;
    }
    for (type_name, name) in [
        ("ipc_image_make", "make0"),
        ("ipc_image_echo", "echo0"),
        ("ipc_value_echo", "vecho0"),
        ("ipc_always_fail", "fail0"),
    ] {
        worker.create_process_ref(&ProcessManifest::new(type_name).name(name).use_memo(false))?;
    }
    let sink = worker.create_process_ref(
        &ProcessManifest::new("ipc_image_sum")
            .name("sum0")
            // 届いた値を get_output で確かめるため、出力を覚える
            .use_memo(true),
    )?;
    let echo = worker.create_process_ref(
        &ProcessManifest::new("ipc_image_echo")
            .name("pub0")
            .use_memo(false),
    )?;
    Ok((echo, sink))
}

fn ipc_proxy(system: &System, name: &str) -> JuizResult<std::sync::Arc<std::sync::Mutex<dyn BrokerProxy>>> {
    system.core_broker().lock()?.worker().broker_proxy("ipc", name, true)
}

/// IPC 経由で得た識別子は、アクセスした IPC ブローカーを指す
fn check_processes(bp: &mut dyn BrokerProxy, ipc_name: &str) -> JuizResult<()> {
    assert!(bp.system_profile_full()?.is_object());
    assert!(bp.system_uuid()?.as_str().is_some());
    let ids = bp.process_list(false, None)?;
    assert!(ids.len() >= 2, "{ids:?}");
    for id in ids.iter() {
        assert_eq!(id.broker_type_name, "ipc", "{id:?}");
        assert_eq!(id.broker_name, ipc_name, "{id:?}");
    }
    // 返された ipc:// の識別子をそのまま渡しても、サーバーが自分宛てに再接続せずに処理できる
    let inc0 = find_process(&ids, "inc0");
    assert_eq!(int_of(bp.process_call(inc0, args("arg1", 10))), 11);
    assert_eq!(int_of(bp.any_process_call(inc0, args("arg1", 20))), 21);
    assert_eq!(bp.process_profile_full(inc0)?.name, "inc0");
    bp.process_p_apply(inc0, "arg1", jvalue!(1).into())?;
    assert_eq!(int_of(bp.process_execute(inc0)), 2);
    assert_eq!(int_of(bp.process_push_by(inc0, "arg1".to_owned(), jvalue!(7).into())), 8);
    // HTTP と同じく、結果の option は空 (#277)
    assert_no_options(bp.process_call(inc0, args("arg1", 1)));
    assert_no_options(bp.any_process_call(inc0, args("arg1", 1)));
    assert_no_options(bp.process_execute(inc0));

    let brokers = bp.broker_list(false)?;
    assert!(
        brokers.iter().any(|id| id.ends_with(&format!("/broker/{ipc_name}::ipc"))),
        "{brokers:?}"
    );
    Ok(())
}

fn check_containers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.container_process_list(false, None)?;
    let add = find_process(&ids, "c0inc");
    let get = find_process(&ids, "c0get");
    let before = int_of(bp.container_process_call(get, CapsuleMap::new()));
    assert_eq!(int_of(bp.container_process_call(add, args("arg1", 4))), before + 4);
    assert_eq!(int_of(bp.any_process_call(get, CapsuleMap::new())), before + 4);
    let containers = bp.container_list(false, None)?;
    assert_eq!(containers.len(), 1);
    assert_eq!(bp.container_profile_full(&containers[0])?.name, "c0");
    Ok(())
}

fn check_create_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let n = bp.process_list(false, None)?.len();
    let created = bp.process_create(&ProcessManifest::new("increment_process").name("inc_new"))?;
    assert_eq!(created.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), n + 1);
    assert_eq!(bp.process_destroy(&created.identifier())?.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), n);

    let cprofile = bp.container_create(
        &ContainerManifest::new("example_container").name("c_new"),
        vec![("name", jvalue!("c_new")), ("initial_value", jvalue!(5))].into(),
    )?;
    assert_eq!(cprofile.name, "c_new");
    assert_eq!(bp.container_destroy(&cprofile.identifier())?.name, "c_new");
    Ok(())
}

fn check_connections(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let src = find_process(&ids, "inc0").clone();
    let dst = find_process(&ids, "inc1").clone();
    let cm = ConnectionManifest::new(ConnectionType::Pull, src.clone(), "arg1".to_owned(), dst.clone());
    assert_eq!(bp.connection_create(&cm)?.arg_name, "arg1");
    // Topic のコネクションも並ぶので、inc0 -> inc1 のものを探す
    let is_inc = |c: &juiz_sdk::connection_identifier::ConnectionIdentifier| {
        c.source_identifier.name == "inc0" && c.destination_identifier.name == "inc1"
    };
    let cons = bp.connection_list(false)?;
    let con = cons.iter().find(|c| is_inc(c)).expect("connection not listed").clone();
    let src_out = int_of(bp.process_execute(&src));
    assert_eq!(int_of(bp.process_execute(&dst)), src_out + 1);
    assert_eq!(bp.connection_destroy(&con)?.arg_name, "arg1");
    assert!(!bp.connection_list(false)?.iter().any(|c| is_inc(c)));
    Ok(())
}

fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ecs = bp.ec_list(false)?;
    let id = ecs
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .find(|id| id.contains("timer0"))
        .expect("timer0 not listed")
        .to_owned();
    assert!(id.starts_with("ipc://"), "{id}");
    assert_eq!(bp.ec_get_state(&id)?["__value__"], jvalue!("STOPPED"));
    bp.ec_start(&id)?;
    bp.ec_stop(&id)?;
    Ok(())
}

/// サーバーで失敗した要求はエラー応答になり、接続はそのまま使える
fn check_error_keeps_connection(bp: &mut dyn BrokerProxy, ipc_name: &str) -> JuizResult<()> {
    let missing = ProcessIdentifier::try_from(format!("ipc://{ipc_name}/process/nothing::increment_process"))?;
    let e = bp.process_call(&missing, args("arg1", 1)).unwrap_err();
    assert!(
        matches!(e.downcast_ref::<JuizError>(), Some(JuizError::BrokerProxyRemoteError { .. })),
        "{e:?}"
    );
    let ids = bp.process_list(false, None)?;
    let fail = find_process(&ids, "fail0");
    let e = bp.process_call(fail, CapsuleMap::new()).unwrap_err();
    assert!(format!("{e:#}").contains("fails on purpose"), "{e:#}");
    // 同じ接続でまだ呼べる
    assert_eq!(int_of(bp.process_call(find_process(&ids, "inc0"), args("arg1", 1))), 2);
    Ok(())
}

/// 画像は形式ごとに、画素を変えずに引数と戻り値で届く
fn check_images(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
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
    // 720p の rgb8
    let hd = image_of_format("rgb8", 1280, 720);
    let mut args = CapsuleMap::new();
    args.insert("img".to_owned(), hd.clone().into());
    assert_eq!(bp.process_call(&echo, args)?.extract_image()?, hd);
    Ok(())
}

/// 要素の多い数の配列は本体で送られ、整数と小数の区別が保たれる。混ざった配列は JSON のまま届く
fn check_arrays(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
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

/// IPC の Topic の push で画像を送ると、subscribe 側に届く (docs/spec/large-data.md §7 の 3)
fn check_topic_push_image(bp: &mut dyn BrokerProxy, sink: &ProcessPtr) -> JuizResult<()> {
    let img = image_of_format("rgb8", 320, 240);
    let expected: u64 = img.as_bytes().iter().map(|b| *b as u64).sum();
    bp.topic_push("ipc_image_topic", img.into(), Some(uuid::Uuid::new_v4()))?;
    let out = sink.lock()?.get_output();
    assert_eq!(out.extract_value()?, jvalue!(expected));
    Ok(())
}

#[test]
fn ipc_broker_api_in_same_process() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "ipc_api_test";
    System::new(server_manifest("ipc_broker_api_test", ipc_name))?
        .setup()?
        .run_and_do_once(|system| {
            let (publisher, sink) = setup_test_processes(system)?;
            {
                let mut cb = system.core_broker().lock_mut()?;
                let worker = cb.worker_mut();
                worker.process_subscribe_topic(
                    sink.clone(),
                    &"img".to_owned(),
                    TopicManifest::new("ipc_image_topic"),
                )?;
                // 同じプロセス内の Topic でも画像が流れる
                worker.process_publish_topic(publisher.clone(), TopicManifest::new("ipc_image_topic"))?;
            }
            let img = image_of_format("l8", 16, 8);
            let expected: u64 = img.as_bytes().iter().map(|b| *b as u64).sum();
            publisher.lock_mut()?.p_apply("img", img.into())?;
            publisher.lock()?.execute()?;
            assert_eq!(sink.lock()?.get_output().extract_value()?, jvalue!(expected));

            let proxy = ipc_proxy(system, ipc_name)?;
            let mut bp = juiz_lock(&proxy)?;
            check_processes(&mut *bp, ipc_name)?;
            check_containers(&mut *bp)?;
            check_error_keeps_connection(&mut *bp, ipc_name)?;
            check_images(&mut *bp)?;
            check_arrays(&mut *bp)?;
            check_topic_push_image(&mut *bp, &sink)?;
            check_connections(&mut *bp)?;
            check_create_destroy(&mut *bp)?;
            check_ec(&mut *bp)?;
            Ok(())
        })
}

/// 同じ名前のサーバーが動いていれば起動はエラー。止めたサーバーのソケットは消え、
/// 作り直したサーバーには同じ BrokerProxy のまま繋ぎ直せる。異常終了で残ったソケットは片付けて起動する
#[test]
fn ipc_broker_restart_and_stale_socket() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "ipc_restart_test";
    let manifest = || {
        jvalue!({
            "name": "ipc_restart_test",
            "option": { "http_broker": { "start": false } },
            "brokers": [ { "type_name": "ipc", "name": ipc_name } ]
        })
    };
    #[cfg(unix)]
    let socket = juiz_core::ipc::ipc_name::ipc_socket_path(ipc_name)?;
    #[cfg(unix)]
    {
        // 前のサーバーが異常終了して残したソケット (繋がらない) を置く
        let _ = std::fs::remove_file(&socket);
        drop(std::os::unix::net::UnixListener::bind(&socket)?);
        assert!(socket.exists());
    }

    let client_manifest = jvalue!({
        "name": "ipc_restart_client",
        "option": { "http_broker": { "start": false } }
    });
    System::new(client_manifest)?.setup()?.run_and_do_once(|client| {
        let proxy = ipc_proxy(client, ipc_name)?;
        // サーバーが居なければエラーになる
        assert!(juiz_lock(&proxy)?.system_uuid().is_err());

        let mut uuid1 = Value::Null;
        System::new(manifest())?.setup()?.run_and_do_once(|_first| {
            uuid1 = juiz_lock(&proxy)?.system_uuid()?;
            // 同じ名前ではもう 1 つ起動できない
            let e = System::new(manifest())?.setup().err().expect("must fail");
            assert!(format!("{e:#}").contains("already in use"), "{e:#}");
            Ok(())
        })?;
        // 止めたサーバーのソケットは消える
        #[cfg(unix)]
        assert!(!socket.exists(), "the socket must be removed when the broker stops");
        assert!(juiz_lock(&proxy)?.system_uuid().is_err());

        // 作り直したサーバーに、同じ BrokerProxy のまま繋ぎ直せる
        System::new(manifest())?.setup()?.run_and_do_once(|_second| {
            let uuid2 = juiz_lock(&proxy)?.system_uuid()?;
            assert_ne!(uuid1, uuid2);
            Ok(())
        })
    })
}

#[test]
fn ipc_broker_rejects_invalid_names() -> JuizResult<()> {
    crate::common::logger::init();
    for (key, name) in [("name", "bad/name"), ("name", ".hidden"), ("namespace", "x")] {
        let mut broker = jvalue!({ "type_name": "ipc" });
        broker[key] = jvalue!(name);
        let r = System::new(jvalue!({
            "name": "ipc_invalid_name_test",
            "option": { "http_broker": { "start": false } },
            "brokers": [ broker ]
        }))?
        .setup();
        assert!(r.is_err(), "{key}={name} must be rejected");
    }
    Ok(())
}

/// juiz -d --ipc <name> で起動したサーバー。drop で止める
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn ipc_broker_api_between_processes() -> JuizResult<()> {
    crate::common::logger::init();
    let ipc_name = "ipc_cli_test";
    let conf = std::env::temp_dir().join(format!("juiz_ipc_cli_test_{}.conf", std::process::id()));
    // YAML の二重引用符では Windows のパスの \ がエスケープとして読まれるので、一重引用符で書く
    let path = target_dir().to_string_lossy().replace('\'', "''");
    std::fs::write(
        &conf,
        format!(
            "name: ipc_cli_test_server\nplugins:\n  process_factories:\n    increment_process:\n      path: '{path}'\nprocesses:\n  - type_name: increment_process\n    name: inc0\n"
        ),
    )?;
    let juiz = target_dir().join(if cfg!(windows) { "juiz.exe" } else { "juiz" });
    let mut server = Server(
        Command::new(juiz)
            .args(["-d", "-q", "--ipc", ipc_name, "-f"])
            .arg(&conf)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()?,
    );
    let result = System::new(jvalue!({
        "name": "ipc_cli_test_client",
        "option": { "http_broker": { "start": false } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let proxy = ipc_proxy(system, ipc_name)?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let ids = loop {
            if let Some(status) = server.0.try_wait()? {
                panic!("juiz server exited ({status})");
            }
            match juiz_lock(&proxy)?.process_list(false, None) {
                Ok(ids) if !ids.is_empty() => break ids,
                r => {
                    assert!(Instant::now() < deadline, "can not connect to server: {r:?}");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        };
        let inc0 = find_process(&ids, "inc0");
        assert_eq!(inc0.broker_type_name, "ipc");
        assert_eq!(int_of(juiz_lock(&proxy)?.process_call(inc0, args("arg1", 41))), 42);
        Ok(())
    });
    drop(server);
    let _ = std::fs::remove_file(&conf);
    result
}
