//! Transformations の API (Step 3、issue #370) のテスト
//!
//! 同じ確認を、CoreBroker・ローカルブローカー (MessengerBrokerProxy)・HTTP ブローカー (CRUDBrokerProxy) の
//! BrokerProxy で行う。ブローカーごとに別の木 (`map_<種類>` → `base_<種類>` → `camera_<種類>`) を使う。
//! 数値は仕様 docs/spec/transformations.md §3.5 の例。HTTP の JSON と OpenAPI・type/list も確かめる。
//! qmp は brokers/qmp_broker/tests/qmp_transformations_api_test.rs。

use std::sync::{Arc, Mutex, RwLock};

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, Quaternion, Vector3};
use juiz_sdk::manifests::{PoseListenerManifest, TransformManifest};

const PORT: i64 = 8065;
const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

struct Part {}

fn z90() -> Quaternion {
    Quaternion::new(0.0, 0.0, H, H)
}

fn pose(x: f64, y: f64, z: f64, q: Quaternion) -> Pose {
    Pose::new(Vector3::new(x, y, z), q)
}

fn assert_pose(actual: &Pose, expected: &Pose) {
    assert!(
        actual.approx_eq(expected, EPS),
        "actual={actual:?} expected={expected:?}"
    );
}

fn error_text<T: std::fmt::Debug>(r: JuizResult<T>) -> String {
    format!("{:#}", r.expect_err("must fail"))
}

static RECEIVED: Mutex<Vec<Value>> = Mutex::new(Vec::new());

fn recorder(args: CapsuleMap) -> JuizResult<Capsule> {
    let v = args.get("pose")?.lock_as_value(|v| v.clone())?;
    RECEIVED.lock().unwrap().push(v.clone());
    Ok(v.into())
}

fn register_factories(worker: &mut CoreWorker) -> JuizResult<()> {
    let juiz_sdk::factory::ContainerFactoryStruct(manifest, constructor) =
        juiz_sdk::factory::container_factory(
            ContainerManifest::new("part"),
            |_args: CapsuleMap| Ok(Box::new(Part {})),
        );
    worker
        .store_mut()
        .containers
        .register_factory("part", container_factory_create(manifest, constructor)?)?;
    let m = ProcessManifest::new("pose_recorder").add_object_arg("pose", "", jvalue!({}));
    worker
        .store_mut()
        .processes
        .register_factory("pose_recorder", process_factory_create(m, recorder)?)?;
    worker.create_process_ref(&ProcessManifest::new("pose_recorder").name("recorder"))?;
    Ok(())
}

fn proxy(system: &System, kind: &str) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
    let name = match kind {
        "http" => format!("localhost:{PORT}"),
        _ => "local".to_owned(),
    };
    system
        .core_broker()
        .lock()?
        .worker()
        .broker_proxy(kind, name.as_str(), true)
}

/// Anchor は Container の API で作る。部品は手元で作る
fn build_tree(system: &System, bp: &mut dyn BrokerProxy, k: &str) -> JuizResult<()> {
    let profile = bp.container_create(
        &ContainerManifest::new("static_anchor").name(&format!("map_{k}")),
        CapsuleMap::new(),
    )?;
    assert_eq!(profile.anchor, Some(jvalue!({"kind": "static"})));
    {
        let mut cb = system.core_broker().lock_mut()?;
        for n in ["base", "camera"] {
            cb.worker_mut().create_container_ref(
                "part",
                &format!("{n}_{k}"),
                Vec::<(&str, Value)>::new().into(),
            )?;
        }
    }
    let p = bp.transform_create(
        &TransformManifest::new(&format!("map_{k}"), &format!("base_{k}"))
            .translation(Vector3::new(1.0, 0.0, 0.0))
            .rotation(z90()),
    )?;
    assert_eq!(
        p["identifier"],
        jvalue!(format!(
            "transform://core://core/container/map_{k}::static_anchor|core://core/container/base_{k}::part"
        ))
    );
    bp.transform_create(
        &TransformManifest::new(&format!("base_{k}"), &format!("camera_{k}"))
            .translation(Vector3::new(0.1, 0.0, 0.3)),
    )?;
    Ok(())
}

