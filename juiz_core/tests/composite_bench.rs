//! Composite と接続の比較のベンチマーク (issue #169)
//!
//! 同じ画像パイプライン `mean(invert(flip(make_image(seed))))` を
//! - direct: juiz を通さずに関数を直接呼ぶ (下限の目安)
//! - composite: 4 段を 1 つの Composite にする
//! - connected: 4 つの Process を Pull 接続でつなぐ
//!
//! で作り、1 回の呼び出しにかかる時間を測る。毎回 seed を p_apply してから execute するので、memo は効かない。
//! Python では、何もしない段 (identity) を k 段はさんだ Composite の時間の増え方から、
//! 1 段あたりの受け渡し (Capsule と PIL の画像の変換、つまりシリアライズ) の時間を求める。
//!
//! 時間がかかるので `#[ignore]` にしてある。次のように release で実行する。
//!
//! ```sh
//! cargo build --release   # ../target/release ではなく、Python の例は tests/python から読む
//! PYTHONPATH=bindings/pyjuiz cargo test --release -p juiz_core --test composite_bench -- --ignored --nocapture --test-threads=1
//! ```
use std::path::Path;
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::connections::ConnectionManifest;
use juiz_sdk::connections::ConnectionType;
use juiz_sdk::image::{DynamicImage, Rgb, RgbImage};
use juiz_sdk::manifests::CompositeManifest;
use juiz_sdk::process_identifier::ProcessIdentifier;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const WARMUP: i64 = 3;
const ROUNDS: i64 = 5;

/// `f(seed)` を `n` 回呼んだときの 1 回あたりの時間。
/// ばらつきを除くため `ROUNDS` 回測って一番短いものを返す
fn time_per_call(n: i64, mut f: impl FnMut(i64) -> JuizResult<()>) -> JuizResult<Duration> {
    for i in 0..WARMUP {
        f(i)?;
    }
    let mut best = Duration::MAX;
    for round in 0..ROUNDS {
        let start = Instant::now();
        for i in 0..n {
            f(WARMUP + round * n + i)?;
        }
        best = best.min(start.elapsed() / n as u32);
    }
    Ok(best)
}

fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1000.0)
}

/// 毎回 seed を p_apply してから execute する (Composite と接続で同じ呼び方にする)
fn p_apply_and_execute(source: &ProcessPtr, sink: &ProcessPtr, seed: i64) -> JuizResult<f64> {
    source.lock_mut()?.p_apply("seed", jvalue!(seed).into())?;
    sink.lock()?
        .execute()?
        .lock_as_value(|v| v.as_f64().expect("mean must be a number"))
}

// ---- Rust の段 ----

fn rust_make_image(seed: i64) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(WIDTH, HEIGHT, |x, y| {
        Rgb([
            (x as i64 + seed) as u8,
            (y as i64 + seed) as u8,
            (x ^ y) as u8,
        ])
    }))
}

fn rust_invert(img: &DynamicImage) -> DynamicImage {
    let mut out = img.clone();
    out.invert();
    out
}

fn rust_mean(img: &DynamicImage) -> f64 {
    let bytes = img.as_bytes();
    bytes.iter().map(|b| *b as u64).sum::<u64>() as f64 / bytes.len() as f64
}

fn make_image(args: CapsuleMap) -> JuizResult<Capsule> {
    let seed = args.get("seed")?.lock_as_value(|v| v.as_i64().unwrap())?;
    Ok(rust_make_image(seed).into())
}

fn flip(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("img")?.lock_as_image(|img| img.fliph())?.into())
}

fn invert(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("img")?.lock_as_image(rust_invert)?.into())
}

fn mean(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(args.get("img")?.lock_as_image(rust_mean)?).into())
}

fn register_rust_factories(worker: &mut CoreWorker) -> JuizResult<()> {
    let image_arg = |type_name: &str| {
        ProcessManifest::new(type_name)
            .use_memo(false)
            .add_image_arg("img", "input image")
    };
    let factories: Vec<(
        ProcessManifest,
        juiz_sdk::processes::ProcessBodyFunctionType,
    )> = vec![
        (
            ProcessManifest::new("make_image")
                .use_memo(false)
                .add_int_arg("seed", "seed", 0),
            make_image,
        ),
        (image_arg("flip"), flip),
        (image_arg("invert"), invert),
        (image_arg("mean"), mean),
    ];
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        worker
            .store_mut()
            .processes
            .register_factory(&type_name, pf)?;
    }
    Ok(())
}

/// `stages` を内側から順に重ねた Composite (`make_image` が一番内側)
fn pipeline_composite(
    type_name: &str,
    make: &str,
    stages: &[&str],
) -> JuizResult<CompositeManifest> {
    let mut body = jvalue!({ "process": make });
    for stage in stages {
        body = jvalue!({ "process": stage, "args": { "img": body } });
    }
    Ok(serde_json::from_value(
        jvalue!({ "type_name": type_name, "body": body }),
    )?)
}

