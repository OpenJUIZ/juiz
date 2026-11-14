//! Container の 3D モデル (ContainerModel Step 1、issue #381、仕様 docs/spec/container-model.md) のテスト
//!
//! - conf の `model:` (conf 相対のパス、作成時の検査、option.model.max_bytes)
//! - `container/model_info`・`container/model`・`container/profile_full` の `model` を、
//!   CoreBroker・ローカルブローカー (MessengerBrokerProxy)・HTTP ブローカー (CRUDBrokerProxy) で
//! - HTTP の生のバイト列・ETag・304・404、API からの作成に `model` を付けるとエラー
//! - サブシステムの Container のモデルを、親の System 越しに取り出す
//! - type/list と OpenAPI
//!
//! qmp は brokers/qmp_broker/tests/qmp_container_model_test.rs。

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use juiz_core::models::{sha256_hex, ContainerModelData};
use juiz_core::prelude::*;

const PORT: i64 = 8067;

/// テスト用の一時ディレクトリ (conf の親ディレクトリにする)
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!(
            "juiz_{name}_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(p.join("models")).unwrap();
        TempDir(p)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
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

fn binary_stl(triangles: u32) -> Vec<u8> {
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&triangles.to_le_bytes());
    v.extend(std::iter::repeat(0u8).take(50 * triangles as usize));
    v
}

const SDF: &str = r#"<sdf version="1.9"><model name="floor"><link name="l"><visual name="v">
<geometry><box><size>10 10 0.01</size></box></geometry></visual></link></model></sdf>"#;

fn model_files(dir: &TempDir) {
    dir.write("models/pantilt.glb", &glb_bytes(1000));
    dir.write("models/camera.stl", &binary_stl(4));
    dir.write("models/floor.sdf", SDF.as_bytes());
}

fn manifest(port: i64) -> Value {
    jvalue!({
        "name": "container_model_test",
        "option": { "http_broker": { "start": true, "port": port } },
        "containers": [
            { "type_name": "static_anchor", "name": "pantilt0", "model": { "path": "models/pantilt.glb" } },
            {
                "type_name": "static_anchor", "name": "camera0", "namespace": "robot1",
                "model": {
                    "path": "models/camera.stl", "scale": 0.001, "color": "#404040",
                    "origin": { "translation": { "z": -0.02 } },
                },
            },
            { "type_name": "static_anchor", "name": "floor", "model": { "path": "models/floor.sdf" } },
            { "type_name": "static_anchor", "name": "plain" },
        ],
        "composite_containers": [
            { "name": "cc0", "slots": { "a": "plain" }, "model": { "path": "models/pantilt.glb" } },
        ],
    })
}

fn proxy(system: &System, kind: &str, port: i64) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
    let name = match kind {
        "http" => format!("localhost:{port}"),
        _ => "local".to_owned(),
    };
    system
        .core_broker()
        .lock()?
        .worker()
        .broker_proxy(kind, name.as_str(), true)
}

fn juiz_error_kind(e: &juiz_sdk::anyhow::Error) -> String {
    match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
        Some(JuizError::ContainerModelInvalidError { .. }) => "invalid".to_owned(),
        Some(JuizError::ContainerModelFileNotFoundError { .. }) => "not_found".to_owned(),
        Some(JuizError::ContainerHasNoModelError { .. }) => "no_model".to_owned(),
        _ => format!("other: {e:#}"),
    }
}