fn check_pose(bp: &mut dyn BrokerProxy, k: &str) -> JuizResult<()> {
    let (map, base, camera) = (
        format!("map_{k}"),
        format!("base_{k}"),
        format!("camera_{k}"),
    );
    let p = bp.container_pose(&camera, &map)?;
    assert_eq!(p.header.frame_id, map);
    assert_pose(&p.pose, &pose(1.0, 0.1, 0.3, z90()));
    // ID でも指定できる
    let camera_id = format!("core://core/container/{camera}::part");
    assert_pose(
        &bp.container_pose(&camera_id, &base)?.pose,
        &pose(0.1, 0.0, 0.3, Quaternion::identity()),
    );
    assert_pose(
        &bp.container_pose(&map, &camera)?.pose,
        &pose(-0.1, 1.0, -0.3, Quaternion::new(0.0, 0.0, -H, H)),
    );

    // set_pose は親 → C の Transform を書き換える (§3.5 の最後)
    let written = bp.container_set_pose(&camera, &map, &pose(1.0, 0.2, 0.3, z90()))?;
    assert_pose(&written.pose, &pose(1.0, 0.2, 0.3, z90()));
    let tid = format!(
        "transform://core://core/container/{base}::part|core://core/container/{camera}::part"
    );
    let t = bp.transform_profile_full(&tid)?;
    let value: juiz_sdk::geometry::Transform = (&t["transform"]).try_into()?;
    assert!(value.approx_eq(
        &juiz_sdk::geometry::Transform::new(Vector3::new(0.2, 0.0, 0.3), Quaternion::identity()),
        EPS
    ));

    // 根の Anchor・基準自身への書き込みはエラー
    let e = error_text(bp.container_set_pose(&map, &camera, &Pose::identity()));
    assert!(e.contains("can not be written"), "{e}");
    let e = error_text(bp.container_pose(&camera, "no_such_container"));
    assert!(e.contains("no_such_container"), "{e}");
    Ok(())
}

fn check_transform(bp: &mut dyn BrokerProxy, k: &str) -> JuizResult<()> {
    let (map, base, camera) = (
        format!("map_{k}"),
        format!("base_{k}"),
        format!("camera_{k}"),
    );
    let tid = format!(
        "transform://core://core/container/{base}::part|core://core/container/{camera}::part"
    );
    assert!(bp.transform_list()?.contains(&tid));

    // 値の変更 (省いたものは今のまま)
    let p = bp.transform_update(&tid, Some(Vector3::new(0.1, 0.0, 0.3)), None)?;
    assert_eq!(
        p["destination"],
        jvalue!(format!("core://core/container/{camera}::part"))
    );
    assert_pose(
        &bp.container_pose(&camera, &map)?.pose,
        &pose(1.0, 0.1, 0.3, z90()),
    );
    let p = bp.transform_update(&tid, None, Some(z90()))?;
    let value: juiz_sdk::geometry::Transform = (&p["transform"]).try_into()?;
    assert!(value.approx_eq(
        &juiz_sdk::geometry::Transform::new(Vector3::new(0.1, 0.0, 0.3), z90()),
        EPS
    ));

    // 木の規則を破る Transform・同じ Transform はエラー
    let e = error_text(bp.transform_create(&TransformManifest::new(&camera, &map)));
    assert!(e.contains("Transform tree"), "{e}");
    let e = error_text(bp.transform_create(&TransformManifest::new(&base, &camera)));
    assert!(e.contains("already exists"), "{e}");

    // Anchor の一覧・更新・木
    let anchor_id = format!("core://core/container/{map}::static_anchor");
    assert!(bp.anchor_list()?.contains(&anchor_id));
    let u = bp.anchor_update(&map)?;
    assert_eq!(u["identifier"], jvalue!(anchor_id));
    assert!(u["stamp"].is_object(), "{u}");
    let tree = bp.transformations_tree(&map)?;
    assert_eq!(tree["identifier"], jvalue!(anchor_id));
    let base_node = &tree["children"][0];
    assert_eq!(
        base_node["identifier"],
        jvalue!(format!("core://core/container/{base}::part"))
    );
    let camera_node = &base_node["children"][0];
    let camera_pose: Pose = (&camera_node["pose"]).try_into()?;
    // base → camera も z 軸まわり 90° にしたので、map から見ると 180°
    assert_pose(
        &camera_pose,
        &pose(1.0, 0.1, 0.3, Quaternion::new(0.0, 0.0, 1.0, 0.0)),
    );
    assert!(camera_node["transform"].is_object());
    let e = error_text(bp.anchor_update(&base));
    assert!(e.contains("not an Anchor"), "{e}");

    // 消すと木から外れ、読めなくなる
    let p = bp.transform_destroy(&tid)?;
    assert_eq!(p["identifier"], jvalue!(tid));
    assert!(!bp.transform_list()?.contains(&tid));
    let e = error_text(bp.container_pose(&camera, &map));
    assert!(e.contains("No anchor contains"), "{e}");
    let e = error_text(bp.transform_destroy(&tid));
    assert!(e.contains("is not found"), "{e}");
    Ok(())
}

