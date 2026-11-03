//! Transformations の core (Step 2、issue #369) のテスト
//!
//! 仕様は docs/spec/transformations.md の §4〜§11。数値は §3.5 の例
//! (map → base0 は (1, 0, 0) と z 軸まわり 90°、base0 → camera0 は (0.1, 0, 0.3))。
//! プラグインは使わず、テストの中で Container・Process の Factory を登録する。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_core::transformations::TransformationsPtr;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::manifests::{PoseListenerManifest, TransformManifest};
use juiz_sdk::transformations::ContainerPoseHandle;
use juiz_sdk::types::{DataType, RecordField, RecordType, TypeSchema};

const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// 部品 (中身のない Container)
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

/// エラーの種類 (JuizError の変種の名前) を含む文字列
fn error_text<T: std::fmt::Debug>(r: JuizResult<T>) -> String {
    let e = r.expect_err("should be an error");
    e.chain()
        .map(|c| match c.downcast_ref::<JuizError>() {
            Some(j) => format!("{j:?}"),
            None => c.to_string(),
        })
        .collect::<Vec<String>>()
        .join(" / ")
}

/// 受け取った値を覚える Pose 通知の受け手。テストごとに別の入れ物を使う
macro_rules! recorder {
    ($name:ident, $store:ident) => {
        static $store: Mutex<Vec<Value>> = Mutex::new(Vec::new());
        fn $name(args: CapsuleMap) -> JuizResult<Capsule> {
            let v = args.get("pose")?.lock_as_value(|v| v.clone())?;
            $store.lock().unwrap().push(v.clone());
            Ok(v.into())
        }
    };
}

fn register_factories(worker: &mut CoreWorker) -> JuizResult<()> {
    let store = worker.store_mut();
    let juiz_sdk::factory::ContainerFactoryStruct(manifest, constructor) =
        juiz_sdk::factory::container_factory(
            ContainerManifest::new("part"),
            |_args: CapsuleMap| Ok(Box::new(Part {})),
        );
    store
        .containers
        .register_factory("part", container_factory_create(manifest, constructor)?)?;
    Ok(())
}

fn register_process(
    worker: &mut CoreWorker,
    manifest: ProcessManifest,
    f: fn(CapsuleMap) -> JuizResult<Capsule>,
    name: &str,
) -> JuizResult<ProcessPtr> {
    let type_name = manifest.type_name.clone();
    worker
        .store_mut()
        .processes
        .register_factory(&type_name, process_factory_create(manifest, f)?)?;
    worker.create_process_ref(&ProcessManifest::new(&type_name).name(name))
}

fn create(worker: &mut CoreWorker, type_name: &str, name: &str) -> JuizResult<ContainerPtr> {
    worker.create_container_ref(type_name, name, Vec::<(&str, Value)>::new().into())
}

fn tf(source: &str, destination: &str, t: (f64, f64, f64), q: Quaternion) -> TransformManifest {
    TransformManifest::new(source, destination)
        .translation(Vector3::new(t.0, t.1, t.2))
        .rotation(q)
}

fn registry(worker: &CoreWorker) -> TransformationsPtr {
    worker.store().transformations.clone()
}

