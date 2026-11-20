//! 実行中のモデルの差し替え (ContainerModel Step 10、issue #389、docs/spec/container-model.md §13.2) のテスト
//!
//! - `container/set_model` を CoreBroker・ローカルブローカー (MessengerBrokerProxy)・HTTP ブローカー (CRUDBrokerProxy) で
//! - 差し替えで SHA-256・ETag が変わる、`null` で外す、値の誤りでは今のモデルを変えない、`path` は断る
//! - HTTP の PATCH の値の誤りは 400
//! - ContainerProcess の口 (ContainerPoseHandle::set_model)、readonly の呼び出し中は断る
//! - サブシステムの Container のモデルを、親の System 越しに差し替える

use std::sync::{Arc, RwLock};

use base64::Engine;
use juiz_core::models::{sha256_hex, ContainerModelData};
use juiz_core::prelude::*;

const PORT: i64 = 8053;
const MASTER_PORT: i64 = 8054;

fn glb_bytes(payload_len: usize) -> Vec<u8> {
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((12 + payload_len) as u32).to_le_bytes());
    v.extend((0..payload_len).map(|i| (i % 251) as u8));
    v
}

const SDF: &str = r#"<sdf version="1.9"><model name="cloud"><link name="l"><visual name="v">
<geometry><sphere><radius>0.1</radius></sphere></geometry></visual></link></model></sdf>"#;

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn manifest(port: i64) -> Value {
    jvalue!({
        "name": "container_model_replace_test",
        "option": { "http_broker": { "start": true, "port": port }, "model": { "max_bytes": 4096 } },
        "containers": [
            { "type_name": "static_anchor", "name": "gripper0" },
            { "type_name": "static_anchor", "name": "cloud0" },
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

fn is_invalid(e: &juiz_sdk::anyhow::Error) -> bool {
    // ブローカー越しでは相手のエラーは文で届く (container-model.md §11)
    matches!(
        e.chain().find_map(|c| c.downcast_ref::<JuizError>()),
        Some(JuizError::ContainerModelInvalidError { .. })
    ) || format!("{e:#}").contains("ContainerModel is invalid")
}

/// どの BrokerProxy でも同じに差し替えられる
fn check_proxy(bp: &dyn BrokerProxy, kind: &str) -> JuizResult<()> {
    let glb = glb_bytes(100 + kind.len());
    let info = bp.container_set_model(
        "gripper0",
        &jvalue!({"data": b64(&glb), "format": "glb", "scale": 0.5}),
    )?;
    assert_eq!(info["format"], "glb", "{kind}: {info}");
    assert_eq!(info["sha256"], sha256_hex(&glb), "{kind}");
    assert_eq!(info["size"], glb.len() as u64);
    assert_eq!(info["file_name"], "model.glb");
    assert_eq!(info["scale"], 0.5);
    assert_eq!(info["up_axis"], "y");
    assert_eq!(bp.container_model_info("gripper0")?, info, "{kind}");
    let d = ContainerModelData::from_value(&bp.container_model("gripper0")?)?;
    assert_eq!(d.data, glb, "{kind}");

    // file_name の拡張子から形式を決める
    let info = bp.container_set_model(
        "cloud0",
        &jvalue!({"data": b64(SDF.as_bytes()), "file_name": "cloud.sdf", "color": "#ff8000"}),
    )?;
    assert_eq!(info["format"], "sdf", "{kind}: {info}");
    assert_eq!(info["media_type"], "application/xml");
    assert_eq!(info["file_name"], "cloud.sdf");
    assert_eq!(info["color"], "#ff8000");

    // 値の誤りは ContainerModelInvalidError。今のモデルは変えない
    for bad in [
        jvalue!({"data": b64(b"not a glb"), "format": "glb"}),
        jvalue!({"data": "!!!", "format": "glb"}),
        jvalue!({"data": b64(&glb)}),
        jvalue!({"data": b64(&glb), "format": "glb", "scale": -1.0}),
        jvalue!({"path": "/etc/passwd", "format": "other"}),
        jvalue!({"data": b64(&glb), "format": "glb", "file_name": "../x.glb"}),
        // option.model.max_bytes (4096) を超える
        jvalue!({"data": b64(&glb_bytes(5000)), "format": "glb"}),
        jvalue!("gripper.glb"),
    ] {
        let e = bp
            .container_set_model("gripper0", &bad)
            .expect_err(&format!("{kind}: {bad}"));
        assert!(is_invalid(&e), "{kind}: {bad}: {e:#}");
    }
    assert_eq!(
        bp.container_model_info("gripper0")?["sha256"],
        sha256_hex(&glb)
    );

    // null で外す
    assert_eq!(
        bp.container_set_model("gripper0", &Value::Null)?,
        Value::Null
    );
    assert_eq!(bp.container_model_info("gripper0")?, Value::Null);
    assert!(bp.container_model("gripper0").is_err(), "{kind}");
    Ok(())
}

fn client() -> JuizResult<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// HTTP: 差し替えで ETag が変わる、値の誤りは 400 (§6.2・§11)
fn check_http(port: i64) -> JuizResult<()> {
    let base = format!("http://localhost:{port}/api");
    let c = client()?;
    let glb = glb_bytes(300);
    let r = c
        .patch(format!("{base}/container/set_model"))
        .query(&[("identifier", "gripper0")])
        .json(&jvalue!({"model": {"data": b64(&glb), "format": "glb"}}))
        .send()?;
    assert_eq!(r.status(), 200);
    let v: Value = r.json()?;
    assert_eq!(v["sha256"], sha256_hex(&glb));
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "gripper0")])
        .send()?;
    assert_eq!(r.status(), 200);
    let etag = format!("\"{}\"", sha256_hex(&glb));
    assert_eq!(r.headers()["etag"], etag.as_str());
    assert_eq!(
        r.headers()["content-disposition"],
        "inline; filename=\"model.glb\""
    );
    assert_eq!(r.bytes()?.to_vec(), glb);

    let glb2 = glb_bytes(400);
    c.patch(format!("{base}/container/set_model"))
        .query(&[("identifier", "gripper0")])
        .json(&jvalue!({"model": {"data": b64(&glb2), "format": "glb"}}))
        .send()?;
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "gripper0")])
        .header("If-None-Match", etag.as_str())
        .send()?;
    assert_eq!(r.status(), 200, "the ETag changes after set_model");
    assert_eq!(r.bytes()?.to_vec(), glb2);
    // system/profile にも載る (管理画面はここから SHA-256 の変化を見る)
    let v: Value = c.get(format!("{base}/system/profile")).send()?.json()?;
    let g = v["core_store"]["containers"]
        .as_array()
        .and_then(|cs| cs.iter().find(|c| c["name"] == "gripper0"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(g["model"]["sha256"], sha256_hex(&glb2), "{g}");

    let r = c
        .patch(format!("{base}/container/set_model"))
        .query(&[("identifier", "gripper0")])
        .json(&jvalue!({"model": {"data": b64(b"xx"), "format": "glb"}}))
        .send()?;
    assert_eq!(r.status(), 400);
    let r = c
        .patch(format!("{base}/container/set_model"))
        .query(&[("identifier", "gripper0")])
        .json(&jvalue!({"model": null}))
        .send()?;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json::<Value>()?, Value::Null);
    let r = c
        .get(format!("{base}/container/model"))
        .query(&[("identifier", "gripper0")])
        .send()?;
    assert_eq!(r.status(), 404);
    Ok(())
}

