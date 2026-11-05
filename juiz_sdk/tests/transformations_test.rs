//! Transformations の SDK 側 (PoseState・ContainerPoseHandle・manifest) のテスト (issue #368)

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::Duration;

use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Time, Vector3};
use juiz_sdk::manifests::{
    pose_listener_identifier, transform_identifier, AnchorKind, DEFAULT_ANCHOR_RATE,
};
use juiz_sdk::prelude::*;
use juiz_sdk::transformations::{ContainerPoseHandle, PoseState, TransformationsRegistry};

const ANCHOR: &str = "core://core/container/map::periodic_anchor";

/// 登録所の代わり。基準 "map" だけを扱い、書かれた値をそのまま PoseState に書く。
/// 本物 (core、Step 2) と同じく Container の利用者の状態のロックは取らない。
/// ContainerPtr は Send でないので、別スレッド (Periodic Anchor) から使う登録所は ContainerPoseHandle を持つ
struct FakeRegistry {
    containers: Mutex<HashMap<String, ContainerPoseHandle>>,
    calls: Mutex<Vec<String>>,
}

impl FakeRegistry {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            containers: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn register(self: &Arc<Self>, c: &ContainerPtr) {
        let weak: Weak<dyn TransformationsRegistry> =
            Arc::downgrade(&(self.clone() as Arc<dyn TransformationsRegistry>));
        c.attach_transformations(weak).unwrap();
        self.containers
            .lock()
            .unwrap()
            .insert(c.identifier().to_string(), c.pose_handle());
    }

    fn state_of(&self, id: &ContainerIdentifier) -> juiz_sdk::transformations::PoseStatePtr {
        self.containers.lock().unwrap()[&id.to_string()].state()
    }
}

impl TransformationsRegistry for FakeRegistry {
    fn pose(&self, container: &ContainerIdentifier, reference: &str) -> JuizResult<PoseStamped> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("pose({}, {reference})", container.name));
        if reference != "map" {
            return Err(anyhow::anyhow!(JuizError::NoCommonAnchorError {
                container: container.to_string(),
                reference: reference.to_owned()
            }));
        }
        let state = self.state_of(container);
        let s = state.read().unwrap();
        let e = s
            .get(ANCHOR)
            .copied()
            .unwrap_or(juiz_sdk::transformations::PoseEntry {
                pose: Pose::identity(),
                stamp: Time::default(),
                version: 0,
            });
        Ok(PoseStamped::new(reference, e.stamp, e.pose))
    }

    fn set_pose(
        &self,
        container: &ContainerIdentifier,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("set_pose({}, {reference})", container.name));
        let stamp = Time::now();
        self.state_of(container)
            .write()
            .unwrap()
            .set(ANCHOR, *pose, stamp);
        self.pose(container, reference)
    }

    fn lookup(
        &self,
        caller: &ContainerIdentifier,
        target: &str,
        reference: &str,
    ) -> JuizResult<PoseStamped> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("lookup({}, {target}, {reference})", caller.name));
        Ok(PoseStamped::new(reference, Time::new(7, 0), z90(0.5, 0.0)))
    }
}

struct MobileBase {
    odom_count: i64,
}

fn base0() -> ContainerPtr {
    ContainerPtr::new(
        ContainerImpl::new(
            ContainerManifest::new("mobile_base").name("base0"),
            Box::new(MobileBase { odom_count: 0 }),
        )
        .unwrap(),
    )
}

fn z90(x: f64, y: f64) -> Pose {
    Pose::new(
        Vector3::new(x, y, 0.0),
        Quaternion::new(0.0, 0.0, 0.7071067811865476, 0.7071067811865476),
    )
}

