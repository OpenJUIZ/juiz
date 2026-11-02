//! C++ と Haskell のプラグインの画像の引数・戻り値のテスト (issue #253、docs/spec/large-data.md §10)
//!
//! C++ の Component examples/cpp/component/cpp_image_component と、Haskell の Component
//! examples/haskell/src/ImageComponent.hs (image_component_hs) を読む。どちらも同じ Process と Container を持つ。
//!
//! - 形式ごとに、画像の引数が画素を変えずに届き、画像の戻り値が返る (8 bit の形式は反転、それ以外はそのまま)
//! - BGR8 で返した画像は RGB8 になる
//! - 画像の後ろに引数が続く Process、Container の ContainerProcess の画像の引数と戻り値
//! - C++ → Haskell・Haskell → C++・Rust → C++ の接続 (Composite) で画像が渡る
//! - 同じことが IPC ブローカー越し (生バイトのフレーム) でもできる
//!
//! C++ と Haskell の例は scripts/test.sh (CI も同じ) でビルドされている前提。GHC が無い環境では
//! Haskell の例が無いので Haskell の分だけ飛ばす (JUIZ_REQUIRE_HASKELL=1 なら失敗させる)。

use juiz_core::prelude::*;
use juiz_sdk::image::{ImageBuffer, Luma, LumaA, Rgb, Rgba};
use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::broker_api::find_process;

fn plugin_file(dir: &str, name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    let file = format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    let file = format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    let file = format!("{name}.dll");
    std::path::PathBuf::from(dir).join(file)
}

/// Haskell の例がビルドされていれば true
fn has_haskell_example() -> bool {
    let file = plugin_file("../target", "image_component_hs");
    if file.exists() {
        return true;
    }
    if std::env::var("JUIZ_REQUIRE_HASKELL").map_or(false, |v| v == "1") {
        panic!("Haskell の例がビルドされていません: {file:?}");
    }
    eprintln!("skip: Haskell の例がビルドされていません ({file:?})。Haskell の分は飛ばします。");
    false
}

/// 調べる言語の接尾辞 (C++ は "cpp"、Haskell は "hs")
fn languages() -> Vec<&'static str> {
    let mut v = vec!["cpp"];
    if has_haskell_example() {
        v.push("hs");
    }
    v
}

fn conf(name: &str, ipc_name: Option<&str>, langs: &[&str]) -> Value {
    let mut processes = Vec::new();
    let mut containers = Vec::new();
    for l in langs {
        for p in ["image_invert", "image_bgr", "image_info"] {
            processes.push(jvalue!({ "type_name": format!("{p}_{l}"), "name": format!("{p}_{l}0"), "use_memo": false }));
        }
        containers.push(jvalue!({
            "type_name": format!("image_store_{l}"),
            "name": format!("store_{l}"),
            "processes": [
                { "type_name": format!("image_store_{l}_put"), "name": format!("store_{l}_put") },
                { "type_name": format!("image_store_{l}_get"), "name": format!("store_{l}_get") }
            ]
        }));
    }
    let call = |process: String, args: Value| jvalue!({ "process": process, "args": args });
    let mut composites = Vec::new();
    // 言語をまたぐ接続。invert(invert(bgr)) は bgr と同じ画像になる
    for (a, b) in [("cpp", "hs"), ("hs", "cpp"), ("cpp", "cpp")] {
        if langs.contains(&a) && langs.contains(&b) {
            composites.push(jvalue!({
                "type_name": format!("bgr_{a}_to_{b}"),
                "body": call(format!("image_invert_{b}"), jvalue!({
                    "img": call(format!("image_invert_{a}"), jvalue!({
                        // Composite では省いた引数に既定値を使わないので書く
                        "img": call(format!("image_bgr_{a}"), jvalue!({
                            "width": { "const": 4 },
                            "height": { "const": 3 }
                        }))
                    }))
                }))
            }));
            processes.push(jvalue!({ "type_name": format!("bgr_{a}_to_{b}"), "name": format!("bgr_{a}_to_{b}0") }));
        }
    }
    let mut components = jvalue!({
        "cpp_image_component": { "language": "c++", "path": "../target/debug" }
    });
    if langs.contains(&"hs") {
        components["image_component_hs"] = jvalue!({ "language": "haskell", "path": "../target" });
    }
    let brokers = match ipc_name {
        Some(n) => jvalue!([{ "type_name": "ipc", "name": n }]),
        None => jvalue!([]),
    };
    jvalue!({
        "name": name,
        "option": { "http_broker": { "start": false } },
        "plugins": { "components": components },
        "brokers": brokers,
        "composites": composites,
        "processes": processes,
        "containers": containers
    })
}