/// ContainerProcess の口 (§13.2)。readonly の呼び出し中は断る (§14)
fn check_handle(system: &System) -> JuizResult<()> {
    let id: ContainerIdentifier = "core://core/container/gripper0::static_anchor"
        .to_owned()
        .try_into()?;
    let c = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .container_from_identifier(&id)?;
    let handle = c.pose_handle();
    let glb = glb_bytes(500);
    let info = handle.set_model_bytes(&glb, "glb", &jvalue!({"file_name": "closed.glb"}))?;
    assert_eq!(info["sha256"], sha256_hex(&glb));
    assert_eq!(info["file_name"], "closed.glb");
    let cb = system.core_broker().lock()?;
    assert_eq!(
        cb.container_model_info("gripper0")?["sha256"],
        sha256_hex(&glb)
    );
    drop(cb);
    {
        let _scope = juiz_sdk::transformations::ReadonlyScope::enter(&handle);
        let e = handle.clear_model().expect_err("readonly");
        assert!(is_invalid(&e), "{e:#}");
    }
    assert_eq!(handle.clear_model()?, Value::Null);
    assert_eq!(
        system
            .core_broker()
            .lock()?
            .container_model_info("gripper0")?,
        Value::Null
    );
    Ok(())
}

#[test]
fn container_model_replace() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(manifest(PORT))?
        .setup()?
        .run_and_do_once(|system| {
            {
                let cb = system.core_broker().lock()?;
                check_proxy(&*cb, "core")?;
            }
            for kind in ["local", "http"] {
                let bp = proxy(system, kind, PORT)?;
                let bp = juiz_borrow(&bp)?;
                check_proxy(&*bp, kind)?;
            }
            check_http(PORT)?;
            check_handle(system)?;
            Ok(())
        })
}