#[test]
fn pose_state_versions_and_removal() {
    let mut s = PoseState::new();
    assert!(s.is_empty());
    assert_eq!(s.last_version(), 0);
    let v1 = s.set("a", Pose::identity(), Time::new(1, 0));
    let v2 = s.set("b", z90(1.0, 0.0), Time::new(2, 0));
    let v3 = s.set("a", z90(2.0, 0.0), Time::new(3, 0));
    assert!(v1 < v2 && v2 < v3);
    assert_eq!(s.get("a").unwrap().version, v3);
    assert_eq!(s.get("a").unwrap().pose.position.x, 2.0);
    assert_eq!(s.anchors(), vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(s.remove("b").unwrap().version, v2);
    assert_eq!(s.len(), 1);
    s.clear();
    assert!(s.get("a").is_none());
    assert_eq!(s.last_version(), v3);
}

#[test]
fn container_impl_and_ptr_share_pose_state() -> JuizResult<()> {
    let ptr = base0();
    let state = ptr.pose_state();
    state
        .write()
        .unwrap()
        .set(ANCHOR, z90(1.0, 2.0), Time::new(5, 0));

    // ContainerImpl から見ても同じ値
    let guard = ptr.lock()?;
    let c = guard.downcast_ref::<ContainerImpl<MobileBase>>().unwrap();
    let e = c.pose_state().read().unwrap().get(ANCHOR).copied().unwrap();
    assert_eq!(e.pose, z90(1.0, 2.0));
    assert!(Arc::ptr_eq(&state, &c.pose_state()));
    assert!(Arc::ptr_eq(&state, &c.pose_handle().state()));
    // 読み出しだけの口も同じ PoseState を見る
    assert_eq!(
        c.pose_reader().entry(ANCHOR)?.unwrap().stamp,
        Time::new(5, 0)
    );
    Ok(())
}

#[test]
fn pose_without_registry_is_an_error() {
    let ptr = base0();
    let err = ptr.pose("map").unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::TransformationsNotAttachedError { .. })
    ));
    let err = ptr.set_pose("map", &Pose::identity()).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::TransformationsNotAttachedError { .. })
    ));

    // 登録所が消えたら (弱い参照) つながっていない扱い
    let registry = FakeRegistry::new();
    registry.register(&ptr);
    assert!(ptr.pose_handle().is_attached());
    registry.containers.lock().unwrap().clear();
    drop(registry);
    assert!(!ptr.pose_handle().is_attached());
    assert!(ptr.pose("map").is_err());
}

#[test]
fn container_process_reads_and_writes_through_registry() -> JuizResult<()> {
    let ptr = base0();
    let registry = FakeRegistry::new();
    registry.register(&ptr);

    // ContainerProcess と同じく、Container の書き込みロックを持ったまま読み書きする
    {
        let mut guard = ptr.lock_mut()?;
        let c = guard.downcast_mut::<ContainerImpl<MobileBase>>().unwrap();
        c.odom_count += 1;
        let written = c.set_pose("map", z90(1.0, 0.0))?;
        assert_eq!(written.reference(), "map");
        assert!(written.pose.approx_eq(&z90(1.0, 0.0), 1e-9));
        let read = c.pose("map")?;
        assert!(read.pose.approx_eq(&z90(1.0, 0.0), 1e-9));

        // 書き込みロックを持っている間でも、ContainerPtr からは Container のロックなしで読める
        let (tx, rx) = mpsc::channel();
        let other = ptr.pose_handle();
        std::thread::spawn(move || tx.send(other.pose("map").map(|p| p.pose)).unwrap());
        let from_other_thread = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("pose() must not wait for the Container lock")?;
        assert!(from_other_thread.approx_eq(&z90(1.0, 0.0), 1e-9));
    }

    // エラーは登録所のもの
    let err = ptr.pose("odom").unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::NoCommonAnchorError { .. })
    ));
    assert_eq!(
        *registry.calls.lock().unwrap(),
        vec![
            "set_pose(base0, map)",
            "pose(base0, map)",
            "pose(base0, map)",
            "pose(base0, map)",
            "pose(base0, odom)",
        ]
    );
    Ok(())
}