fn check_pose_listener(bp: &mut dyn BrokerProxy, k: &str) -> JuizResult<()> {
    let (map, base) = (format!("map_{k}"), format!("base_{k}"));
    let p =
        bp.pose_listener_create(&PoseListenerManifest::new(&base, "recorder").reference(&map))?;
    let id = p["identifier"].as_str().unwrap().to_owned();
    assert_eq!(
        id,
        format!("pose_listener://core://core/container/{base}::part|core://core/process/recorder::pose_recorder|pose")
    );
    assert!(bp.pose_listener_list()?.contains(&id));
    assert_eq!(bp.pose_listener_profile_full(&id)?["arg"], jvalue!("pose"));
    let e = error_text(bp.pose_listener_create(&PoseListenerManifest::new(&base, "recorder")));
    assert!(e.contains("already exists"), "{e}");

    RECEIVED.lock().unwrap().clear();
    bp.container_set_pose(&base, &map, &pose(2.0, 0.0, 0.0, Quaternion::identity()))?;
    let received = RECEIVED.lock().unwrap().clone();
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(received[0]["header"]["frame_id"], jvalue!(map));
    assert_eq!(received[0]["pose"]["position"]["x"], jvalue!(2.0));

    assert_eq!(bp.pose_listener_destroy(&id)?["identifier"], jvalue!(id));
    assert!(!bp.pose_listener_list()?.contains(&id));
    Ok(())
}

fn check_all(system: &System, kind: &str) -> JuizResult<()> {
    let proxy = proxy(system, kind)?;
    let mut bp = juiz_write(&proxy)?;
    build_tree(system, &mut *bp, kind)?;
    check_pose(&mut *bp, kind)?;
    check_transform(&mut *bp, kind)?;
    check_pose_listener(&mut *bp, kind)?;
    Ok(())
}

/// HTTP の JSON (クエリ・ボディの形)、型の検査、OpenAPI と type/list
fn check_http_json(system: &System) -> JuizResult<()> {
    let k = "json";
    {
        let mut cb = system.core_broker().lock_mut()?;
        let w = cb.worker_mut();
        w.create_container_ref(
            "static_anchor",
            "map_json",
            Vec::<(&str, Value)>::new().into(),
        )?;
        w.create_container_ref("part", "base_json", Vec::<(&str, Value)>::new().into())?;
    }
    let base_url = format!("http://localhost:{PORT}/api");
    let client = reqwest::blocking::Client::new();
    let r = client
        .post(format!("{base_url}/transform/create"))
        .json(&jvalue!({"source": "map_json", "destination": "base_json", "translation": {"x": 1.0, "y": 0.0, "z": 0.0}}))
        .send()?;
    assert_eq!(r.status(), 200);
    let p: Value = client
        .get(format!(
            "{base_url}/container/pose?identifier=base_json&reference=map_json"
        ))
        .send()?
        .json()?;
    assert_eq!(p["pose"]["position"]["x"], jvalue!(1.0));
    assert_eq!(p["header"]["frame_id"], jvalue!("map_json"));
    let r = client
        .patch(format!("{base_url}/container/set_pose?identifier=base_json"))
        .json(&jvalue!({"reference": "map_json", "pose": {"position": {"x": 3.0, "y": 0.0, "z": 0.0}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}}}))
        .send()?;
    assert_eq!(r.status(), 200);
    let t: Value = client
        .get(format!("{base_url}/transformations/tree?anchor=map_json"))
        .send()?
        .json()?;
    assert_eq!(t["children"][0]["pose"]["position"]["x"], jvalue!(3.0));
    // 型に合わない値はエラー (juiz_std.geometry.Pose)
    let r = client
        .patch(format!(
            "{base_url}/container/set_pose?identifier=base_json"
        ))
        .json(&jvalue!({"reference": "map_json", "pose": {"position": "here"}}))
        .send()?;
    assert_eq!(r.status(), 500);
    let body = r.text()?;
    assert!(body.contains("juiz_std.geometry.Pose"), "{body}");
    let _ = k;

    // OpenAPI に値の型が載る
    let api: Value = client
        .get(format!("http://localhost:{PORT}/api-docs/openapi.json"))
        .send()?
        .json()?;
    assert!(api["components"]["schemas"]["juiz_std.geometry.PoseStamped"].is_object());
    assert_eq!(
        api.pointer(
            "/paths/~1api~1container~1pose/get/responses/200/content/application~1json/schema/$ref"
        ),
        Some(&jvalue!(
            "#/components/schemas/juiz_std.geometry.PoseStamped"
        ))
    );
    // type/list に載る
    let types: Value = client.get(format!("{base_url}/type/list")).send()?.json()?;
    let names: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for n in [
        "juiz_std.geometry.Pose",
        "juiz_std.geometry.PoseStamped",
        "juiz_std.geometry.Transform",
        "juiz_std.geometry.TransformStamped",
    ] {
        assert!(names.contains(&n), "{n} not in {names:?}");
    }
    Ok(())
}

#[test]
fn transformations_api() -> JuizResult<()> {
    crate::common::logger::init();
    Ok(System::new(jvalue!({
        "name": "transformations_api_test",
        "option": { "http_broker": { "start": true, "port": PORT } },
    }))?
    .setup()?
    .run_and_do_once(|system| {
        register_factories(system.core_broker().lock_mut()?.worker_mut())?;
        check_all(system, "local")?;
        check_all(system, "http")?;
        check_http_json(system)?;
        Ok(())
    })
    .expect("Error in System::run_and_do()"))
}
