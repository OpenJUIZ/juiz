//! System をまたぐ Transform の core (Step 1、issue #414) のテスト
//!
//! 仕様は docs/spec/transformations.md §19。同じプロセスに CoreWorker を複数作って System の代わりにし、
//! 取り込みの口 (`PoseStateFetcher`) を、ブローカー名から相手の登録所の `pose_state` を直接呼ぶ
//! 偽の通信 ([`Net`]) に差し替える。相手を「落とす」と取り込みは失敗する。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_core::transformations::api_types::PoseStateQuery;
use juiz_core::transformations::TransformationsPtr;
use juiz_sdk::geometry::{Pose, Quaternion, Vector3};
use juiz_sdk::manifests::{transform_identifier, TransformManifest};

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
    let e = r.expect_err("should be an error");
    e.chain()
        .map(|c| match c.downcast_ref::<JuizError>() {
            Some(j) => format!("{j:?}"),
            None => c.to_string(),
        })
        .collect::<Vec<String>>()
        .join(" / ")
}

fn tf(source: &str, destination: &str, t: (f64, f64, f64), q: Quaternion) -> TransformManifest {
    TransformManifest::new(source, destination)
        .translation(Vector3::new(t.0, t.1, t.2))
        .rotation(q)
}

/// 偽の通信。ブローカー名 → (相手の登録所, つながっているか)
#[derive(Clone, Default)]
struct Net {
    systems: Arc<Mutex<HashMap<String, (TransformationsPtr, bool)>>>,
}

impl Net {
    fn add(&self, broker_name: &str, registry: TransformationsPtr) {
        self.systems
            .lock()
            .unwrap()
            .insert(broker_name.to_owned(), (registry, true));
    }

    fn set_up(&self, broker_name: &str, up: bool) {
        self.systems.lock().unwrap().get_mut(broker_name).unwrap().1 = up;
    }

    /// `registry` の取り込みの口をこの通信にする
    fn attach(&self, registry: &TransformationsPtr) -> JuizResult<()> {
        let net = self.clone();
        registry.set_pose_state_fetcher(Arc::new(
            move |source: &ContainerIdentifier, query: &PoseStateQuery| {
                let target = {
                    let systems = net.systems.lock().unwrap();
                    match systems.get(&source.broker_name) {
                        Some((r, true)) => r.clone(),
                        _ => {
                            return Err(juiz_sdk::anyhow::anyhow!(
                                "System ({}) is down",
                                source.broker_name
                            ))
                        }
                    }
                };
                target.pose_state(query, "")
            },
        ))
    }
}

/// System の代わりの CoreWorker
struct Sys {
    worker: CoreWorker,
    /// 相手から見たブローカー名
    broker: String,
}

impl Sys {
    fn new(net: &Net, broker: &str) -> JuizResult<Self> {
        let mut worker = CoreWorker::new(uuid::Uuid::new_v4(), jvalue!({"name": broker}));
        let juiz_sdk::factory::ContainerFactoryStruct(manifest, constructor) =
            juiz_sdk::factory::container_factory(
                ContainerManifest::new("part"),
                |_args: CapsuleMap| Ok(Box::new(Part {})),
            );
        worker
            .store_mut()
            .containers
            .register_factory("part", container_factory_create(manifest, constructor)?)?;
        let s = Self {
            worker,
            broker: broker.to_owned(),
        };
        net.add(broker, s.reg());
        net.attach(&s.reg())?;
        Ok(s)
    }

    fn reg(&self) -> TransformationsPtr {
        self.worker.store().transformations.clone()
    }

    fn uuid(&self) -> String {
        self.reg().system_uuid().unwrap()
    }

    fn create(&mut self, type_name: &str, name: &str) -> JuizResult<ContainerPtr> {
        self.worker
            .create_container_ref(type_name, name, Vec::<(&str, Value)>::new().into())
    }

    /// 相手から見たこの System の Container の ID (ブローカー付き)
    fn remote_id(&self, name: &str, type_name: &str) -> String {
        format!("http://{}/container/{name}::{type_name}", self.broker)
    }

    fn start(&self) -> JuizResult<()> {
        self.reg().start()
    }
}

impl Drop for Sys {
    fn drop(&mut self) {
        let _ = self.reg().clear();
    }
}

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for: {what}");
}

fn init() {
    crate::common::logger::init();
}