#[test]
fn container_process_looks_up_another_container() -> JuizResult<()> {
    let ptr = base0();
    let registry = FakeRegistry::new();
    registry.register(&ptr);
    {
        // readonly ContainerProcess と同じく、読み出しロックのまま読める
        let guard = ptr.lock()?;
        let c = guard.downcast_ref::<ContainerImpl<MobileBase>>().unwrap();
        let p = c.lookup("lidar0", "robot0")?;
        assert_eq!(p.reference(), "robot0");
        assert!(p.pose.approx_eq(&z90(0.5, 0.0), 1e-9));
        let r = c.pose_reader().lookup("lidar0", "map")?;
        assert_eq!(r.reference(), "map");
    }
    ptr.lookup("lidar0", "robot0")?;
    assert_eq!(
        *registry.calls.lock().unwrap(),
        vec![
            "lookup(base0, lidar0, robot0)",
            "lookup(base0, lidar0, map)",
            "lookup(base0, lidar0, robot0)",
        ]
    );

    // 登録所につながっていない Container では読めない
    let lonely = base0();
    assert!(matches!(
        lonely.lookup("lidar0", "robot0").unwrap_err().downcast_ref::<JuizError>(),
        Some(JuizError::TransformationsNotAttachedError { .. })
    ));
    Ok(())
}

#[test]
fn set_pose_normalizes_and_rejects_invalid_values_before_registry() -> JuizResult<()> {
    let ptr = base0();
    let registry = FakeRegistry::new();
    registry.register(&ptr);

    let p = Pose::new(
        Vector3::new(1.0, 0.0, 0.0),
        Quaternion::new(0.0, 0.0, 0.0, 3.0),
    );
    let written = ptr.set_pose("map", &p)?;
    assert_eq!(written.pose.orientation, Quaternion::identity());

    let bad = Pose::new(Vector3::zero(), Quaternion::new(0.0, 0.0, 0.0, 0.0));
    let err = ptr.set_pose("map", &bad).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::InvalidPoseValueError { .. })
    ));
    // 不正な値は登録所に届かない
    assert_eq!(registry.calls.lock().unwrap().len(), 2);

    // detach で PoseState も空になる
    ptr.detach_transformations()?;
    assert!(ptr.pose_state().read().unwrap().is_empty());
    assert!(ptr.pose("map").is_err());
    Ok(())
}

#[test]
fn anchor_manifest_from_container_manifest() -> JuizResult<()> {
    // 普通の Container は Anchor ではない
    assert!(ContainerManifest::new("mobile_base")
        .name("base0")
        .anchor_manifest()?
        .is_none());

    let m = ContainerManifest::new("periodic_anchor").name("map");
    let a = m.anchor_manifest()?.unwrap();
    assert_eq!(a.kind, AnchorKind::Periodic);
    assert_eq!(a.rate, Some(DEFAULT_ANCHOR_RATE));
    assert_eq!(a.period(), Some(Duration::from_millis(100)));
    assert_eq!(
        a.to_profile_value(),
        jvalue!({"kind": "periodic", "rate": 10.0})
    );

    // conf の書き方 (rate はトップレベル)
    let m: ContainerManifest = serde_json::from_value(
        jvalue!({"type_name": "periodic_anchor", "name": "map", "rate": 30.0}),
    )?;
    assert_eq!(m.anchor_manifest()?.unwrap().rate, Some(30.0));
    // rate は manifest の往復で残る
    let v = serde_json::to_value(&m)?;
    assert_eq!(v["rate"], 30.0);
    // Anchor 以外の Container の manifest には rate が出ない
    let v = serde_json::to_value(ContainerManifest::new("mobile_base").name("b"))?;
    assert!(v.get("rate").is_none());

    let a = ContainerManifest::new("static_anchor")
        .name("odom")
        .anchor_manifest()?
        .unwrap();
    assert_eq!(a.kind, AnchorKind::Static);
    assert_eq!(a.period(), None);
    assert_eq!(a.to_profile_value(), jvalue!({"kind": "static"}));

    for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let err = ContainerManifest::new("periodic_anchor")
            .name("map")
            .rate(rate)
            .anchor_manifest()
            .unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::ContainerManifestInvalidError { .. })
        ));
    }

    // Anchor に ContainerProcess は付けられない
    let err = ContainerManifest::new("static_anchor")
        .name("odom")
        .add_process(ProcessManifest::new("update"))
        .anchor_manifest()
        .unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::ContainerManifestInvalidError { .. })
    ));
    Ok(())
}