const FORMATS: [&str; 10] = [
    "l8", "la8", "rgb8", "rgba8", "l16", "la16", "rgb16", "rgba16", "rgb32f", "rgba32f",
];

fn image_of_format(format: &str, w: u32, h: u32) -> DynamicImage {
    let b = |x: u32, y: u32| ((x * 7 + y * 13) % 256) as u8;
    let s = |x: u32, y: u32| (x * 300 + y * 7) as u16;
    let f = |x: u32, y: u32| x as f32 / 3.0 - y as f32;
    match format {
        "l8" => DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| Luma([b(x, y)]))),
        "la8" => DynamicImage::ImageLumaA8(ImageBuffer::from_fn(w, h, |x, y| LumaA([b(x, y), 200]))),
        "rgb8" => DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| Rgb([b(x, y), y as u8, 9]))),
        "rgba8" => DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, y| Rgba([b(x, y), 1, 2, 255]))),
        "l16" => DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| Luma([s(x, y)]))),
        "la16" => DynamicImage::ImageLumaA16(ImageBuffer::from_fn(w, h, |x, y| LumaA([s(x, y), 65535]))),
        "rgb16" => DynamicImage::ImageRgb16(ImageBuffer::from_fn(w, h, |x, y| Rgb([s(x, y), 1, 60000]))),
        "rgba16" => DynamicImage::ImageRgba16(ImageBuffer::from_fn(w, h, |x, y| Rgba([s(x, y), 2, 3, 4]))),
        "rgb32f" => DynamicImage::ImageRgb32F(ImageBuffer::from_fn(w, h, |x, y| Rgb([f(x, y), 0.5, -1e-3]))),
        "rgba32f" => DynamicImage::ImageRgba32F(ImageBuffer::from_fn(w, h, |x, y| Rgba([f(x, y), 1.0, 2.0, 1.0]))),
        _ => panic!("unknown format {format}"),
    }
}

/// image_invert_* の結果。8 bit の形式はすべてのバイト (アルファも) を反転し、それ以外はそのまま
fn inverted(img: &DynamicImage) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let bytes = || img.as_bytes().iter().map(|b| 255 - b).collect::<Vec<u8>>();
    match img {
        DynamicImage::ImageLuma8(_) => DynamicImage::ImageLuma8(ImageBuffer::from_raw(w, h, bytes()).unwrap()),
        DynamicImage::ImageLumaA8(_) => DynamicImage::ImageLumaA8(ImageBuffer::from_raw(w, h, bytes()).unwrap()),
        DynamicImage::ImageRgb8(_) => DynamicImage::ImageRgb8(ImageBuffer::from_raw(w, h, bytes()).unwrap()),
        DynamicImage::ImageRgba8(_) => DynamicImage::ImageRgba8(ImageBuffer::from_raw(w, h, bytes()).unwrap()),
        _ => img.clone(),
    }
}

/// image_bgr_* が返す画像 (B = x, G = y, R = 7 の BGR8) を RGB8 にしたもの
fn bgr_expected(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| Rgb([7, y as u8, x as u8])))
}

fn image_args(name: &str, img: &DynamicImage) -> CapsuleMap {
    let mut args = CapsuleMap::new();
    args.insert(name.to_owned(), img.clone().into());
    args
}