fn run(name: &str, f: impl Fn(&System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(jvalue!({
        "name": name,
        "option": { "http_broker": { "start": false } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        register_factories(system.core_broker().lock_mut()?.worker_mut())?;
        f(system)
    })
}

/// map (static) → base0 → camera0 (§3.5)
fn build_example(worker: &mut CoreWorker, anchor_type: &str) -> JuizResult<Vec<ContainerPtr>> {
    let map = create(worker, anchor_type, "map")?;
    let base0 = create(worker, "part", "base0")?;
    let camera0 = create(worker, "part", "camera0")?;
    registry(worker).create_transforms(
        &[
            tf("map", "base0", (1.0, 0.0, 0.0), z90()),
            tf("base0", "camera0", (0.1, 0.0, 0.3), Quaternion::identity()),
        ],
        "",
    )?;
    Ok(vec![map, base0, camera0])
}

#[test]
fn numeric_example_with_static_anchor() -> JuizResult<()> {
    run("tf_numeric", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let (map, base0, camera0) = (&c[0], &c[1], &c[2]);

        // §3.5 の値
        let p = camera0.pose("map")?;
        assert_eq!(p.reference(), "map");
        assert_pose(&p.pose, &pose(1.0, 0.1, 0.3, z90()));
        assert_pose(
            &camera0.pose("base0")?.pose,
            &pose(0.1, 0.0, 0.3, Quaternion::identity()),
        );
        let p = map.pose("camera0")?;
        assert_eq!(p.reference(), "camera0");
        assert_pose(
            &p.pose,
            &pose(-0.1, 1.0, -0.3, Quaternion::new(0.0, 0.0, -H, H)),
        );
        assert_pose(&base0.pose("base0")?.pose, &Pose::identity());

        // StaticAnchor は作られたとき・木が変わったときに PoseState を更新している (§4.4)
        let map_id = map.identifier().to_string();
        let e = camera0
            .pose_handle()
            .entry(&map_id)?
            .expect("PoseState[map]");
        assert_pose(&e.pose, &pose(1.0, 0.1, 0.3, z90()));
        assert_pose(
            &map.pose_handle().entry(&map_id)?.unwrap().pose,
            &Pose::identity(),
        );

        // set_pose は親 → C の Transform を書き換える (§3.5 の最後)
        let written = camera0.set_pose("map", &pose(1.0, 0.2, 0.3, z90()))?;
        assert_pose(&written.pose, &pose(1.0, 0.2, 0.3, z90()));
        let tid = juiz_sdk::manifests::transform_identifier(
            &base0.identifier().to_string(),
            &camera0.identifier().to_string(),
        );
        let t = registry(worker).transform_profile(&tid)?;
        let value: juiz_sdk::geometry::Transform = (&t["transform"]).try_into()?;
        assert!(value.approx_eq(
            &juiz_sdk::geometry::Transform::new(
                Vector3::new(0.2, 0.0, 0.3),
                Quaternion::identity()
            ),
            EPS
        ));
        // set_pose が戻った時点で Static の木は新しい値 (§4.4 の 2)
        let e = camera0.pose_handle().entry(&map_id)?.unwrap();
        assert_pose(&e.pose, &pose(1.0, 0.2, 0.3, z90()));

        // 基準を子孫でない普通の Container にした書き込み
        camera0.set_pose("base0", &pose(0.0, 0.0, 1.0, Quaternion::identity()))?;
        assert_pose(&camera0.pose("map")?.pose, &pose(1.0, 0.0, 1.0, z90()));

        // Transform の値の変更 (§7.3)
        registry(worker).update_transform(&tid, Some(Vector3::new(0.1, 0.0, 0.3)), None)?;
        assert_pose(&camera0.pose("map")?.pose, &pose(1.0, 0.1, 0.3, z90()));
        Ok(())
    })
}

#[test]
fn container_impl_reads_and_writes_pose() -> JuizResult<()> {
    run("tf_container_impl", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let camera0 = &c[2];
        let p = {
            let mut guard = camera0.lock_mut()?;
            let ci = guard.downcast_mut::<ContainerImpl<Part>>().unwrap();
            ci.set_pose("map", pose(2.0, 0.0, 0.0, Quaternion::identity()))?;
            ci.pose("base0")?
        };
        // base0 から見て map の x 方向 1 は -y 方向 (base0 は 90° 回っている)
        assert_pose(
            &p.pose,
            &pose(0.0, -1.0, 0.0, Quaternion::new(0.0, 0.0, -H, H)),
        );
        Ok(())
    })
}

#[test]
fn periodic_anchor_follows_in_next_period() -> JuizResult<()> {
    run("tf_periodic", |system| {
        let (camera0, map_id, tid) = {
            let mut cb = system.core_broker().lock_mut()?;
            let worker = cb.worker_mut();
            worker.create_container_ref(
                "periodic_anchor",
                "map",
                vec![("rate", jvalue!(100.0))].into(),
            )?;
            let base0 = create(worker, "part", "base0")?;
            let camera0 = create(worker, "part", "camera0")?;
            registry(worker).create_transforms(
                &[
                    tf("map", "base0", (1.0, 0.0, 0.0), z90()),
                    tf("base0", "camera0", (0.1, 0.0, 0.3), Quaternion::identity()),
                ],
                "",
            )?;
            // まだ更新されていなくても、読むとその場で計算する (§7.1 の 2)
            assert_pose(&camera0.pose("map")?.pose, &pose(1.0, 0.1, 0.3, z90()));
            let map_id = registry(worker).anchor_list()?[0].clone();
            let tid =
                juiz_sdk::manifests::transform_identifier(&map_id, &base0.identifier().to_string());
            (camera0, map_id, tid)
        };
        let wait_until = |f: &dyn Fn() -> bool| {
            let start = Instant::now();
            while !f() {
                assert!(start.elapsed() < Duration::from_secs(5), "timeout");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let handle = camera0.pose_handle();
        let entry_is = |p: Pose| {
            handle
                .entry(&map_id)
                .unwrap()
                .is_some_and(|e| e.pose.approx_eq(&p, EPS))
        };
        wait_until(&|| entry_is(pose(1.0, 0.1, 0.3, z90())));
        // 親の Transform を変えると、子は次の周期で追従する
        registry(&system.core_broker().lock()?.worker()).update_transform(
            &tid,
            Some(Vector3::new(2.0, 0.0, 0.0)),
            None,
        )?;
        wait_until(&|| entry_is(pose(2.0, 0.1, 0.3, z90())));
        Ok(())
    })
}

#[test]
fn tree_rules_are_checked() -> JuizResult<()> {
    run("tf_tree_rules", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        create(worker, "static_anchor", "map")?;
        create(worker, "static_anchor", "world")?;
        for n in ["a", "b", "c"] {
            create(worker, "part", n)?;
        }
        let reg = registry(worker);
        let id = Quaternion::identity();
        reg.create_transforms(
            &[
                tf("map", "a", (1.0, 0.0, 0.0), id),
                tf("map", "b", (0.0, 1.0, 0.0), id),
                tf("a", "c", (0.0, 0.0, 1.0), id),
            ],
            "",
        )?;
        // 2 通りの道で届く (§5.2)。破る変更は丸ごと拒む
        let e = error_text(reg.create_transforms(&[tf("b", "c", (0.0, 0.0, 0.0), id)], ""));
        assert!(e.contains("TransformTreeError"), "{e}");
        assert_eq!(reg.transform_list()?.len(), 3);
        // Anchor へ戻る循環
        let e = error_text(reg.create_transforms(&[tf("c", "map", (0.0, 0.0, 0.0), id)], ""));
        assert!(e.contains("TransformTreeError"), "{e}");
        // 途中の循環
        let e = error_text(reg.create_transforms(&[tf("c", "a", (0.0, 0.0, 0.0), id)], ""));
        assert!(e.contains("TransformTreeError"), "{e}");
        // まとめて作るときは 1 つでも不備があれば何も作らない
        let e = error_text(reg.create_transforms(
            &[
                tf("world", "b", (0.0, 0.0, 0.0), id),
                tf("b", "c", (0.0, 0.0, 0.0), id),
            ],
            "",
        ));
        assert!(e.contains("TransformTreeError"), "{e}");
        assert_eq!(reg.transform_list()?.len(), 3);
        // 同じ組・同じ端点・無い端点
        let e = error_text(reg.create_transforms(&[tf("map", "a", (0.0, 0.0, 0.0), id)], ""));
        assert!(e.contains("TransformAlreadyExistsError"), "{e}");
        let e = error_text(reg.create_transforms(&[tf("a", "/a", (0.0, 0.0, 0.0), id)], ""));
        assert!(e.contains("TransformManifestInvalidError"), "{e}");
        assert!(reg
            .create_transforms(&[tf("a", "nothing", (0.0, 0.0, 0.0), id)], "")
            .is_err());
        // 1 つの Container が複数の Anchor の木に入るのはよい (map と world は互いに届かない)
        reg.create_transforms(&[tf("world", "a", (5.0, 0.0, 0.0), id)], "")?;
        let c = worker.container_from_typename_and_name("part", "c")?;
        assert_pose(&c.pose("map")?.pose, &pose(1.0, 0.0, 1.0, id));
        assert_pose(&c.pose("world")?.pose, &pose(5.0, 0.0, 1.0, id));
        // どの Anchor からも届かない Transform はエラーにしない
        create(worker, "part", "d")?;
        create(worker, "part", "e")?;
        reg.create_transforms(&[tf("d", "e", (0.0, 0.0, 0.0), id)], "")?;
        let e = worker.container_from_typename_and_name("part", "e")?;
        let err = error_text(e.pose("map"));
        assert!(err.contains("NoCommonAnchorError"), "{err}");
        Ok(())
    })
}

#[test]
fn set_pose_errors() -> JuizResult<()> {
    run("tf_set_pose_errors", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let (map, base0, camera0) = (&c[0], &c[1], &c[2]);
        let p = Pose::identity();
        let e = error_text(map.set_pose("base0", &p));
        assert!(e.contains("PoseNotWritableError"), "{e}");
        let e = error_text(base0.set_pose("base0", &p));
        assert!(e.contains("PoseNotWritableError"), "{e}");
        // 基準が子孫
        let e = error_text(base0.set_pose("camera0", &p));
        assert!(e.contains("PoseNotWritableError"), "{e}");
        // 値の不備
        let bad = Pose::new(Vector3::new(f64::NAN, 0.0, 0.0), Quaternion::identity());
        let e = error_text(camera0.set_pose("map", &bad));
        assert!(e.contains("InvalidPoseValueError"), "{e}");
        // どの木にも入っていない Container (Transform は自動では作らない)
        let lonely = create(worker, "part", "lonely")?;
        let e = error_text(lonely.set_pose("map", &p));
        assert!(e.contains("NoCommonAnchorError"), "{e}");
        assert_eq!(registry(worker).transform_list()?.len(), 2);
        Ok(())
    })
}

#[test]
fn anchor_under_anchor_and_common_anchor() -> JuizResult<()> {
    run("tf_anchor_under_anchor", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let map = create(worker, "static_anchor", "map")?;
        let odom = create(worker, "static_anchor", "odom")?;
        let base0 = create(worker, "part", "base0")?;
        let arm0 = create(worker, "part", "arm0")?;
        let id = Quaternion::identity();
        registry(worker).create_transforms(
            &[
                tf("map", "odom", (10.0, 0.0, 0.0), id),
                tf("odom", "base0", (1.0, 0.0, 0.0), id),
                tf("base0", "arm0", (0.0, 0.0, 1.0), id),
            ],
            "",
        )?;
        let map_id = map.identifier().to_string();
        let odom_id = odom.identifier().to_string();
        // odom の木に map は入らない (§4.2)
        let tree = registry(worker).tree("odom", "")?;
        assert_eq!(tree["identifier"], jvalue!(odom_id));
        assert_eq!(
            tree["children"][0]["children"][0]["identifier"],
            jvalue!(arm0.identifier().to_string())
        );
        assert!(odom.pose_handle().entry(&map_id)?.is_some());
        assert!(map.pose_handle().entry(&odom_id)?.is_none());
        assert_pose(&odom.pose("map")?.pose, &pose(10.0, 0.0, 0.0, id));
        assert_pose(&map.pose("odom")?.pose, &pose(-10.0, 0.0, 0.0, id));
        // 共通の Anchor が 2 つ (map と odom) なら深さの和が小さい odom を使う
        assert_pose(&arm0.pose("base0")?.pose, &pose(0.0, 0.0, 1.0, id));
        // odom を基準に書くと odom → base0 が書き換わる
        base0.set_pose("odom", &pose(2.0, 0.0, 0.0, id))?;
        assert_pose(&arm0.pose("map")?.pose, &pose(12.0, 0.0, 1.0, id));
        // profile に Anchor の種類が載る (§4.1)
        let profile = odom.lock()?.profile()?;
        assert_eq!(profile.anchor, Some(jvalue!({"kind": "static"})));
        assert!(base0.lock()?.profile()?.anchor.is_none());
        Ok(())
    })
}

