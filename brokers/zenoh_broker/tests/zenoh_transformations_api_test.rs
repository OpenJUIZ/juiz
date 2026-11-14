//! zenoh ブローカー経由で Transformations の API を呼べること (issue #370)
//!
//! System 自身の zenoh ブローカーに zenoh の BrokerProxy で繋ぎ、Anchor の作成・Transform の作成・
//! ContainerPose の読み書き・木・Pose 通知の登録を確かめる。数値は docs/spec/transformations.md §3.5 の例。
//! HTTP とローカルブローカーは juiz_core の transformations_api_test。

mod common;

use common::*;
use juiz_core::env_logger;
use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, Quaternion, Vector3};
use juiz_sdk::manifests::{PoseListenerManifest, TransformManifest};

const PORT: i64 = 5225;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

struct Part {}

fn record(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("pose")?.lock_as_value(|v| v.clone())?.into())
}

#[test]
fn zenoh_transformations_api() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let broker_name = format!("localhost:{PORT}");
    let manifest = server_manifest("zenoh_transformations_api_test", PORT, 8266, true);
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        {
            let mut cb = system.core_broker().lock_mut()?;
            let w = cb.worker_mut();
            let juiz_sdk::factory::ContainerFactoryStruct(m, constructor) =
                juiz_sdk::factory::container_factory(ContainerManifest::new("part"), |_| {
                    Ok(Box::new(Part {}))
                });
            w.store_mut()
                .containers
                .register_factory("part", container_factory_create(m, constructor)?)?;
            for n in ["base0", "camera0"] {
                w.create_container_ref("part", n, Vec::<(&str, Value)>::new().into())?;
            }
            let pm = ProcessManifest::new("record").add_object_arg("pose", "", jvalue!({}));
            w.store_mut()
                .processes
                .register_factory("record", process_factory_create(pm, record)?)?;
            w.create_process_ref(&ProcessManifest::new("record").name("rec0"))?;
        }

        let proxy = system.core_broker().lock()?.worker().broker_proxy(
            "zenoh",
            broker_name.as_str(),
            true,
        )?;
        let mut bp = juiz_write(&proxy)?;
        let z90 = Quaternion::new(0.0, 0.0, H, H);

        let profile = bp.container_create(
            &ContainerManifest::new("periodic_anchor").name("map"),
            CapsuleMap::new(),
        )?;
        assert_eq!(
            profile.anchor,
            Some(jvalue!({"kind": "periodic", "rate": 10.0}))
        );
        bp.transform_create(
            &TransformManifest::new("map", "base0")
                .translation(Vector3::new(1.0, 0.0, 0.0))
                .rotation(z90),
        )?;
        bp.transform_create(
            &TransformManifest::new("base0", "camera0").translation(Vector3::new(0.1, 0.0, 0.3)),
        )?;
        assert_eq!(bp.transform_list()?.len(), 2);
        assert_eq!(
            bp.anchor_list()?,
            vec!["core://core/container/map::periodic_anchor"]
        );

        let p = bp.container_pose("camera0", "map")?;
        assert!(
            p.pose
                .approx_eq(&Pose::new(Vector3::new(1.0, 0.1, 0.3), z90), 1e-9),
            "{p:?}"
        );
        let w = bp.container_set_pose(
            "camera0",
            "map",
            &Pose::new(Vector3::new(1.0, 0.2, 0.3), z90),
        )?;
        assert!(w
            .pose
            .approx_eq(&Pose::new(Vector3::new(1.0, 0.2, 0.3), z90), 1e-9));
        let tid =
            "transform://core://core/container/base0::part|core://core/container/camera0::part";
        let t = bp.transform_update(tid, Some(Vector3::new(0.1, 0.0, 0.3)), None)?;
        assert_eq!(t["transform"]["translation"]["x"], jvalue!(0.1));
        bp.anchor_update("map")?;
        let tree = bp.transformations_tree("map")?;
        assert_eq!(
            tree["children"][0]["children"][0]["identifier"],
            jvalue!("core://core/container/camera0::part")
        );

        let l = bp.pose_listener_create(&PoseListenerManifest::new("camera0", "rec0"))?;
        let lid = l["identifier"].as_str().unwrap().to_owned();
        assert_eq!(bp.pose_listener_list()?, vec![lid.clone()]);
        assert_eq!(bp.pose_listener_profile_full(&lid)?["arg"], jvalue!("pose"));
        bp.pose_listener_destroy(&lid)?;
        assert!(bp.pose_listener_list()?.is_empty());

        bp.transform_destroy(tid)?;
        let e = format!("{:#}", bp.container_pose("camera0", "map").unwrap_err());
        assert!(e.contains("No anchor contains"), "{e}");
        Ok(())
    })
}