fn connect(worker: &mut CoreWorker, chain: &[&ProcessPtr]) -> JuizResult<()> {
    for pair in chain.windows(2) {
        worker.create_connection(&ConnectionManifest::new(
            ConnectionType::Pull,
            pair[0].identifier(),
            "img".to_owned(),
            pair[1].identifier(),
        ))?;
    }
    Ok(())
}

#[test]
#[ignore]
fn bench_rust_image_pipeline() -> JuizResult<()> {
    let n = 200;
    System::new(jvalue!({
        "name": "composite_bench_rust",
        "option": { "http_broker": { "start": false } },
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let (comp, chain) = {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            register_rust_factories(worker)?;
            worker.register_composite(&pipeline_composite(
                "rust_pipeline",
                "make_image",
                &["flip", "invert", "mean"],
            )?)?;
            let comp = worker.create_process_ref(
                &ProcessManifest::new("rust_pipeline")
                    .name("comp0")
                    .use_memo(false),
            )?;
            let mut chain = Vec::new();
            for type_name in ["make_image", "flip", "invert", "mean"] {
                chain.push(worker.create_process_ref(
                    &ProcessManifest::new(type_name).name(&format!("{type_name}0")),
                )?);
            }
            connect(worker, &chain.iter().collect::<Vec<&ProcessPtr>>())?;
            (comp, chain)
        };

        let direct = time_per_call(n, |seed| {
            let img = rust_make_image(seed);
            std::hint::black_box(rust_mean(&rust_invert(&img.fliph())));
            Ok(())
        })?;
        let composite = time_per_call(n, |seed| {
            p_apply_and_execute(&comp, &comp, seed).map(|_| ())
        })?;
        let connected = time_per_call(n, |seed| {
            p_apply_and_execute(&chain[0], &chain[3], seed).map(|_| ())
        })?;
        // 同じ値になることを確かめる
        assert_eq!(
            p_apply_and_execute(&comp, &comp, 7)?,
            p_apply_and_execute(&chain[0], &chain[3], 7)?
        );

        println!("\n## Rust: mean(invert(flip(make_image))) {WIDTH}x{HEIGHT} RGB, {n} 回の平均の {ROUNDS} 回中の最短");
        println!("| 方式 | 1 回 [ms] |");
        println!("|---|---|");
        println!("| direct (juiz なし) | {} |", ms(direct));
        println!("| composite | {} |", ms(composite));
        println!("| connected (Pull) | {} |", ms(connected));
        Ok(())
    })
}

/// pyjuiz を PYTHONPATH に入れておく (scripts/test.sh と同じ)
fn python_option() -> Value {
    jvalue!({
        "http_broker": { "start": false },
        "pythonpath": ["../../../bindings/pyjuiz"]
    })
}

fn py_plugins() -> Value {
    let mut factories = serde_json::Map::new();
    for name in [
        "py_bench_make_image",
        "py_bench_flip",
        "py_bench_invert",
        "py_bench_identity",
        "py_bench_mean",
    ] {
        factories.insert(
            name.to_owned(),
            jvalue!({ "language": "python", "path": "python" }),
        );
    }
    jvalue!({ "process_factories": factories })
}

/// Python の画像パイプラインを conf だけで作る (composites:、processes:、connections:)
fn py_bench_conf() -> JuizResult<Value> {
    let identity = |k: usize| {
        let mut stages = vec!["py_bench_identity"; k];
        stages.push("py_bench_mean");
        stages
    };
    let composites = vec![
        pipeline_composite(
            "py_pipeline",
            "py_bench_make_image",
            &["py_bench_flip", "py_bench_invert", "py_bench_mean"],
        )?,
        pipeline_composite("py_identity0", "py_bench_make_image", &identity(0))?,
        pipeline_composite("py_identity4", "py_bench_make_image", &identity(4))?,
    ];
    let chain = [
        "py_bench_make_image",
        "py_bench_flip",
        "py_bench_invert",
        "py_bench_mean",
    ];
    let processes: Vec<Value> = ["py_pipeline", "py_identity0", "py_identity4"]
        .iter()
        .map(|t| jvalue!({ "type_name": t, "name": format!("{t}_0"), "use_memo": false }))
        .chain(
            chain
                .iter()
                .map(|t| jvalue!({ "type_name": t, "name": format!("{t}_0"), "use_memo": false })),
        )
        .collect();
    let connections: Vec<Value> = chain
        .windows(2)
        .map(|pair| {
            jvalue!({
                "connection_type": "Pull",
                "source": { "type_name": pair[0], "name": format!("{}_0", pair[0]) },
                "destination": { "type_name": pair[1], "name": format!("{}_0", pair[1]) },
                "arg_name": "img"
            })
        })
        .collect();
    Ok(jvalue!({
        "name": "composite_bench_python",
        "option": python_option(),
        "plugins": py_plugins(),
        "composites": serde_json::to_value(composites)?,
        "processes": processes,
        "connections": connections,
    }))
}

/// 同じパイプラインを Python だけで実行したときの 1 回あたりの時間 (juiz を通さない)
fn python_direct(n: i64) -> Option<Duration> {
    let script = format!(
        r#"
import sys, time
sys.path[0:0] = ["tests/python", "../bindings/pyjuiz"]
from py_bench_make_image import py_bench_make_image as make
from py_bench_flip import py_bench_flip as flip
from py_bench_invert import py_bench_invert as invert
from py_bench_mean import py_bench_mean as mean
for i in range({WARMUP}):
    mean(invert(flip(make(i))))
best = None
for r in range({ROUNDS}):
    start = time.perf_counter()
    for i in range({n}):
        mean(invert(flip(make(i))))
    t = (time.perf_counter() - start) / {n}
    best = t if best is None else min(best, t)
print(best)
"#
    );
    let python = std::env::var("PYO3_PYTHON").unwrap_or("python3".to_owned());
    let out = std::process::Command::new(python)
        .arg("-c")
        .arg(script)
        .output()
        .ok()?;
    if !out.status.success() {
        eprintln!(
            "python direct failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    // pyjuiz が import 時に出力する行があるので、最後の行を読む
    let stdout = String::from_utf8_lossy(&out.stdout);
    let secs: f64 = stdout.lines().last()?.trim().parse().ok()?;
    Some(Duration::from_secs_f64(secs))
}

#[test]
#[ignore]
fn bench_python_image_pipeline() -> JuizResult<()> {
    let n = 50;
    let direct = python_direct(n);
    System::new(py_bench_conf()?)?
        .set_working_dir(Path::new("tests"))
        .setup()?
        .run_and_do_once(|system| {
            let process = |t: &str| -> JuizResult<ProcessPtr> {
                let id: ProcessIdentifier = format!("core://core/process/{t}_0::{t}").try_into()?;
                system
                    .core_broker()
                    .lock()?
                    .worker()
                    .store()
                    .processes
                    .get(&id.to_string())
                    .cloned()
            };
            let comp = process("py_pipeline")?;
            let id0 = process("py_identity0")?;
            let id4 = process("py_identity4")?;
            let source = process("py_bench_make_image")?;
            let sink = process("py_bench_mean")?;

            let composite = time_per_call(n, |seed| {
                p_apply_and_execute(&comp, &comp, seed).map(|_| ())
            })?;
            let connected = time_per_call(n, |seed| {
                p_apply_and_execute(&source, &sink, seed).map(|_| ())
            })?;
            let identity0 =
                time_per_call(n, |seed| p_apply_and_execute(&id0, &id0, seed).map(|_| ()))?;
            let identity4 =
                time_per_call(n, |seed| p_apply_and_execute(&id4, &id4, seed).map(|_| ()))?;
            assert_eq!(
                p_apply_and_execute(&comp, &comp, 7)?,
                p_apply_and_execute(&source, &sink, 7)?
            );

            // 何もしない段 1 つあたりの時間 = 画像を Python に渡して受け取る時間 (変換と呼び出し)
            let per_stage = identity4.saturating_sub(identity0) / 4;
            // パイプラインの 4 段は、画像を 3 回渡して 3 回受け取る (make の出力、flip・invert の入出力、mean の入力)
            let serialization = per_stage * 3;

            println!(
                "\n## Python: mean(invert(flip(make_image))) {WIDTH}x{HEIGHT} RGB, {n} 回の平均の {ROUNDS} 回中の最短"
            );
            println!("| 方式 | 1 回 [ms] |");
            println!("|---|---|");
            match direct {
                Some(d) => println!("| direct (Python だけ、juiz なし) | {} |", ms(d)),
                None => println!("| direct (Python だけ、juiz なし) | (測れなかった) |"),
            }
            println!("| composite | {} |", ms(composite));
            println!("| connected (Pull) | {} |", ms(connected));
            println!("| composite: make → mean | {} |", ms(identity0));
            println!(
                "| composite: make → identity × 4 → mean | {} |",
                ms(identity4)
            );
            println!(
                "| identity 1 段あたり (画像の受け渡し) | {} |",
                ms(per_stage)
            );
            println!(
                "| composite のうち画像の受け渡しの見積もり (3 段分) | {} ({:.0}%) |",
                ms(serialization),
                serialization.as_secs_f64() / composite.as_secs_f64() * 100.0
            );
            Ok(())
        })
}