#[test]
fn anchor_manifest_errors() -> JuizResult<()> {
    run("tf_anchor_manifest", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        for rate in [jvalue!(0.0), jvalue!(-1.0), jvalue!("fast")] {
            let r =
                worker.create_container_ref("periodic_anchor", "p0", vec![("rate", rate)].into());
            let e = error_text(r);
            assert!(e.contains("ContainerManifestInvalidError"), "{e}");
        }
        let r = worker.create_container_ref(
            "static_anchor",
            "s0",
            vec![("processes", jvalue!([{"type_name": "x", "name": "x0"}]))].into(),
        );
        assert!(error_text(r).contains("ContainerManifestInvalidError"));
        assert!(worker.store().containers.objects().is_empty());
        let p = worker.create_container_ref(
            "periodic_anchor",
            "p0",
            Vec::<(&str, Value)>::new().into(),
        )?;
        assert_eq!(
            p.lock()?.profile()?.anchor,
            Some(jvalue!({"kind": "periodic", "rate": 10.0}))
        );
        // 同じ ID の二重登録
        assert!(worker
            .create_container_ref("periodic_anchor", "p0", Vec::<(&str, Value)>::new().into())
            .is_err());
        Ok(())
    })
}

#[test]
fn destroy_removes_transforms_and_pose_state() -> JuizResult<()> {
    run("tf_destroy", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let (map, base0, camera0) = (c[0].clone(), c[1].clone(), c[2].clone());
        create(worker, "periodic_anchor", "world")?;
        registry(worker).create_transforms(
            &[tf(
                "world",
                "base0",
                (0.0, 0.0, 0.0),
                Quaternion::identity(),
            )],
            "",
        )?;
        let map_id = map.identifier().to_string();
        assert!(camera0.pose_handle().entry(&map_id)?.is_some());
        // Transform を消すと、木から外れた Container の項目が消える (§10)
        let tid =
            juiz_sdk::manifests::transform_identifier(&map_id, &base0.identifier().to_string());
        registry(worker).destroy_transform(&tid)?;
        assert!(camera0.pose_handle().entry(&map_id)?.is_none());
        assert!(registry(worker).transform_profile(&tid).is_err());
        // Anchor を消すと、すべての PoseState から自分の項目を消し、スレッドを止める
        let world = worker.container_from_typename_and_name("periodic_anchor", "world")?;
        let world_id = world.identifier();
        std::thread::sleep(Duration::from_millis(50));
        assert!(camera0
            .pose_handle()
            .entry(&world_id.to_string())?
            .is_some());
        worker.destroy_container_ref(&world_id)?;
        assert!(camera0
            .pose_handle()
            .entry(&world_id.to_string())?
            .is_none());
        assert_eq!(registry(worker).anchor_list()?.len(), 1);
        assert_eq!(registry(worker).transform_list()?.len(), 1);
        // Container を消すと、それを端点に持つ Transform が消え、登録所から外れる
        worker.destroy_container_ref(&base0.identifier())?;
        assert!(registry(worker).transform_list()?.is_empty());
        let e = error_text(base0.pose("map"));
        assert!(e.contains("TransformationsNotAttachedError"), "{e}");
        Ok(())
    })
}

