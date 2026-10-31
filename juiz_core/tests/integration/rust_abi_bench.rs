//! Rust プラグインの C ABI と今までの Rust ABI の呼び出しの時間を比べるベンチマーク (issue #333)
//!
//! 同じプラグインを conf の `abi` だけ変えて 2 回読み込み、CoreBroker の process_call で
//! - small: increment_process に整数を渡す (呼び出し自体のオーバーヘッド)
//! - 720p / 4K: image_echo_proc に RGB の画像を渡し、そのまま返してもらう (画像の受け渡し)
//!
//! の 1 回あたりの時間を測る。C ABI は引数と戻り値を JZF1 のフレームで運ぶので、画像は行きと帰りで 1 回ずつ複製される。
//! 目安として、画像を 1 回複製するだけの時間も出す。
//!
//! 時間がかかるので `#[ignore]` にしてある。次のように release で実行する。
//!
//! ```sh
//! cargo build --release -p increment_process -p image_echo_proc
//! cargo test --release -p juiz_core --test integration -- rust_abi_bench --ignored --nocapture --test-threads=1
//! ```
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::image::{DynamicImage, RgbImage};
use juiz_sdk::process_identifier::ProcessIdentifier;

const PLUGIN_DIR: &str = "../target/release";
const ROUNDS: u32 = 5;

/// `f` を `n` 回呼んだときの 1 回あたりの時間。`ROUNDS` 回測って一番短いものを返す
fn time_per_call(n: u32, mut f: impl FnMut() -> JuizResult<()>) -> JuizResult<Duration> {
    for _ in 0..n.min(10) {
        f()?;
    }
    let mut best = Duration::MAX;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        for _ in 0..n {
            f()?;
        }
        best = best.min(start.elapsed() / n);
    }
    Ok(best)
}

fn us(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1e6)
}

fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1e3)
}

fn system_conf(abi: &str) -> Value {
    jvalue!({
        "name": "rust_abi_bench",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": PLUGIN_DIR, "abi": abi },
                "image_echo_proc": { "path": PLUGIN_DIR, "abi": abi },
            }
        },
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "image_echo_proc", "name": "echo0" },
        ],
    })
}

fn image(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        juiz_sdk::image::Rgb([x as u8, y as u8, (x ^ y) as u8])
    }))
}

struct Row {
    small: Duration,
    images: Vec<Duration>,
}

const SIZES: [(&str, u32, u32, u32); 2] = [("720p", 1280, 720, 200), ("4K", 3840, 2160, 30)];

fn measure(abi: &str) -> JuizResult<Row> {
    let mut row = None;
    System::new(system_conf(abi))?
        .setup()?
        .run_and_do_once(|system| {
            let inc: ProcessIdentifier =
                "core://core/process/inc0::increment_process".try_into()?;
            let echo: ProcessIdentifier =
                "core://core/process/echo0::image_echo_proc".try_into()?;
            let small = time_per_call(20000, || {
                let r = system
                    .core_broker()
                    .lock_mut()?
                    .process_call(&inc, vec![("arg1", jvalue!(41))].into())?;
                assert_eq!(r.lock_as_value(|v| v.as_i64())?, Some(42));
                Ok(())
            })?;
            let mut images = Vec::new();
            for (_, w, h, n) in SIZES {
                let img = image(w, h);
                images.push(time_per_call(n, || {
                    let mut args = CapsuleMap::new();
                    args.insert("img".to_owned(), Capsule::from(img.clone()).into());
                    let r = system.core_broker().lock_mut()?.process_call(&echo, args)?;
                    r.lock_as_image(|r| assert_eq!(r.width(), w))?;
                    Ok(())
                })?);
            }
            row = Some(Row { small, images });
            Ok(())
        })?;
    Ok(row.expect("measured"))
}

#[test]
#[ignore = "ベンチマーク。release で --ignored を付けて実行する"]
fn rust_abi_bench() -> JuizResult<()> {
    crate::common::logger::init();
    let c = measure("c")?;
    let rust = measure("rust")?;
    let clone: Vec<Duration> = SIZES
        .iter()
        .map(|(_, w, h, n)| {
            let img = image(*w, *h);
            time_per_call(*n, || {
                std::hint::black_box(img.clone());
                Ok(())
            })
            .unwrap()
        })
        .collect();

    println!("\n## Rust プラグインの呼び出し (CoreBroker の process_call、{ROUNDS} 回中の最短の 1 回あたり)\n");
    println!("|  | C ABI | Rust ABI | 差 | 画像の複製 1 回 |");
    println!("|---|---|---|---|---|");
    println!(
        "| small [µs] | {} | {} | {} | - |",
        us(c.small),
        us(rust.small),
        us(c.small.saturating_sub(rust.small))
    );
    for (i, (name, ..)) in SIZES.iter().enumerate() {
        println!(
            "| {name} [ms] | {} | {} | {} | {} |",
            ms(c.images[i]),
            ms(rust.images[i]),
            ms(c.images[i].saturating_sub(rust.images[i])),
            ms(clone[i])
        );
    }
    Ok(())
}