/// 例 A (§19.1): S1 の map (Anchor) → S2 の odom → base0 → camera0。P 自身が Anchor なので P' = B'
#[test]
fn example_a_source_is_anchor() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8000")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8001")?;
    s1.create("static_anchor", "map")?;
    let _odom = s2.create("part", "odom")?;
    let _base0 = s2.create("part", "base0")?;
    let camera0 = s2.create("part", "camera0")?;
    let map_remote = s1.remote_id("map", "static_anchor");
    let created = s2.reg().create_transforms(
        &[
            tf(&map_remote, "odom", (2.0, 0.0, 0.0), Quaternion::identity()).rate(50.0),
            tf("odom", "base0", (1.0, 0.0, 0.0), z90()),
            tf("base0", "camera0", (0.1, 0.0, 0.3), Quaternion::identity()),
        ],
        "",
    )?;
    // Transform の ID の source はブローカー付きの ID のまま (§19.2 の 4)
    assert_eq!(created[0]["source"], jvalue!(map_remote));
    // 1 回目が届くまでは写しがない (§19.4)
    let e = error_text(camera0.pose("map"));
    assert!(
        e.contains("NoCommonAnchorError") && e.contains("not been imported"),
        "{e}"
    );

    s2.start()?;
    wait_until("replica of map", || camera0.pose("map").is_ok());

    // 名前 (自分に同名がない)・ブローカー付きの ID・相手での core の ID のどれでも写しに解決する (§19.5)
    for reference in [
        "map",
        map_remote.as_str(),
        "core://core/container/map::static_anchor",
    ] {
        let p = camera0.pose(reference)?;
        assert_pose(&p.pose, &pose(3.0, 0.1, 0.3, z90()));
        assert_eq!(p.reference(), map_remote, "frame_id is the replica's ID");
    }
    let reg2 = s2.reg();
    assert_eq!(reg2.anchor_list()?, vec![map_remote.clone()]);
    // 取り込みの辺は Transform の一覧に出ない
    assert_eq!(reg2.transform_list()?.len(), 3);
    let tid = transform_identifier(&map_remote, "core://core/container/odom::part");
    let profile = reg2.transform_profile(&tid)?;
    assert_eq!(profile["remote"]["state"], jvalue!("ok"));
    assert_eq!(profile["remote"]["rate"], jvalue!(50.0));
    assert_eq!(profile["remote"]["timeout"], jvalue!(3.0));
    assert_eq!(profile["remote"]["system"], jvalue!(s1.uuid()));
    let transforms = reg2.transform_list()?;
    assert!(transforms.contains(&tid));

    // P'・B' への set_pose はエラー、P' → C の Transform は書き換えられる (§19.3)
    let odom = s2
        .worker
        .container_from_identifier(&ContainerIdentifier::try_from(
            "core://core/container/odom::part".to_owned(),
        )?)?;
    let written = odom.set_pose("map", &pose(5.0, 0.0, 0.0, Quaternion::identity()))?;
    assert_pose(&written.pose, &pose(5.0, 0.0, 0.0, Quaternion::identity()));
    assert_pose(&camera0.pose("map")?.pose, &pose(6.0, 0.1, 0.3, z90()));
    let e = error_text(reg2.set_pose_by_name(&map_remote, "odom", "", &Pose::identity()));
    assert!(e.contains("PoseNotWritableError"), "{e}");

    // S1 では S2 の木は取り込まない (§19.3)
    assert_eq!(s1.reg().transform_list()?.len(), 0);
    Ok(())
}