/// 言語 l の Process / Container を bp で確かめる
fn check_language(bp: &dyn BrokerProxy, l: &str, large: bool) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let invert = find_process(&ids, &format!("image_invert_{l}0")).clone();
    let bgr = find_process(&ids, &format!("image_bgr_{l}0")).clone();
    let info = find_process(&ids, &format!("image_info_{l}0")).clone();

    for format in FORMATS {
        let img = image_of_format(format, 13, 5);
        let got = bp.process_call(&invert, image_args("img", &img))?.extract_image()?;
        assert_eq!(got, inverted(&img), "{l} {format}");
    }
    if large {
        let hd = image_of_format("rgb8", 1280, 720);
        let got = bp.process_call(&invert, image_args("img", &hd))?.extract_image()?;
        assert_eq!(got, inverted(&hd), "{l} 720p");
    }

    let args: CapsuleMap = vec![("width", jvalue!(5)), ("height", jvalue!(2))].into();
    assert_eq!(bp.process_call(&bgr, args)?.extract_image()?, bgr_expected(5, 2), "{l} bgr");

    let mut args = image_args("img", &image_of_format("la16", 6, 4));
    args.insert("label".to_owned(), jvalue!("cam").into());
    let text = bp.process_call(&info, args)?.extract_value()?;
    assert_eq!(text, jvalue!("cam:6x4:la16"), "{l} info");

    // 画像でない値を画像の引数に渡すとエラー
    let not_image: CapsuleMap = vec![("img", jvalue!(1))].into();
    assert!(bp.process_call(&invert, not_image).is_err(), "{l} not an image");

    // ContainerProcess の画像の引数と戻り値
    let cids = bp.container_process_list(false, None)?;
    let put = find_process(&cids, &format!("store_{l}_put")).clone();
    let get = find_process(&cids, &format!("store_{l}_get")).clone();
    let img = image_of_format("rgba8", 8, 3);
    let n = bp.container_process_call(&put, image_args("img", &img))?.extract_value()?;
    assert!(n.as_i64().unwrap_or(0) >= 1, "{l} put {n}");
    let got = bp.container_process_call(&get, CapsuleMap::new())?.extract_image()?;
    assert_eq!(got, img, "{l} container");
    Ok(())
}

/// 言語をまたぐ接続 (Composite) で画像が渡る
fn check_composites(bp: &dyn BrokerProxy, langs: &[&str]) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    for (a, b) in [("cpp", "hs"), ("hs", "cpp"), ("cpp", "cpp")] {
        if !(langs.contains(&a) && langs.contains(&b)) {
            continue;
        }
        let id: &ProcessIdentifier = find_process(&ids, &format!("bgr_{a}_to_{b}0"));
        let got = bp.process_call(id, CapsuleMap::new())?.extract_image()?;
        assert_eq!(got, bgr_expected(4, 3), "{a} -> {b}");
    }
    Ok(())
}

#[test]
fn plugin_images_in_process() -> JuizResult<()> {
    crate::common::logger::init();
    let langs = languages();
    System::new(conf("plugin_image_test", None, &langs))?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;
            for l in langs.iter() {
                check_language(&*cb, l, false)?;
            }
            check_composites(&*cb, &langs)?;
            Ok(())
        })
}

/// IPC ブローカー越しでも、C++ / Haskell の Process に画像が生バイトで届き、返る
#[test]
fn plugin_images_through_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let langs = languages();
    let ipc_name = "plugin_image_test";
    System::new(conf("plugin_image_ipc_test", Some(ipc_name), &langs))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system.core_broker().lock()?.worker().broker_proxy("ipc", ipc_name, true)?;
            let bp = juiz_borrow(&proxy)?;
            for l in langs.iter() {
                check_language(&*bp, l, true)?;
            }
            check_composites(&*bp, &langs)?;
            Ok(())
        })
}

/// juiz/opencv.h の変換 (to_mat / from_mat) で画像が往復する。OpenCV が無くて例がビルドされていなければ飛ばす
#[test]
fn opencv_mat_conversion() -> JuizResult<()> {
    crate::common::logger::init();
    if !plugin_file("../target/debug", "cpp_image_opencv_component").exists() {
        eprintln!("skip: cpp_image_opencv_component がビルドされていません (OpenCV が無い環境)");
        return Ok(());
    }
    let conf = jvalue!({
        "name": "opencv_image_test",
        "option": { "http_broker": { "start": false } },
        "plugins": { "components": {
            "cpp_image_opencv_component": { "language": "c++", "path": "../target/debug" }
        } },
        "processes": [
            { "type_name": "image_flip_opencv_cpp", "name": "flip0", "use_memo": false },
            { "type_name": "image_crop_opencv_cpp", "name": "crop0", "use_memo": false }
        ]
    });
    System::new(conf)?.setup()?.run_and_do_once(|system| {
        let cb = system.core_broker().lock()?;
        let ids = cb.process_list(false, None)?;
        let flip = find_process(&ids, "flip0").clone();
        let crop = find_process(&ids, "crop0").clone();
        for format in FORMATS {
            let img = image_of_format(format, 9, 6);
            let got = cb.process_call(&flip, image_args("img", &img))?.extract_image()?;
            assert_eq!(got, img.fliph(), "flip {format}");
            // ROI の Mat (行の終わりに余白がある) からも作れる
            let got = cb.process_call(&crop, image_args("img", &img))?.extract_image()?;
            assert_eq!(got, img.crop_imm(1, 1, 7, 4), "crop {format}");
        }
        Ok(())
    })
}
