//! Python のプラグインとの画像の受け渡しのテストとベンチマーク (issue #270)
//!
//! - Rust の画像を Python の Process に渡すと、画素形式に合った PIL のモードになり、画素が変わらない
//!   (以前は 'RGB' 決め打ちで、グレースケールや RGBA が崩れていた)
//! - Python の Process が返した PIL の画像を Rust で読むと、元の画像と同じになる
//! - ベンチ: 720p / 1080p / 4K の RGB 画像を Python に渡す時間と、Python から受け取る時間
//!
//! ベンチは時間がかかるので `#[ignore]` にしてある。次のように release で実行する。
//!
//! ```sh
//! PYTHONPATH=bindings/pyjuiz cargo test --release -p juiz_core --test integration -- python_image_test --ignored --nocapture --test-threads=1
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{DynamicImage, ImageBuffer, Luma, LumaA, Rgb, Rgba};
use juiz_sdk::manifests::{CompositeArgument, CompositeManifest, CompositeNode};

const PY_PROCESSES: [&str; 3] = ["py_image_probe", "py_image_width", "py_bench_identity"];

fn conf() -> Value {
    let mut factories = serde_json::Map::new();
    for name in PY_PROCESSES {
        factories.insert(
            name.to_owned(),
            jvalue!({ "language": "python", "path": "python" }),
        );
    }
    jvalue!({
        "name": "python_image_test",
        "option": {
            "http_broker": { "start": false },
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": { "process_factories": factories },
    })
}

/// 形式ごとの小さな画像。画素は位置から決まる
fn small_image(format: &str, w: u32, h: u32) -> DynamicImage {
    let v = |x: u32, y: u32, c: u32| ((x * 40 + y * 70 + c * 25) % 256) as u8;
    let v16 = |x: u32, y: u32| (x * 4000 + y * 9000 + 7) as u16;
    match format {
        "l8" => DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| Luma([v(x, y, 0)]))),
        "la8" => DynamicImage::ImageLumaA8(ImageBuffer::from_fn(w, h, |x, y| {
            LumaA([v(x, y, 0), v(x, y, 3)])
        })),
        "rgb8" => DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([v(x, y, 0), v(x, y, 1), v(x, y, 2)])
        })),
        "rgba8" => DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([v(x, y, 0), v(x, y, 1), v(x, y, 2), v(x, y, 3)])
        })),
        "l16" => DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| Luma([v16(x, y)]))),
        "rgb16" => DynamicImage::ImageRgb16(ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([v16(x, y), v16(x, y) / 2, v16(x, y) / 3])
        })),
        _ => panic!("unknown format {format}"),
    }
}

/// ベンチ用の RGB 画像。大きさごとに 1 度だけ作る
fn bench_image(w: u32, h: u32) -> DynamicImage {
    static CACHE: OnceLock<Mutex<HashMap<(u32, u32), DynamicImage>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache
        .lock()
        .unwrap()
        .entry((w, h))
        .or_insert_with(|| small_image("rgb8", w, h))
        .clone()
}

fn rust_image(args: CapsuleMap) -> JuizResult<Capsule> {
    let format = args.get("format")?.lock_as_str(|s| s.to_owned())?;
    let int = |name: &str| -> JuizResult<u32> {
        args.get(name)?.lock_as_value(|v| v.as_i64().unwrap_or(0) as u32)
    };
    let (w, h) = (int("width")?, int("height")?);
    Ok(if format == "bench" {
        bench_image(w, h)
    } else {
        small_image(&format, w, h)
    }
    .into())
}

/// rust_image を登録し、`stage(rust_image(format, w, h))` の Composite の Process を作る
fn composite(system: &System, stage: &str, format: &str, w: u32, h: u32) -> JuizResult<ProcessPtr> {
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    if !worker.store().processes.factories().contains_key("rust_image") {
        let pf = process_factory_create(
            ProcessManifest::new("rust_image")
                .add_string_arg("format", "image format", "rgb8")
                .add_int_arg("width", "width", 4)
                .add_int_arg("height", "height", 3),
            rust_image,
        )?;
        worker.store_mut().processes.register_factory("rust_image", pf)?;
    }
    let source = CompositeNode::new("rust_image")
        .arg("format", CompositeArgument::Const(jvalue!(format)))
        .arg("width", CompositeArgument::Const(jvalue!(w)))
        .arg("height", CompositeArgument::Const(jvalue!(h)));
    let type_name = format!("{stage}_{format}_{w}x{h}");
    let body = if stage == "rust_image" {
        source
    } else {
        CompositeNode::new(stage).arg("img", CompositeArgument::Node(Box::new(source)))
    };
    worker.register_composite(&CompositeManifest::new(&type_name, body))?;
    worker.create_process_ref(
        &ProcessManifest::new(&type_name)
            .name(&format!("{type_name}0"))
            .use_memo(false),
    )
}