/// P が Anchor でない Container: B' の直下の子が P' (§19.3)。相手の値の変化が届き、変わらなければ何もしない
#[test]
fn example_a_source_is_container_and_follows_updates() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8010")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8011")?;
    s1.create("static_anchor", "map")?;
    s1.create("part", "base_a")?;
    s1.reg()
        .create_transforms(&[tf("map", "base_a", (1.0, 0.0, 0.0), z90())], "")?;
    let arm = s2.create("part", "arm")?;
    let base_remote = s1.remote_id("base_a", "part");
    s2.reg().create_transforms(
        &[tf(&base_remote, "arm", (0.0, 1.0, 0.0), Quaternion::identity()).rate(100.0)],
        "",
    )?;
    s2.start()?;
    wait_until("replica of map", || arm.pose("map").is_ok());
    // map → base_a は (1,0,0) z90、base_a → arm は (0,1,0) → map から見て (0, 0, 0) 回転 z90
    assert_pose(&arm.pose("map")?.pose, &pose(0.0, 0.0, 0.0, z90()));
    // P' も基準にできる
    assert_pose(
        &arm.pose(&base_remote)?.pose,
        &pose(0.0, 1.0, 0.0, Quaternion::identity()),
    );
    let map_remote = s1.remote_id("map", "static_anchor");
    assert_eq!(s2.reg().anchor_list()?, vec![map_remote.clone()]);

    // 変わらなければ PoseState の版番号も変わらない (版番号で省く、§19.4)
    let v0 = arm.pose_handle().entry(&map_remote)?.unwrap().version;
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(arm.pose_handle().entry(&map_remote)?.unwrap().version, v0);

    // S1 で動かすと S2 に届く
    let tid = transform_identifier(
        "core://core/container/map::static_anchor",
        "core://core/container/base_a::part",
    );
    s1.reg()
        .update_transform(&tid, Some(Vector3::new(2.0, 0.0, 0.0)), None)?;
    wait_until("update from S1", || {
        arm.pose("map")
            .map(|p| p.pose.approx_eq(&pose(1.0, 0.0, 0.0, z90()), EPS))
            .unwrap_or(false)
    });
    // 時刻は相手の System の値 (§19.3)
    let s1_stamp = s1.reg().pose_by_name("base_a", "map", "")?.header.stamp;
    assert_eq!(arm.pose("map")?.header.stamp, s1_stamp);

    // Transform を消せば写しの始点も写しも消える (§19.6)
    let cross = transform_identifier(&base_remote, "core://core/container/arm::part");
    s2.reg().destroy_transform(&cross)?;
    assert!(s2.reg().anchor_list()?.is_empty());
    assert!(arm.pose("map").is_err());
    Ok(())
}

/// 多段 (§19.3): S1 map → S2 odom → camera0、S3 は S2 の camera0 から取り込む
#[test]
fn multi_stage() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8020")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8021")?;
    let mut s3 = Sys::new(&net, "127.0.0.1:8022")?;
    s1.create("static_anchor", "map")?;
    s2.create("part", "odom")?;
    s2.create("part", "camera0")?;
    let tool = s3.create("part", "tool")?;
    let map_remote = s1.remote_id("map", "static_anchor");
    s2.reg().create_transforms(
        &[
            tf(&map_remote, "odom", (1.0, 0.0, 0.0), Quaternion::identity()).rate(100.0),
            tf("odom", "camera0", (0.0, 1.0, 0.0), Quaternion::identity()),
        ],
        "",
    )?;
    s3.reg().create_transforms(
        &[tf(
            &s2.remote_id("camera0", "part"),
            "tool",
            (0.0, 0.0, 1.0),
            Quaternion::identity(),
        )
        .rate(100.0)],
        "",
    )?;
    s2.start()?;
    s3.start()?;
    wait_until("multi stage", || tool.pose("map").is_ok());
    assert_pose(
        &tool.pose("map")?.pose,
        &pose(1.0, 1.0, 1.0, Quaternion::identity()),
    );
    // S3 の写しの Anchor の ID は S2 の写しの ID のまま (ブローカー付き、§19.5)。本当の持ち主は S1
    assert_eq!(s3.reg().anchor_list()?, vec![map_remote.clone()]);
    let st = s3
        .reg()
        .pose_state(&PoseStateQuery::new("core://core/container/tool::part"), "")?;
    assert_eq!(st["entries"][0]["origin_system"], jvalue!(s1.uuid()));
    assert_eq!(
        st["entries"][0]["origin_anchor"],
        jvalue!("core://core/container/map::static_anchor")
    );
    Ok(())
}

