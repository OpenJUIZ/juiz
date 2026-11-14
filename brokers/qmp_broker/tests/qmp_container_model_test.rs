//! qmp ブローカー経由で Container の 3D モデルを取り出せること (issue #381、docs/spec/container-model.md §6.3)
//!
//! System 自身の qmp ブローカーに qmp の BrokerProxy で繋ぎ、`container/model_info`・`container/model`
//! (`{info, data: <base64>}`)・`container/profile_full` の `model` を確かめる。
//! HTTP とローカルブローカーは juiz_core の container_model_test。
//!
//! 50 MB の .glb を HTTP (生のバイト列) と qmp (base64 の JSON) で取り出すベンチは `container_model_bench`
//! (`#[ignore]`、release で測る):
//!
//! ```sh
//! cargo test --release -p juiz_qmp_broker --test qmp_container_model_test -- --ignored --nocapture
//! ```

mod common;

use std::path::PathBuf;
use std::time::Instant;

use common::*;
use juiz_core::env_logger;
use juiz_core::models::{sha256_hex, ContainerModelData};
use juiz_core::prelude::*;

const PORT: i64 = 5026;
const HTTP_PORT: i64 = 8069;
const BENCH_PORT: i64 = 5027;
const BENCH_HTTP_PORT: i64 = 8088;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("juiz_{name}_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(p.join("models")).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn glb_bytes(payload_len: usize) -> Vec<u8> {
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((12 + payload_len) as u32).to_le_bytes());
    v.extend((0..payload_len).map(|i| (i % 251) as u8));
    v
}

fn manifest_with_models(name: &str, port: i64, http_port: i64, http: bool) -> Value {
    let mut m = server_manifest(name, port, http_port, true);
    m["option"]["http_broker"]["start"] = jvalue!(http);
    m["containers"] = jvalue!([
        { "type_name": "static_anchor", "name": "pantilt0", "model": { "path": "models/pantilt.glb" } },
        { "type_name": "static_anchor", "name": "plain" },
    ]);
    m
}

#[test]
fn qmp_container_model() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let dir = TempDir::new("qmp_container_model");
    let glb = glb_bytes(1000);
    std::fs::write(dir.0.join("models/pantilt.glb"), &glb)?;
    let broker_name = format!("localhost:{PORT}");
    System::new(manifest_with_models("qmp_container_model_test", PORT, HTTP_PORT, false))?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "qmp",
                broker_name.as_str(),
                true,
            )?;
            let bp = juiz_borrow(&proxy)?;
            let info = bp.container_model_info("pantilt0")?;
            assert_eq!(info["format"], "glb");
            assert_eq!(info["sha256"], sha256_hex(&glb));
            assert_eq!(info["file_name"], "pantilt.glb");
            let d = ContainerModelData::from_value(&bp.container_model("pantilt0")?)?;
            assert_eq!(d.data, glb);
            assert_eq!(d.info, info);
            assert_eq!(bp.container_model_info("plain")?, Value::Null);
            let e = bp.container_model("plain").unwrap_err();
            assert!(format!("{e:#}").contains("has no model"), "{e:#}");
            let id = ContainerIdentifier::try_from(
                "core://core/container/pantilt0::static_anchor".to_owned(),
            )?;
            assert_eq!(bp.container_profile_full(&id)?.model, Some(info));
            Ok(())
        })
}

/// 50 MB の .glb を HTTP (生のバイト列) と qmp (base64 の JSON) で取り出す時間
#[test]
#[ignore]
fn container_model_bench() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let dir = TempDir::new("container_model_bench");
    let size = 50 * 1000 * 1000;
    let glb = glb_bytes(size - 12);
    std::fs::write(dir.0.join("models/pantilt.glb"), &glb)?;
    let sha = sha256_hex(&glb);
    let broker_name = format!("localhost:{BENCH_PORT}");
    const N: usize = 5;
    // release で測るときは qmp のプラグインも release のものを読む (他のプラグインは使わない)
    let mut manifest = manifest_with_models("container_model_bench", BENCH_PORT, BENCH_HTTP_PORT, true);
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let plugin_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target").join(profile);
    manifest["plugins"] = jvalue!({
        "broker_factories": { "qmp_broker": { "path": plugin_dir.to_string_lossy() } }
    });
    manifest["processes"] = jvalue!([]);
    manifest["ecs"] = jvalue!([]);
    System::new(manifest)?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?;
            let url = format!("http://localhost:{BENCH_HTTP_PORT}/api/container/model");
            let mut http = Vec::new();
            for _ in 0..N {
                let t = Instant::now();
                let r = client.get(&url).query(&[("identifier", "pantilt0")]).send()?;
                let bytes = r.bytes()?;
                http.push(t.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(bytes.len(), glb.len());
            }
            let t = Instant::now();
            let r = client
                .get(&url)
                .query(&[("identifier", "pantilt0")])
                .header("If-None-Match", format!("\"{sha}\""))
                .send()?;
            let not_modified = t.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(r.status(), 304);

            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "qmp",
                broker_name.as_str(),
                true,
            )?;
            let bp = juiz_borrow(&proxy)?;
            let mut qmp = Vec::new();
            for _ in 0..N {
                let t = Instant::now();
                let d = ContainerModelData::from_value(&bp.container_model("pantilt0")?)?;
                qmp.push(t.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(d.data.len(), glb.len());
            }
            let t = Instant::now();
            let info = bp.container_model_info("pantilt0")?;
            let info_ms = t.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(info["sha256"], sha.as_str());

            let median = |v: &mut Vec<f64>| {
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                v[v.len() / 2]
            };
            println!("container/model 50 MB glb (median of {N}):");
            println!("  HTTP raw            : {:8.1} ms", median(&mut http));
            println!("  HTTP 304            : {:8.1} ms", not_modified);
            println!("  qmp JSON (base64)   : {:8.1} ms", median(&mut qmp));
            println!("  qmp model_info      : {:8.1} ms", info_ms);
            Ok(())
        })
}