recorder!(record_basic, RECORD_BASIC);
recorder!(record_update, RECORD_UPDATE);

#[test]
fn pose_listener_is_called_on_explicit_write() -> JuizResult<()> {
    run("tf_listener", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let (base0, camera0) = (c[1].clone(), c[2].clone());
        register_process(
            worker,
            ProcessManifest::new("record_basic").add_object_arg("pose", "pose", jvalue!({})),
            record_basic,
            "rec0",
        )?;
        // arg は引数が 1 つなら省略できる。reference を省くと書き込みの基準
        worker.create_pose_listener(PoseListenerManifest::new("camera0", "rec0"), "")?;
        camera0.set_pose("map", &pose(1.0, 0.2, 0.3, z90()))?;
        let got = RECORD_BASIC.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        let p: PoseStamped = (&got[0]).try_into()?;
        assert_eq!(p.reference(), "map");
        assert_pose(&p.pose, &pose(1.0, 0.2, 0.3, z90()));
        // 周期更新・Static の再計算では呼ばない。base0 を動かしても camera0 の通知は出ない
        base0.set_pose("map", &pose(0.0, 0.0, 0.0, Quaternion::identity()))?;
        assert_eq!(RECORD_BASIC.lock().unwrap().len(), 1);
        // Transform の値の変更は終点について通知する (基準は始点)
        let tid = juiz_sdk::manifests::transform_identifier(
            &base0.identifier().to_string(),
            &camera0.identifier().to_string(),
        );
        registry(worker).update_transform(&tid, Some(Vector3::new(0.5, 0.0, 0.0)), None)?;
        let got = RECORD_BASIC.lock().unwrap().clone();
        assert_eq!(got.len(), 2);
        let p: PoseStamped = (&got[1]).try_into()?;
        assert_eq!(p.reference(), "base0");
        assert_pose(&p.pose, &pose(0.5, 0.0, 0.0, Quaternion::identity()));
        // 二重登録はエラー、Process を消すと登録も消える
        let e = error_text(
            worker.create_pose_listener(PoseListenerManifest::new("camera0", "rec0"), ""),
        );
        assert!(e.contains("PoseListenerError"), "{e}");
        assert_eq!(registry(worker).pose_listener_list()?.len(), 1);
        let pid = worker
            .process_from_typename_and_name("record_basic", "rec0")?
            .identifier();
        worker.destroy_process_ref(&pid)?;
        assert!(registry(worker).pose_listener_list()?.is_empty());
        Ok(())
    })
}