#[test]
fn transform_manifests_from_conf() -> JuizResult<()> {
    let conf = jvalue!({
        "transforms": [
            {"source": "map", "destination": "odom"},
            {"source": "base0", "destination": "pantilt0", "translation": {"x": 0.1, "y": 0, "z": 0.3}},
            {"source": "pantilt0", "destination": "camera0", "rotation": {"x": 0, "y": 0, "z": 2.0, "w": 2.0}}
        ]
    });
    let ts = TransformManifest::list_from_conf(&conf)?;
    assert_eq!(ts.len(), 3);
    assert_eq!(ts[0], TransformManifest::new("map", "odom"));
    assert_eq!(ts[0].transform(), juiz_sdk::geometry::Transform::identity());
    assert_eq!(ts[1].translation, Vector3::new(0.1, 0.0, 0.3));
    // 回転は正規化される
    assert!((ts[2].rotation.norm() - 1.0).abs() < 1e-12);

    assert!(TransformManifest::list_from_conf(&jvalue!({}))?.is_empty());

    for bad in [
        jvalue!({"source": "map", "destination": "map"}),
        jvalue!({"source": "", "destination": "map"}),
        jvalue!({"source": "map"}),
    ] {
        let err = TransformManifest::try_from(bad).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::TransformManifestInvalidError { .. })
        ));
    }
    let err = TransformManifest::try_from(
        jvalue!({"source": "a", "destination": "b", "rotation": {"x": 0, "y": 0, "z": 0, "w": 0}}),
    )
    .unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::InvalidPoseValueError { .. })
    ));
    assert!(TransformManifest::list_from_conf(&jvalue!({"transforms": {}})).is_err());

    let v: Value = TransformManifest::new("a", "b")
        .translation(Vector3::new(1.0, 0.0, 0.0))
        .into();
    assert_eq!(v["rotation"]["w"], 1.0);
    assert_eq!(v["translation"]["x"], 1.0);
    Ok(())
}

#[test]
fn pose_listener_manifests_from_conf() -> JuizResult<()> {
    let conf = jvalue!({
        "pose_listeners": [
            {"container": "base0", "process": "map_view0", "arg": "pose", "reference": "map"},
            {"container": "base0", "process": "logger0"},
            {"container": "camera0", "process": "view0", "reference": "map", "on_update": true},
            {"container": "lidar0", "process": "view0", "initial": true}
        ]
    });
    let ls = PoseListenerManifest::list_from_conf(&conf)?;
    assert_eq!(
        ls[0],
        PoseListenerManifest::new("base0", "map_view0")
            .arg("pose")
            .reference("map")
    );
    assert_eq!(ls[1].arg, None);
    assert!(!ls[1].on_update);
    assert!(ls[2].on_update);
    assert!(!ls[2].initial);
    assert_eq!(ls[3], PoseListenerManifest::new("lidar0", "view0").initial(true));

    for bad in [
        jvalue!({"container": "base0", "process": "view0", "on_update": true}),
        jvalue!({"container": "", "process": "view0"}),
        jvalue!({"container": "base0", "process": "view0", "arg": ""}),
        jvalue!({"container": "base0"}),
    ] {
        let err = PoseListenerManifest::try_from(bad).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::PoseListenerError { .. })
        ));
    }
    let v: Value = PoseListenerManifest::new("a", "b").into();
    assert_eq!(
        v,
        jvalue!({"container": "a", "process": "b", "on_update": false})
    );
    let v: Value = PoseListenerManifest::new("a", "b").initial(true).into();
    assert_eq!(v["initial"], jvalue!(true));
    Ok(())
}

#[test]
fn identifiers_follow_spec() {
    assert_eq!(
        transform_identifier(
            "core://core/container/map::periodic_anchor",
            "core://core/container/base0::mobile_base"
        ),
        "transform://core://core/container/map::periodic_anchor|core://core/container/base0::mobile_base"
    );
    assert_eq!(
        pose_listener_identifier("X", "Y", "pose"),
        "pose_listener://X|Y|pose"
    );
}
