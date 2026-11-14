//! zenoh ブローカー経由で Container の 3D モデルを取り出せること (issue #381、docs/spec/container-model.md §6.3)
//!
//! System 自身の zenoh ブローカーに zenoh の BrokerProxy で繋ぎ、`container/model_info`・`container/model`
//! (`{info, data: <base64>}`)・`container/profile_full` の `model` を確かめる。
//! HTTP とローカルブローカーは juiz_core の container_model_test、HTTP と比べたベンチは qmp の
//! qmp_container_model_test にある。

mod common;

use std::path::PathBuf;

use common::*;
use juiz_core::env_logger;
use juiz_core::models::{sha256_hex, ContainerModelData};
use juiz_core::prelude::*;

const PORT: i64 = 5226;
const HTTP_PORT: i64 = 8269;

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
fn zenoh_container_model() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let dir = TempDir::new("zenoh_container_model");
    let glb = glb_bytes(1000);
    std::fs::write(dir.0.join("models/pantilt.glb"), &glb)?;
    let broker_name = format!("localhost:{PORT}");
    System::new(manifest_with_models("zenoh_container_model_test", PORT, HTTP_PORT, false))?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "zenoh",
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