#[test]
fn pose_listener_on_update() -> JuizResult<()> {
    run("tf_listener_on_update", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let base0 = c[1].clone();
        register_process(
            worker,
            ProcessManifest::new("record_update").add_object_arg("pose", "pose", jvalue!({})),
            record_update,
            "rec1",
        )?;
        worker.create_pose_listener(
            PoseListenerManifest::new("camera0", "rec1")
                .arg("pose")
                .reference("map")
                .on_update(true),
            "",
        )?;
        // base0 を動かすと Static の更新で camera0 の値が変わるので呼ばれる
        base0.set_pose("map", &pose(3.0, 0.0, 0.0, z90()))?;
        let got = RECORD_UPDATE.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        let p: PoseStamped = (&got[0]).try_into()?;
        assert_eq!(p.reference(), "map");
        assert_pose(&p.pose, &pose(3.0, 0.1, 0.3, z90()));
        // 値が変わらない更新では呼ばない
        registry(worker).update_anchor("map", "")?;
        assert_eq!(RECORD_UPDATE.lock().unwrap().len(), 1);
        Ok(())
    })
}

static REENTRANT_HANDLE: OnceLock<ContainerPoseHandle> = OnceLock::new();
static REENTRANT_CALLS: Mutex<i64> = Mutex::new(0);