/// どの BrokerProxy でも同じに見える
fn check_proxy(bp: &dyn BrokerProxy, kind: &str) -> JuizResult<()> {
    let glb = glb_bytes(1000);
    let info = bp.container_model_info("pantilt0")?;
    assert_eq!(info["format"], "glb", "{kind}: {info}");
    assert_eq!(info["media_type"], "model/gltf-binary");
    assert_eq!(info["size"], glb.len() as u64);
    assert_eq!(info["sha256"], sha256_hex(&glb));
    assert_eq!(info["file_name"], "pantilt.glb");
    assert_eq!(info["up_axis"], "y");

    // ID でも名前でも引ける。ネームスペースの下は `robot1/camera0`
    let by_id = bp.container_model_info("core://core/container/pantilt0::static_anchor")?;
    assert_eq!(by_id, info, "{kind}");
    let camera = bp.container_model_info("robot1/camera0")?;
    assert_eq!(camera["format"], "stl");
    assert_eq!(camera["scale"], 0.001);
    assert_eq!(camera["color"], "#404040");
    assert_eq!(camera["up_axis"], "z");
    assert_eq!(camera["origin"]["translation"]["z"], -0.02);

    // 中身は JSON の経路の形 (base64)
    let d = ContainerModelData::from_value(&bp.container_model("pantilt0")?)?;
    assert_eq!(d.data, glb, "{kind}");
    assert_eq!(d.info, info);
    let d = ContainerModelData::from_value(&bp.container_model("floor")?)?;
    assert_eq!(d.data, SDF.as_bytes());
    assert_eq!(d.media_type(), "application/xml");

    // CompositeContainer もモデルを持てる
    assert_eq!(bp.container_model_info("cc0")?["format"], "glb", "{kind}");

    // モデルの無い Container
    assert_eq!(bp.container_model_info("plain")?, Value::Null, "{kind}");
    let e = bp.container_model("plain").unwrap_err();
    assert!(format!("{e:#}").contains("has no model"), "{kind}: {e:#}");
    // 無い Container はエラー
    assert!(bp.container_model_info("nothing").is_err());

    // profile_full に載る。無ければ null
    let id = ContainerIdentifier::try_from("core://core/container/pantilt0::static_anchor".to_owned())?;
    assert_eq!(bp.container_profile_full(&id)?.model, Some(info.clone()), "{kind}");
    let plain = ContainerIdentifier::try_from("core://core/container/plain::static_anchor".to_owned())?;
    assert_eq!(bp.container_profile_full(&plain)?.model, None, "{kind}");
    Ok(())
}

fn client() -> JuizResult<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// HTTP の生のバイト列 (§6.2)
fn check_http_raw(dir: &TempDir, port: i64) -> JuizResult<()> {
    let base = format!("http://localhost:{port}/api");
    let c = client()?;
    let glb = glb_bytes(1000);
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .send()?;
    assert_eq!(r.status(), 200);
    let h = r.headers().clone();
    assert_eq!(h["content-type"], "model/gltf-binary");
    assert_eq!(h["content-length"], glb.len().to_string().as_str());
    let etag = format!("\"{}\"", sha256_hex(&glb));
    assert_eq!(h["etag"], etag.as_str());
    assert_eq!(h["cache-control"], "no-cache");
    assert_eq!(h["content-disposition"], "inline; filename=\"pantilt.glb\"");
    assert_eq!(r.bytes()?.to_vec(), glb);

    // 自分を指す http の ID でも引ける
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[(
            "identifier",
            format!("http://localhost:{port}/container/pantilt0::static_anchor").as_str(),
        )])
        .send()?;
    assert_eq!(r.status(), 200);

    // If-None-Match が同じなら 304 (本文なし、ETag は付ける)
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .header("If-None-Match", etag.as_str())
        .send()?;
    assert_eq!(r.status(), 304);
    assert_eq!(r.headers()["etag"], etag.as_str());
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .header("If-None-Match", "\"0000\", *")
        .send()?;
    assert_eq!(r.status(), 304);
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .header("If-None-Match", "\"0000\"")
        .send()?;
    assert_eq!(r.status(), 200);

    // STL・SDF の Content-Type
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "robot1/camera0")])
        .send()?;
    assert_eq!(r.headers()["content-type"], "model/stl");
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "floor")])
        .send()?;
    assert_eq!(r.headers()["content-type"], "application/xml");
    assert_eq!(r.text()?, SDF);

    // model_info は JSON。モデルが無ければ null
    let v: Value = c
        .get(format!("{base}/container/model_info"))
        .query(&[("identifier", "pantilt0")])
        .send()?
        .json()?;
    assert_eq!(v["sha256"], sha256_hex(&glb));
    let v: Value = c
        .get(format!("{base}/container/model_info"))
        .query(&[("identifier", "plain")])
        .send()?
        .json()?;
    assert_eq!(v, Value::Null);
    // profile_full に model
    let v: Value = c
        .get(format!("{base}/container/profile_full"))
        .query(&[("identifier", "core://core/container/pantilt0::static_anchor")])
        .send()?
        .json()?;
    assert_eq!(v["model"]["format"], "glb", "{v}");
    let v: Value = c
        .get(format!("{base}/container/profile_full"))
        .query(&[("identifier", "core://core/container/plain::static_anchor")])
        .send()?
        .json()?;
    assert!(v.get("model").is_some_and(|m| m.is_null()), "{v}");

    // モデルの無い Container は 404
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "plain")])
        .send()?;
    assert_eq!(r.status(), 404);

    // ファイルを書き換えると ETag も変わる。消すと 404、戻すと元どおり
    let path = dir.0.join("models/pantilt.glb");
    let glb2 = glb_bytes(2000);
    std::fs::write(&path, &glb2)?;
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .header("If-None-Match", etag.as_str())
        .send()?;
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["etag"], format!("\"{}\"", sha256_hex(&glb2)).as_str());
    assert_eq!(r.bytes()?.to_vec(), glb2);
    std::fs::remove_file(&path)?;
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .send()?;
    assert_eq!(r.status(), 404);
    let r = c
        .get(format!("{base}/container/model_info"))
        .query(&[("identifier", "pantilt0")])
        .send()?;
    assert_eq!(r.status(), 404);
    std::fs::write(&path, &glb)?;
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "pantilt0")])
        .send()?;
    assert_eq!(r.status(), 200);
    assert_eq!(r.bytes()?.to_vec(), glb);
    Ok(())
}