fn with_system(f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?
        .set_working_dir(Path::new("tests"))
        .setup()?
        .run_and_do_once(f)
}

/// Rust の画像を Python に渡すと、形式に合った PIL のモードになり、画素が変わらない
#[test]
fn rust_image_reaches_python_with_its_mode() -> JuizResult<()> {
    with_system(|system| {
        // getpixel((0, 0)) と getpixel((3, 2)) を Python の書き方にしたもの
        let rgb16_as_8 = small_image("rgb16", 4, 3).to_rgb8();
        let rgb = |p: &Rgb<u8>| format!("({}, {}, {})", p[0], p[1], p[2]);
        let cases = [
            ("l8", "L 4x3 0 4".to_owned()),
            ("la8", "LA 4x3 (0, 75) (4, 79)".to_owned()),
            ("rgb8", "RGB 4x3 (0, 25, 50) (4, 29, 54)".to_owned()),
            ("rgba8", "RGBA 4x3 (0, 25, 50, 75) (4, 29, 54, 79)".to_owned()),
            ("l16", "I;16 4x3 7 30007".to_owned()),
            // PIL に 16 ビットの RGB のモードは無いので、8 ビットにして渡す
            (
                "rgb16",
                format!(
                    "RGB 4x3 {} {}",
                    rgb(rgb16_as_8.get_pixel(0, 0)),
                    rgb(rgb16_as_8.get_pixel(3, 2))
                ),
            ),
        ];
        for (format, expected) in cases {
            let p = composite(system, "py_image_probe", format, 4, 3)?;
            let got = p.lock()?.call(CapsuleMap::new())?.lock_as_str(|s| s.to_owned())?;
            assert_eq!(got, expected, "{format}");
        }
        Ok(())
    })
}

/// Python が返した PIL の画像を Rust で読むと、元の画像と同じになる
#[test]
fn python_image_returns_to_rust_unchanged() -> JuizResult<()> {
    with_system(|system| {
        for format in ["l8", "la8", "rgb8", "rgba8", "l16"] {
            let p = composite(system, "py_bench_identity", format, 4, 3)?;
            let got = p.lock()?.call(CapsuleMap::new())?.extract_image()?;
            assert_eq!(got, small_image(format, 4, 3), "{format}");
        }
        // 16 ビットの RGB は 8 ビットになって戻る
        let p = composite(system, "py_bench_identity", "rgb16", 4, 3)?;
        let got = p.lock()?.call(CapsuleMap::new())?.extract_image()?;
        assert_eq!(got, DynamicImage::ImageRgb8(small_image("rgb16", 4, 3).to_rgb8()));
        Ok(())
    })
}

const WARMUP: usize = 2;
const ROUNDS: usize = 5;

/// 1 回あたりの時間。ばらつきを除くため `ROUNDS` 回測って一番短いものを返す
fn time_per_call(mut f: impl FnMut() -> JuizResult<()>) -> JuizResult<Duration> {
    for _ in 0..WARMUP {
        f()?;
    }
    let mut best = Duration::MAX;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        f()?;
        best = best.min(start.elapsed());
    }
    Ok(best)
}

fn ms(d: Duration) -> String {
    format!("{:.2}", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore]
fn bench_python_image_conversion() -> JuizResult<()> {
    with_system(|system| {
        println!("\n## Python との画像の受け渡し (RGB, {ROUNDS} 回中の最短 [ms])");
        println!("| | Rust の Process だけ | Python へ渡す (幅を返す) | Python へ渡して受け取る | 渡す分 | 受け取る分 |");
        println!("|---|---|---|---|---|---|");
        for (w, h, label) in [(1280, 720, "720p"), (1920, 1080, "1080p"), (3840, 2160, "4K")] {
            let source = composite(system, "rust_image", "bench", w, h)?;
            let to_python = composite(system, "py_image_width", "bench", w, h)?;
            let round_trip = composite(system, "py_bench_identity", "bench", w, h)?;
            let t_source = time_per_call(|| {
                source.lock()?.call(CapsuleMap::new())?.lock_as_image(|i| i.width())?;
                Ok(())
            })?;
            let t_to = time_per_call(|| {
                let width = to_python
                    .lock()?
                    .call(CapsuleMap::new())?
                    .lock_as_value(|v| v.as_i64())?;
                assert_eq!(width, Some(w as i64));
                Ok(())
            })?;
            let t_round = time_per_call(|| {
                let img = round_trip.lock()?.call(CapsuleMap::new())?.extract_image()?;
                assert_eq!(img.width(), w);
                Ok(())
            })?;
            println!(
                "| {label} | {} | {} | {} | {} | {} |",
                ms(t_source),
                ms(t_to),
                ms(t_round),
                ms(t_to.saturating_sub(t_source)),
                ms(t_round.saturating_sub(t_to)),
            );
        }
        Ok(())
    })
}