/// 通知の中でまた set_pose する受け手 (§8.4)
fn reentrant(args: CapsuleMap) -> JuizResult<Capsule> {
    let _ = args.get("pose")?;
    *REENTRANT_CALLS.lock().unwrap() += 1;
    REENTRANT_HANDLE
        .get()
        .unwrap()
        .set_pose("map", &pose(9.0, 0.0, 0.0, Quaternion::identity()))?;
    Ok(jvalue!(null).into())
}

#[test]
fn pose_listener_reentry_stops_at_one_level() -> JuizResult<()> {
    run("tf_listener_reentry", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let c = build_example(worker, "static_anchor")?;
        let camera0 = c[2].clone();
        let _ = REENTRANT_HANDLE.set(camera0.pose_handle());
        register_process(
            worker,
            ProcessManifest::new("reentrant").add_object_arg("pose", "pose", jvalue!({})),
            reentrant,
            "re0",
        )?;
        worker.create_pose_listener(PoseListenerManifest::new("camera0", "re0"), "")?;
        camera0.set_pose("map", &pose(1.0, 0.0, 0.0, Quaternion::identity()))?;
        // 中の書き込みは行うが、その書き込みによる通知は出さない
        assert_eq!(*REENTRANT_CALLS.lock().unwrap(), 1);
        assert_pose(
            &camera0.pose("map")?.pose,
            &pose(9.0, 0.0, 0.0, Quaternion::identity()),
        );
        Ok(())
    })
}

fn noop(_args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(null).into())
}

