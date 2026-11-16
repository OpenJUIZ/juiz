//! 画像・数の配列・Topic の push を生バイトのフレーム (docs/spec/large-data.md §3) で送る確認。
//! IPC・qmp・Zenoh のようにフレームで受け渡すブローカーで走らせる。
//!
//! 相手の System には [`setup_data_processes`] で Rust の関数の Process を登録しておく。
//! 同じプロセス内の System に、そのブローカーの BrokerProxy で繋いで確かめる。

use juiz_core::prelude::*;
use juiz_sdk::image::{ImageBuffer, Luma, LumaA, Rgb, Rgba};

use crate::support::*;

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
fn image_make(args: CapsuleMap) -> JuizResult<Capsule> {
    let format = args
        .get("format")?
        .lock_as_value(|v| v.as_str().unwrap_or("rgb8").to_owned())?;
    let w = args
        .get("width")?
        .lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    let h = args
        .get("height")?
        .lock_as_value(|v| v.as_u64().unwrap_or(1))? as u32;
    Ok(image_of_format(&format, w, h).into())
}

/// 受け取った画像をそのまま返す
fn image_echo(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("img")?.extract_image()?.into())
}

/// 受け取った画像の画素のバイトの合計
fn image_sum(args: CapsuleMap) -> JuizResult<Capsule> {
    let sum: u64 = args
        .get("img")?
        .lock_as_image(|img| img.as_bytes().iter().map(|b| *b as u64).sum())?;
    Ok(jvalue!(sum).into())
}

/// 受け取った値をそのまま返す
fn value_echo(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("value")?.extract_value()?.into())
}

fn always_fail(_args: CapsuleMap) -> JuizResult<Capsule> {
    Err(juiz_sdk::anyhow::anyhow!("always_fail fails on purpose"))
}

/// 画像と配列の Process を登録して作る。Topic の確かめに使う (publish する Process, subscribe する Process) を返す
pub fn setup_data_processes(system: &System) -> JuizResult<(ProcessPtr, ProcessPtr)> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let factories: Vec<(
        ProcessManifest,
        juiz_sdk::processes::ProcessBodyFunctionType,
    )> = vec![
        (
            ProcessManifest::new("image_make")
                .use_memo(false)
                .add_string_arg("format", "format", "rgb8")
                .add_int_arg("width", "width", 1)
                .add_int_arg("height", "height", 1),
            image_make,
        ),
        (
            ProcessManifest::new("image_echo")
                .use_memo(false)
                .add_image_arg("img", "image"),
            image_echo,
        ),
        (
            ProcessManifest::new("image_sum")
                .use_memo(true)
                .add_image_arg("img", "image"),
            image_sum,
        ),
        (
            ProcessManifest::new("value_echo")
                .use_memo(false)
                .add_object_arg("value", "value", jvalue!({})),
            value_echo,
        ),
        (
            ProcessManifest::new("always_fail").use_memo(false),
            always_fail,
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
    for (type_name, name) in [
        ("image_make", "make0"),
        ("image_echo", "echo0"),
        ("value_echo", "vecho0"),
        ("always_fail", "fail0"),
    ] {
        worker.create_process_ref(&ProcessManifest::new(type_name).name(name).use_memo(false))?;
    }
    let sink = worker.create_process_ref(
        &ProcessManifest::new("image_sum")
            .name("sum0")
            // 届いた値を get_output で確かめるため、出力を覚える
            .use_memo(true),
    )?;
    let publisher = worker.create_process_ref(
        &ProcessManifest::new("image_echo")
            .name("pub0")
            .use_memo(false),
    )?;
    Ok((publisher, sink))
}

/// [`setup_data_processes`] に加えて、pub0 の出力を topic に publish し、sum0 の img で subscribe する。
/// 同じプロセス内の Topic でも画像が流れることを確かめてから (publish する Process, subscribe する Process) を返す
pub fn setup_data_processes_with_topic(
    system: &System,
    topic: &str,
) -> JuizResult<(ProcessPtr, ProcessPtr)> {
    let (publisher, sink) = setup_data_processes(system)?;
    {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        worker.process_subscribe_topic(
            sink.clone(),
            &"img".to_owned(),
            TopicManifest::new(topic),
        )?;
        worker.process_publish_topic(publisher.clone(), TopicManifest::new(topic))?;
    }
    let img = image_of_format("l8", 16, 8);
    let expected: u64 = img.as_bytes().iter().map(|b| *b as u64).sum();
    publisher.lock_mut()?.p_apply("img", img.into())?;
    publisher.lock()?.execute()?;
    assert_eq!(
        sink.lock()?.get_output().extract_value()?,
        jvalue!(expected)
    );
    Ok((publisher, sink))
}

/// サーバーのエラーは BrokerProxyRemoteError で返り、Process の失敗の文も届く。
/// エラーの後も同じ接続で呼べる
pub fn check_remote_error(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let is_remote_error = |e: &juiz_sdk::anyhow::Error| {
        matches!(
            e.downcast_ref::<JuizError>(),
            Some(JuizError::BrokerProxyRemoteError { .. })
        )
    };
    let missing = renamed(find_process(&ids, "make0"), "nothing");
    let e = bp.process_call(&missing, CapsuleMap::new()).unwrap_err();
    assert!(is_remote_error(&e), "{e:?}");

    let e = bp
        .process_call(find_process(&ids, "fail0"), CapsuleMap::new())
        .unwrap_err();
    assert!(is_remote_error(&e), "{e:?}");
    assert!(format!("{e:#}").contains("fails on purpose"), "{e:#}");
    // 同じ接続でまだ呼べる
    let vecho = find_process(&ids, "vecho0");
    let got = bp
        .process_call(vecho, vec![("value", jvalue!({"a": 1}))].into())?
        .extract_value()?;
    assert_eq!(got, jvalue!({"a": 1}));
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
    // 720p の rgb8 (QUIC のフロー制御の窓より大きく、Zenoh が分割して送る大きさ)
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
    for value in [
        jvalue!(ints),
        jvalue!(floats),
        jvalue!(mixed),
        jvalue!({"small": [1, 2.0]}),
    ] {
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

/// Topic の push で画像を送ると、subscribe 側に届く (docs/spec/large-data.md §7 の 3)
pub fn check_topic_push_image(
    bp: &mut dyn BrokerProxy,
    topic: &str,
    sink: &ProcessPtr,
) -> JuizResult<()> {
    let img = image_of_format("rgb8", 320, 240);
    let expected: u64 = img.as_bytes().iter().map(|b| *b as u64).sum();
    bp.topic_push(topic, img.into(), Some(uuid::Uuid::new_v4()))?;
    let out = sink.lock()?.get_output();
    assert_eq!(out.extract_value()?, jvalue!(expected));
    Ok(())
}
