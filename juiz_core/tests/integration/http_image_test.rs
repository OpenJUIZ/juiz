//! HTTP ブローカーで画像を送るテスト (issue #233、docs/spec/large-data.md §7 の 1〜3)
//!
//! - HttpBrokerProxy で画像を引数に渡せる (以前は CapsuleMap を JSON にするところで panic した)
//! - PNG が 2MB を超える画像も届く (以前は axum の本文の既定の上限 2MB に当たり、サーバーが panic した)
//! - 本文が max_body_bytes を超えたら 413 とエラーの JSON を返し、サーバーは動き続ける
//! - Topic の push で画像を送れる

use juiz_core::prelude::*;
use juiz_sdk::image::{ImageBuffer, Rgb};

use crate::broker_api::find_process;

const PORT: i64 = 8061;
const LIMITED_PORT: i64 = 8062;

/// PNG がほとんど縮まない (生とほぼ同じ大きさになる) 画像
fn noise_image(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        Rgb([h as u8, (h >> 8) as u8, (h >> 16) as u8])
    }))
}

fn byte_sum(img: &DynamicImage) -> u64 {
    img.as_bytes().iter().map(|b| *b as u64).sum()
}

fn image_sum(args: CapsuleMap) -> JuizResult<Capsule> {
    let gain = args.get("gain")?.lock_as_value(|v| v.as_i64().unwrap_or(1))?;
    let sum = args.get("img")?.lock_as_image(byte_sum)?;
    Ok(jvalue!(sum as i64 * gain).into())
}

fn setup(system: &System) -> JuizResult<ProcessPtr> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    let manifest = ProcessManifest::new("http_image_sum")
        .use_memo(true)
        .add_image_arg("img", "image")
        .add_int_arg("gain", "gain", 1);
    let pf = process_factory_create(manifest, image_sum)?;
    worker.store_mut().processes.register_factory("http_image_sum", pf)?;
    let sum = worker.create_process_ref(&ProcessManifest::new("http_image_sum").name("sum0"))?;
    let sink = worker.create_process_ref(&ProcessManifest::new("http_image_sum").name("sink0"))?;
    worker.process_subscribe_topic(sink.clone(), &"img".to_owned(), TopicManifest::new("http_image_topic"))?;
    drop(sum);
    Ok(sink)
}

fn manifest(name: &str, port: i64, max_body_bytes: Option<u64>) -> Value {
    let mut http = jvalue!({ "start": true, "port": port });
    if let Some(max) = max_body_bytes {
        http["max_body_bytes"] = jvalue!(max);
    }
    jvalue!({ "name": name, "option": { "http_broker": http } })
}

#[test]
fn http_broker_proxy_sends_large_image_argument() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(manifest("http_image_test", PORT, None))?
        .setup()?
        .run_and_do_once(|system| {
            let sink = setup(system)?;
            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "http",
                format!("localhost:{PORT}").as_str(),
                true,
            )?;
            let bp = juiz_borrow(&proxy)?;
            let ids = bp.process_list(false, None)?;
            let sum0 = find_process(&ids, "sum0").clone();

            // PNG が 2MB を超える画像を、値の引数と一緒に送る
            let img = noise_image(1280, 720);
            let mut png = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png, juiz_sdk::image::ImageFormat::Png)?;
            assert!(png.get_ref().len() > 2 << 20, "PNG must exceed 2MB ({})", png.get_ref().len());
            let mut args = CapsuleMap::new();
            args.insert("img".to_owned(), img.clone().into());
            args.insert("gain".to_owned(), jvalue!(3).into());
            let got = bp.process_call(&sum0, args)?.extract_value()?;
            assert_eq!(got, jvalue!(byte_sum(&img) * 3));

            // Topic の push で画像を送る
            let small = noise_image(64, 48);
            bp.topic_push("http_image_topic", small.clone().into(), Some(uuid::Uuid::new_v4()))?;
            assert_eq!(sink.lock()?.get_output().extract_value()?, jvalue!(byte_sum(&small)));
            Ok(())
        })
}

#[test]
fn http_broker_rejects_too_large_body_without_panic() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(manifest("http_image_limit_test", LIMITED_PORT, Some(1 << 20)))?
        .setup()?
        .run_and_do_once(|system| {
            setup(system)?;
            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "http",
                format!("localhost:{LIMITED_PORT}").as_str(),
                true,
            )?;
            let bp = juiz_borrow(&proxy)?;
            let ids = bp.process_list(false, None)?;
            let sum0 = find_process(&ids, "sum0").clone();

            let mut args = CapsuleMap::new();
            args.insert("img".to_owned(), noise_image(1280, 720).into());
            args.insert("gain".to_owned(), jvalue!(1).into());
            let e = bp.process_call(&sum0, args).unwrap_err();
            let msg = format!("{e:#}");
            assert!(msg.contains("413"), "{msg}");
            assert!(msg.contains("max_body_bytes"), "{msg}");

            // 壊れた multipart は 400
            let client = reqwest::blocking::Client::new();
            let res = client
                .put(format!("http://127.0.0.1:{LIMITED_PORT}/api/process/call"))
                .query(&[("identifier", sum0.to_string())])
                .header("Content-Type", "multipart/form-data; boundary=xyz")
                .body("--xyz\r\nContent-Disposition: form-data; name=\"img\"\r\nContent-Type: image/png\r\n\r\nnot a png\r\n--xyz--\r\n")
                .send()?;
            assert_eq!(res.status().as_u16(), 400);
            assert!(res.json::<Value>()?["message"].as_str().is_some());

            // サーバーは動き続け、小さな画像は届く
            let small = noise_image(32, 32);
            let mut args = CapsuleMap::new();
            args.insert("img".to_owned(), small.clone().into());
            args.insert("gain".to_owned(), jvalue!(1).into());
            assert_eq!(bp.process_call(&sum0, args)?.extract_value()?, jvalue!(byte_sum(&small)));
            Ok(())
        })
}