/// API からの作成に model は付けられない (§5.5)
fn check_create_rejects_model(system: &System, port: i64) -> JuizResult<()> {
    let c = client()?;
    let r = c
        .post(format!("http://localhost:{port}/api/container/create"))
        .json(&jvalue!({ "__manifest__": {
            "type_name": "static_anchor", "name": "evil",
            "model": { "path": "/etc/passwd", "format": "other" },
        }}))
        .send()?;
    assert_eq!(r.status(), 400, "{:?}", r.text());
    // manifest の外に書いても同じ
    let r = c
        .post(format!("http://localhost:{port}/api/container/create"))
        .json(&jvalue!({
            "__manifest__": { "type_name": "static_anchor", "name": "evil1" },
            "model": { "path": "/etc/passwd", "format": "other" },
        }))
        .send()?;
    assert_eq!(r.status(), 400, "{:?}", r.text());
    let r = c
        .post(format!("http://localhost:{port}/api/composite_container/create"))
        .json(&jvalue!({ "__manifest__": {
            "name": "evil_cc", "slots": { "a": "plain" },
            "model": { "path": "/etc/passwd", "format": "other" },
        }}))
        .send()?;
    assert_eq!(r.status(), 400, "{:?}", r.text());

    let proxy = proxy(system, "local", port)?;
    let mut bp = juiz_write(&proxy)?;
    let m = ContainerManifest::new("static_anchor")
        .name("evil2")
        .model(juiz_sdk::manifests::ModelManifest::new("/etc/passwd"));
    // ローカルブローカー越しのエラーは文で届く
    let e = bp.container_create(&m, CapsuleMap::new()).unwrap_err();
    assert!(format!("{e:#}").contains("only be written in the conf"), "{e:#}");
    let e = system
        .core_broker()
        .lock_mut()?
        .container_create(&m.clone().name("evil3"), CapsuleMap::new())
        .unwrap_err();
    assert_eq!(juiz_error_kind(&e), "invalid");
    // どれも作られていない
    let names = bp
        .container_list(false, None)?
        .into_iter()
        .map(|id| id.name)
        .collect::<Vec<_>>();
    assert!(!names.iter().any(|n| n.starts_with("evil")), "{names:?}");
    Ok(())
}

/// type/list と OpenAPI (§8)
fn check_types(port: i64) -> JuizResult<()> {
    let c = client()?;
    let types: Value = c
        .get(format!("http://localhost:{port}/api/type/list"))
        .send()?
        .json()?;
    let entry = types
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "juiz_std.model.ContainerModelInfo")
        .unwrap_or_else(|| panic!("ContainerModelInfo is not in type/list: {types}"));
    assert_eq!(entry["used_by"], jvalue!(["container_model"]));
    let api: Value = c
        .get(format!("http://localhost:{port}/api-docs/openapi.json"))
        .send()?
        .json()?;
    assert!(
        api.pointer("/components/schemas/juiz_std.model.ContainerModelInfo")
            .is_some(),
        "no schema"
    );
    let r = api
        .pointer("/paths/~1api~1container~1model_info/get/responses/200/content/application~1json/schema")
        .expect("model_info response");
    assert_eq!(
        r["oneOf"][0]["$ref"],
        "#/components/schemas/juiz_std.model.ContainerModelInfo"
    );
    assert!(api
        .pointer("/paths/~1api~1container~1model/get/responses/200/content/model~1gltf-binary")
        .is_some());
    Ok(())
}