fn record_schema(namespace: &str, name: &str) -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new(namespace, name).field(RecordField::new("x", DataType::Double)),
    ))
    .unwrap()
}

#[test]
fn pose_listener_registration_errors() -> JuizResult<()> {
    run("tf_listener_errors", |system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        build_example(worker, "static_anchor")?;
        register_process(
            worker,
            ProcessManifest::new("two_args")
                .add_int_arg("a", "a", 0)
                .add_int_arg("b", "b", 0),
            noop,
            "two0",
        )?;
        register_process(
            worker,
            ProcessManifest::new("typed").add_arg(ArgumentManifest::new_typed(
                "pose",
                record_schema("robot.geometry", "Pose2D"),
                jvalue!({"x": 0.0}),
            )?),
            noop,
            "typed0",
        )?;
        register_process(
            worker,
            ProcessManifest::new("pose_typed").add_arg(ArgumentManifest::new_typed(
                "pose",
                record_schema("juiz_std.geometry", "PoseStamped"),
                jvalue!({"x": 0.0}),
            )?),
            noop,
            "pose_typed0",
        )?;
        let l = |p: &str| PoseListenerManifest::new("camera0", p);
        // 引数が 2 つ以上なら arg が要る、無い引数はエラー
        assert!(
            error_text(worker.create_pose_listener(l("two0"), "")).contains("PoseListenerError")
        );
        assert!(
            error_text(worker.create_pose_listener(l("two0").arg("c"), ""))
                .contains("PoseListenerError")
        );
        worker.create_pose_listener(l("two0").arg("a"), "")?;
        // 型が PoseStamped でない
        assert!(
            error_text(worker.create_pose_listener(l("typed0"), "")).contains("PoseListenerError")
        );
        worker.create_pose_listener(l("pose_typed0"), "")?;
        // 無い Process・無い Container・on_update で reference なし
        assert!(worker.create_pose_listener(l("nothing"), "").is_err());
        assert!(worker
            .create_pose_listener(PoseListenerManifest::new("nothing", "two0").arg("a"), "")
            .is_err());
        assert!(
            error_text(worker.create_pose_listener(l("two0").arg("b").on_update(true), ""))
                .contains("PoseListenerError")
        );
        Ok(())
    })
}

#[test]
fn transforms_in_conf() -> JuizResult<()> {
    crate::common::logger::init();
    // Anchor の子に Anchor を置けるので、プラグインなしで conf だけで木を作れる
    System::new(jvalue!({
        "name": "tf_conf",
        "namespace": "robot1",
        "option": { "http_broker": { "start": false } },
        "containers": [
            {"type_name": "static_anchor", "name": "map"},
            {"type_name": "static_anchor", "name": "base0"},
            {"type_name": "periodic_anchor", "name": "camera0", "rate": 30.0},
        ],
        "transforms": [
            {"source": "map", "destination": "base0",
             "translation": {"x": 1.0, "y": 0.0, "z": 0.0},
             "rotation": {"x": 0.0, "y": 0.0, "z": H, "w": H}},
            {"source": "base0", "destination": "/robot1/camera0",
             "translation": {"x": 0.1, "z": 0.3}},
        ],
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        let camera0 = worker.container_from_identifier(&ContainerIdentifier::try_from(
            "core://core/container/robot1/camera0::periodic_anchor".to_owned(),
        )?)?;
        let p = camera0.pose("map")?;
        assert_eq!(p.reference(), "robot1/map");
        assert_pose(&p.pose, &pose(1.0, 0.1, 0.3, z90()));
        assert_eq!(registry(worker).anchor_list()?.len(), 3);
        Ok(())
    })?;

    // 木の規則を破る conf は読み込みエラー
    let r = System::new(jvalue!({
        "name": "tf_conf_broken",
        "option": { "http_broker": { "start": false } },
        "containers": [
            {"type_name": "static_anchor", "name": "map"},
            {"type_name": "static_anchor", "name": "odom"},
        ],
        "transforms": [
            {"source": "map", "destination": "odom"},
            {"source": "odom", "destination": "map"},
        ],
    }))?
    .setup();
    let e = error_text(r.map(|_| ()));
    assert!(e.contains("TransformTreeError"), "{e}");
    Ok(())
}