/// サブシステムの Container のモデルを、親の System 越しに差し替える (§13.2)
#[test]
fn container_model_replace_of_subsystem() -> JuizResult<()> {
    crate::common::logger::init();
    let master = jvalue!({
        "name": "container_model_replace_master",
        "option": { "http_broker": { "start": true, "port": MASTER_PORT } },
    });
    let mut sub = manifest(MASTER_PORT);
    sub["name"] = jvalue!("container_model_replace_sub");
    System::new(master)?.setup()?.run_and_do_once(|master| {
        System::new(sub.clone())?.setup()?.run_and_do_once(|_sub| {
            let ids = master.core_broker().lock()?.container_list(true, None)?;
            let remote = ids
                .iter()
                .find(|id| id.name == "gripper0")
                .unwrap_or_else(|| panic!("gripper0 not in {ids:?}"))
                .to_string();
            let glb = glb_bytes(200);
            {
                let cb = master.core_broker().lock()?;
                let info = cb
                    .container_set_model(&remote, &jvalue!({"data": b64(&glb), "format": "glb"}))?;
                assert_eq!(info["sha256"], sha256_hex(&glb));
                assert_eq!(
                    cb.container_model_info(&remote)?["sha256"],
                    sha256_hex(&glb)
                );
            }
            // 親の HTTP の PATCH からも
            let glb2 = glb_bytes(250);
            let r = client()?
                .patch(format!(
                    "http://localhost:{MASTER_PORT}/api/container/set_model"
                ))
                .query(&[("identifier", remote.as_str())])
                .json(&jvalue!({"model": {"data": b64(&glb2), "format": "glb"}}))
                .send()?;
            assert_eq!(r.status(), 200);
            let r = client()?
                .get(format!(
                    "http://localhost:{MASTER_PORT}/api/container/model"
                ))
                .query(&[("identifier", remote.as_str())])
                .send()?;
            assert_eq!(r.bytes()?.to_vec(), glb2);
            let cb = master.core_broker().lock()?;
            assert_eq!(cb.container_set_model(&remote, &Value::Null)?, Value::Null);
            assert_eq!(cb.container_model_info(&remote)?, Value::Null);
            Ok(())
        })
    })
}

/// 組み込みの Container `frame` (URDF・SDF の取り込みの link、container-model.md §13.1)。
/// プラグイン無しで conf に書け、Transform の端点・モデルの置き場所になり、消せる。ContainerProcess は付けられない
#[test]
fn builtin_frame_container() -> JuizResult<()> {
    crate::common::logger::init();
    let conf = jvalue!({
        "name": "frame_test",
        "containers": [
            { "type_name": "static_anchor", "name": "base_link" },
            { "type_name": "frame", "name": "head", "namespace": "arm" },
        ],
        "transforms": [
            { "source": "base_link", "destination": "arm/head", "translation": {"z": 0.3} },
        ],
    });
    System::new(conf)?.setup()?.run_and_do_once(|system| {
        let id: ContainerIdentifier = "core://core/container/arm/head::frame"
            .to_owned()
            .try_into()?;
        let mut cb = system.core_broker().lock_mut()?;
        let p = juiz_sdk::geometry::PoseStamped::try_from(&Value::from(
            TransformationsBrokerProxy::container_pose(&*cb, "arm/head", "base_link")?,
        ))?;
        assert!((p.pose.position.z - 0.3).abs() < 1e-12, "{p:?}");
        let glb = glb_bytes(10);
        cb.container_set_model("arm/head", &jvalue!({"data": b64(&glb), "format": "glb"}))?;
        assert_eq!(
            cb.container_model_info("arm/head")?["sha256"],
            sha256_hex(&glb)
        );
        let profile = cb.container_profile(&id)?;
        assert_eq!(profile.identifier().type_name, "frame");
        cb.container_destroy(&id)?;
        assert!(cb.container_model_info("arm/head").is_err());
        Ok(())
    })?;
    // ContainerProcess は付けられない
    let conf = jvalue!({
        "name": "frame_test_processes",
        "containers": [
            { "type_name": "frame", "name": "f", "processes": [ { "type_name": "x", "name": "y" } ] },
        ],
    });
    let r = System::new(conf)?.setup().and_then(|mut s| {
        s.run_and_do_once(|system| {
            let ids = system.core_broker().lock()?.container_list(false, None)?;
            assert!(ids.iter().all(|i| i.name != "f"), "{ids:?}");
            Ok(())
        })
    });
    // conf のエラーで止まるか、作られないかのどちらか (他の conf のエラーと同じ扱い)
    let _ = r;
    Ok(())
}