/// 循環 (§19.5): S2 が S1 から、S1 が S2 から取り込んでも、回ってきた自分の Anchor は取り込まない
#[test]
fn cycle_is_not_imported() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8030")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8031")?;
    s1.create("static_anchor", "map")?;
    let x = s1.create("part", "x")?;
    let odom = s2.create("part", "odom")?;
    s2.reg().create_transforms(
        &[tf(
            &s1.remote_id("map", "static_anchor"),
            "odom",
            (1.0, 0.0, 0.0),
            Quaternion::identity(),
        )
        .rate(100.0)],
        "",
    )?;
    s1.reg().create_transforms(
        &[tf(
            &s2.remote_id("odom", "part"),
            "x",
            (1.0, 0.0, 0.0),
            Quaternion::identity(),
        )
        .rate(100.0)],
        "",
    )?;
    s1.start()?;
    s2.start()?;
    wait_until("S2 imports map", || odom.pose("map").is_ok());
    // S1 が S2 から取り込むのは S1 自身の map の値だけなので、写しは作らない
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        s1.reg().anchor_list()?,
        vec!["core://core/container/map::static_anchor".to_owned()]
    );
    let e = error_text(x.pose("map"));
    assert!(e.contains("NoCommonAnchorError"), "{e}");
    let tid = transform_identifier(
        &s2.remote_id("odom", "part"),
        "core://core/container/x::part",
    );
    assert_eq!(
        s1.reg().transform_profile(&tid)?["remote"]["state"],
        jvalue!("ok")
    );
    Ok(())
}

/// つながらなくなったとき (§19.6): timeout まで最後の値、過ぎたら写しを消し、戻れば復活。remove_subsystem でも消す
#[test]
fn disconnect_and_reconnect() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8040")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8041")?;
    s1.create("static_anchor", "map")?;
    let odom = s2.create("part", "odom")?;
    let map_remote = s1.remote_id("map", "static_anchor");
    s2.reg().create_transforms(
        &[
            tf(&map_remote, "odom", (1.0, 0.0, 0.0), Quaternion::identity())
                .rate(100.0)
                .timeout(0.5),
        ],
        "",
    )?;
    let tid = transform_identifier(&map_remote, "core://core/container/odom::part");
    s2.start()?;
    wait_until("import", || odom.pose("map").is_ok());

    net.set_up(&s1.broker, false);
    wait_until("stale", || {
        s2.reg().transform_profile(&tid).unwrap()["remote"]["state"] == jvalue!("stale")
    });
    // timeout 前は最後の値が読める
    assert_pose(
        &odom.pose("map")?.pose,
        &pose(1.0, 0.0, 0.0, Quaternion::identity()),
    );
    wait_until("lost", || {
        s2.reg().transform_profile(&tid).unwrap()["remote"]["state"] == jvalue!("lost")
    });
    let e = error_text(odom.pose("map"));
    assert!(
        e.contains("NoCommonAnchorError") && e.contains("not reachable"),
        "{e}"
    );
    // Transform は消さない
    assert!(s2.reg().transform_list()?.contains(&tid));
    assert!(s2.reg().anchor_list()?.is_empty());

    // つながり直せば戻る (間隔は 1 秒になっている)
    net.set_up(&s1.broker, true);
    wait_until("restored", || odom.pose("map").is_ok());
    assert_eq!(
        s2.reg().transform_profile(&tid)?["remote"]["state"],
        jvalue!("ok")
    );

    // remove_subsystem で相手の System が外れたら写しを消す (Transform は残す)
    s2.reg().remove_remote_system(&s1.uuid())?;
    let e = error_text(odom.pose("map"));
    assert!(
        e.contains("NoCommonAnchorError") && e.contains("removed"),
        "{e}"
    );
    assert!(s2.reg().transform_list()?.contains(&tid));
    // 取り込みは続くので、相手が答えればまた戻る
    wait_until("restored after remove", || odom.pose("map").is_ok());
    Ok(())
}