#[test]
fn container_model_api() -> JuizResult<()> {
    crate::common::logger::init();
    let dir = TempDir::new("container_model_api");
    model_files(&dir);
    Ok(System::new(manifest(PORT))?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            {
                let cb = system.core_broker().lock()?;
                check_proxy(&*cb, "core")?;
            }
            for kind in ["local", "http"] {
                let proxy = proxy(system, kind, PORT)?;
                let bp = juiz_borrow(&proxy)?;
                check_proxy(&*bp, kind)?;
            }
            check_http_raw(&dir, PORT)?;
            check_create_rejects_model(system, PORT)?;
            check_types(PORT)?;
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

/// conf を読んで setup したときのエラー (ContainerModel の作成時の検査、§5.2)
fn setup_error(dir: &Path, containers: Value, option: Value) -> String {
    let r = System::new(jvalue!({
        "name": "container_model_conf_error_test",
        "option": option,
        "containers": containers,
    }))
    .and_then(|s| s.set_working_dir(dir).setup());
    match r {
        Ok(_) => panic!("setup must fail"),
        Err(e) => juiz_error_kind(&e),
    }
}

#[test]
fn container_model_conf_errors() -> JuizResult<()> {
    crate::common::logger::init();
    let dir = TempDir::new("container_model_conf");
    model_files(&dir);
    dir.write("models/fake.glb", b"this is not a glb file");
    let anchor = |model: Value| jvalue!([{ "type_name": "static_anchor", "name": "a", "model": model }]);
    let no_http = jvalue!({ "http_broker": { "start": false } });
    assert_eq!(
        setup_error(&dir.0, anchor(jvalue!({"path": "models/none.glb"})), no_http.clone()),
        "not_found"
    );
    assert_eq!(
        setup_error(&dir.0, anchor(jvalue!({"path": "models/fake.glb"})), no_http.clone()),
        "invalid"
    );
    assert_eq!(
        setup_error(&dir.0, anchor(jvalue!({"path": "models/pantilt.glb", "scale": -1})), no_http.clone()),
        "invalid"
    );
    assert_eq!(
        setup_error(&dir.0, anchor(jvalue!({"path": "models/pantilt.glb", "format": "stl"})), no_http.clone()),
        "invalid"
    );
    // 上限 (option.model.max_bytes)
    assert_eq!(
        setup_error(
            &dir.0,
            anchor(jvalue!({"path": "models/pantilt.glb"})),
            jvalue!({ "http_broker": { "start": false }, "model": { "max_bytes": 100 } })
        ),
        "invalid"
    );
    // conf のディレクトリからの相対で解決する (カレントディレクトリではない)
    let other = TempDir::new("container_model_conf_other");
    assert_eq!(
        setup_error(&other.0, anchor(jvalue!({"path": "models/pantilt.glb"})), no_http.clone()),
        "not_found"
    );
    Ok(())
}

/// サブシステムの Container のモデルを、親の System 越しに取り出す (§6.1)
#[test]
fn container_model_of_subsystem() -> JuizResult<()> {
    crate::common::logger::init();
    const MASTER_PORT: i64 = 8068;
    let dir = TempDir::new("container_model_sub");
    model_files(&dir);
    let master = jvalue!({
        "name": "container_model_master",
        "option": { "http_broker": { "start": true, "port": MASTER_PORT } },
    });
    let mut sub = manifest(MASTER_PORT);
    sub["name"] = jvalue!("container_model_sub");
    System::new(master)?.setup()?.run_and_do_once(|master| {
        // 同じポートを指定すると次のポートで待ち受け、先の juiz のサブシステムとして登録される
        System::new(sub.clone())?
            .set_working_dir(&dir.0)
            .setup()?
            .run_and_do_once(|_sub| {
                let ids = master.core_broker().lock()?.container_list(true, None)?;
                let remote = ids
                    .iter()
                    .find(|id| id.name == "pantilt0")
                    .unwrap_or_else(|| panic!("pantilt0 not in {ids:?}"))
                    .to_string();
                assert!(remote.starts_with("http://"), "{remote}");
                let glb = glb_bytes(1000);
                // CoreBroker 越し
                {
                    let cb = master.core_broker().lock()?;
                    assert_eq!(cb.container_model_info(&remote)?["sha256"], sha256_hex(&glb));
                    let d = ContainerModelData::from_value(&cb.container_model(&remote)?)?;
                    assert_eq!(d.data, glb);
                }
                // 親の HTTP からは生のバイト列
                let r = client()?
                    .get(format!("http://localhost:{MASTER_PORT}/api/container/model"))
                    .query(&[("identifier", remote.as_str())])
                    .send()?;
                assert_eq!(r.status(), 200);
                assert_eq!(r.headers()["etag"], format!("\"{}\"", sha256_hex(&glb)).as_str());
                assert_eq!(r.headers()["content-type"], "model/gltf-binary");
                assert_eq!(r.bytes()?.to_vec(), glb);
                let v: Value = client()?
                    .get(format!("http://localhost:{MASTER_PORT}/api/container/model_info"))
                    .query(&[("identifier", remote.as_str())])
                    .send()?
                    .json()?;
                assert_eq!(v["file_name"], "pantilt.glb");
                Ok(())
            })
    })
}
