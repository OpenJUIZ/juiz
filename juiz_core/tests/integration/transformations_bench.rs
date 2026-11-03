//! Transformations の木の更新時間のベンチマーク (issue #369)
//!
//! StaticAnchor の下に 1000 個の Container を並べた木 (各ノードが 10 個の子を持つ) を作り、
//! - 根の直下の Transform を変えたときの更新 (全ノードの計算し直し)
//! - 何も変わっていないときの更新 (版番号で全部飛ばす)
//!
//! の 1 回あたりの時間を測る。時間がかかるので `#[ignore]` にしてある。
//!
//! ```sh
//! cargo test --release -p juiz_core --test integration -- transformations_bench --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Quaternion, Vector3};
use juiz_sdk::manifests::TransformManifest;

const NODES: usize = 1000;
const ROUNDS: usize = 50;

struct Part {}

fn best_of(mut f: impl FnMut(usize) -> JuizResult<()>) -> JuizResult<Duration> {
    let mut best = Duration::MAX;
    for i in 0..ROUNDS {
        let start = Instant::now();
        f(i)?;
        best = best.min(start.elapsed());
    }
    Ok(best)
}

#[test]
#[ignore]
fn transformations_bench_1000_nodes() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(jvalue!({
        "name": "tf_bench",
        "option": { "http_broker": { "start": false } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let juiz_sdk::factory::ContainerFactoryStruct(manifest, constructor) =
            juiz_sdk::factory::container_factory(
                ContainerManifest::new("part"),
                |_args: CapsuleMap| Ok(Box::new(Part {})),
            );
        worker
            .store_mut()
            .containers
            .register_factory("part", container_factory_create(manifest, constructor)?)?;
        worker.create_container_ref("static_anchor", "map", Vec::<(&str, Value)>::new().into())?;
        let mut transforms = Vec::new();
        for i in 0..NODES {
            worker.create_container_ref(
                "part",
                &format!("n{i}"),
                Vec::<(&str, Value)>::new().into(),
            )?;
            // n0 が map の子、n(i) の親は n((i - 1) / 10) (子を 10 個ずつ持つ木)
            let parent = if i == 0 {
                "map".to_owned()
            } else {
                format!("n{}", (i - 1) / 10)
            };
            transforms.push(
                TransformManifest::new(&parent, &format!("n{i}"))
                    .translation(Vector3::new(0.1, 0.0, 0.0))
                    .rotation(Quaternion::from_rpy(0.0, 0.0, 0.01)),
            );
        }
        let reg = worker.store().transformations.clone();
        let start = Instant::now();
        reg.create_transforms(&transforms, "")?;
        println!(
            "create {NODES} transforms (with tree check and first update): {:?}",
            start.elapsed()
        );
        let map_id = reg.anchor_list()?[0].clone();
        let n0 = worker.container_from_typename_and_name("part", "n0")?;
        let tid = juiz_sdk::manifests::transform_identifier(&map_id, &n0.identifier().to_string());
        let changed = best_of(|i| {
            reg.update_transform(&tid, Some(Vector3::new(i as f64, 0.0, 0.0)), None)
                .map(|_| ())
        })?;
        let unchanged = best_of(|_| reg.update_anchor("map", "").map(|_| ()))?;
        println!("update after the root transform changed: {changed:?}");
        println!("update with nothing changed: {unchanged:?}");
        Ok(())
    })
}