/// 取り込みで写しの Anchor の木が規則を破るとき (§19.3)、その B' を作らない
#[test]
fn replica_tree_breaking_the_rule_is_not_made() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8050")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8051")?;
    s1.create("static_anchor", "map")?;
    s1.create("part", "a")?;
    s1.create("part", "b")?;
    s1.reg().create_transforms(
        &[
            tf("map", "a", (1.0, 0.0, 0.0), Quaternion::identity()),
            tf("map", "b", (2.0, 0.0, 0.0), Quaternion::identity()),
        ],
        "",
    )?;
    let c = s2.create("part", "c")?;
    let d = s2.create("part", "d")?;
    // c は a' と b' の両方の子になる (それぞれ別の Transform としては作れる)
    s2.reg().create_transforms(
        &[
            tf(
                &s1.remote_id("a", "part"),
                "c",
                (0.0, 0.0, 0.0),
                Quaternion::identity(),
            )
            .rate(100.0),
            tf(
                &s1.remote_id("b", "part"),
                "c",
                (0.0, 0.0, 0.0),
                Quaternion::identity(),
            )
            .rate(100.0),
        ],
        "",
    )?;
    s2.start()?;
    std::thread::sleep(Duration::from_millis(200));
    assert!(s2.reg().anchor_list()?.is_empty());
    assert!(c.pose("map").is_err());

    // 片方を消せば作れる (木の形が変われば検査し直す)
    let t = transform_identifier(&s1.remote_id("b", "part"), "core://core/container/c::part");
    s2.reg().destroy_transform(&t)?;
    wait_until("replica made", || c.pose("map").is_ok());
    assert_pose(
        &c.pose("map")?.pose,
        &pose(1.0, 0.0, 0.0, Quaternion::identity()),
    );
    let _ = d;
    Ok(())
}

/// 作れない Transform・答えられない問い合わせ (§19.2・§19.3・§19.4)
#[test]
fn cross_system_errors() -> JuizResult<()> {
    init();
    let net = Net::default();
    let mut s1 = Sys::new(&net, "127.0.0.1:8060")?;
    let mut s2 = Sys::new(&net, "127.0.0.1:8061")?;
    s1.create("static_anchor", "map")?;
    s2.create("part", "odom")?;
    s2.create("part", "other")?;
    let map_remote = s1.remote_id("map", "static_anchor");

    // 終点が別 System
    let e = error_text(s2.reg().create_transforms(
        &[tf(
            "odom",
            &map_remote,
            (0.0, 0.0, 0.0),
            Quaternion::identity(),
        )],
        "",
    ));
    assert!(e.contains("CrossSystemTransformError"), "{e}");
    // rate・timeout は正の数
    let e = error_text(
        s2.reg()
            .create_transforms(&[TransformManifest::new(&map_remote, "odom").rate(0.0)], ""),
    );
    assert!(e.contains("TransformManifestInvalidError"), "{e}");
    // 失敗したら何も残らない
    assert!(s2.reg().transform_list()?.is_empty());

    s2.reg().create_transforms(
        &[tf(&map_remote, "odom", (0.0, 0.0, 0.0), Quaternion::identity()).rate(100.0)],
        "",
    )?;
    s2.start()?;
    wait_until("import", || s2.reg().anchor_list().unwrap().len() == 1);
    // 写しを名前で端点にする Transform は作れない (始点はブローカー付きの ID で書く)
    let e = error_text(s2.reg().create_transforms(
        &[tf("map", "other", (0.0, 0.0, 0.0), Quaternion::identity())],
        "",
    ));
    assert!(e.contains("CrossSystemTransformError"), "{e}");
    let e = error_text(s2.reg().create_transforms(
        &[tf("other", "map", (0.0, 0.0, 0.0), Quaternion::identity())],
        "",
    ));
    assert!(e.contains("CrossSystemTransformError"), "{e}");
    // 写しの PoseState は答えない
    let e = error_text(s2.reg().pose_state(&PoseStateQuery::new(&map_remote), ""));
    assert!(e.contains("CrossSystemTransformError"), "{e}");

    // pose_state の since・system (§19.4)
    let reg1 = s1.reg();
    let first = reg1.pose_state(&PoseStateQuery::new("map"), "")?;
    assert_eq!(first["changed"], jvalue!(true));
    assert_eq!(first["entries"].as_array().unwrap().len(), 1);
    let mut q = PoseStateQuery::new("map");
    q.since = first["version"].as_u64();
    q.system = Some(s1.uuid());
    let second = reg1.pose_state(&q, "")?;
    assert_eq!(second["changed"], jvalue!(false));
    assert!(second["entries"].as_array().unwrap().is_empty());
    q.system = Some("another-uuid".to_owned());
    assert_eq!(reg1.pose_state(&q, "")?["changed"], jvalue!(true));

    // 自分の System の Transform に rate を書いても使わない (WARN) だけで作れる
    s2.reg()
        .create_transforms(&[TransformManifest::new("odom", "other").rate(5.0)], "")?;
    let t = s2.reg().transform_profile(&transform_identifier(
        "core://core/container/odom::part",
        "core://core/container/other::part",
    ))?;
    assert!(t.get("remote").is_none());
    Ok(())
}
