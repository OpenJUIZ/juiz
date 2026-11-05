//! ContainerPose の口 (transformations.md §12.3) のテスト。
//! ホストの登録所の代わりに、決まった値を返す登録所を使う

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use super::*;
use crate::cabi::JUIZ_OK;
use crate::geometry::{Pose, Transform, TransformStamped};

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// 書いた値を覚えて返す登録所。基準 "nowhere" は NoCommonAnchorError、"root" は PoseNotWritableError
struct FakeRegistry {
    written: Mutex<Option<(String, Pose)>>,
    /// set_transform で書いた (呼び出し元の名前, source, destination, 値)
    transform_written: Mutex<Option<(String, String, String, Transform)>>,
}

impl FakeRegistry {
    fn new() -> Arc<Self> {
        Arc::new(FakeRegistry {
            written: Mutex::new(None),
            transform_written: Mutex::new(None),
        })
    }
}

fn camera_in_map() -> Pose {
    Pose::new(
        Vector3::new(1.0, 0.1, 0.3),
        Quaternion::new(0.0, 0.0, H, H),
    )
}

impl TransformationsRegistry for FakeRegistry {
    fn pose(&self, container: &ContainerIdentifier, reference: &str) -> JuizResult<PoseStamped> {
        if reference == "nowhere" {
            return Err(anyhow!(JuizError::NoCommonAnchorError {
                container: container.to_string(),
                reference: reference.to_owned(),
            }));
        }
        Ok(PoseStamped::new(
            "map",
            Time {
                sec: 12,
                nanosec: 34,
            },
            camera_in_map(),
        ))
    }

    fn set_pose(
        &self,
        container: &ContainerIdentifier,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        if reference == "root" {
            return Err(anyhow!(JuizError::PoseNotWritableError {
                container: container.to_string(),
                reference: reference.to_owned(),
                message: "root".to_owned(),
            }));
        }
        *self.written.lock().unwrap() = Some((reference.to_owned(), *pose));
        Ok(PoseStamped::new(reference, Time { sec: 1, nanosec: 2 }, *pose))
    }

    /// 呼び出し元のネームスペースと target を frame_id に入れて返す。基準 "nowhere" は NoCommonAnchorError
    fn lookup(
        &self,
        caller: &ContainerIdentifier,
        target: &str,
        reference: &str,
    ) -> JuizResult<PoseStamped> {
        if reference == "nowhere" {
            return Err(anyhow!(JuizError::NoCommonAnchorError {
                container: target.to_owned(),
                reference: reference.to_owned(),
            }));
        }
        Ok(PoseStamped::new(
            &format!("{reference}<-{target}@{}", caller.name),
            Time { sec: 5, nanosec: 6 },
            lidar_on_robot(),
        ))
    }

    /// 書いた値を覚えて返す。source "forbidden" は TransformNotWritableError、"missing" は TransformNotFoundError
    fn set_transform(
        &self,
        caller: &ContainerIdentifier,
        source: &str,
        destination: &str,
        value: &Transform,
    ) -> JuizResult<TransformStamped> {
        if source == "forbidden" {
            return Err(anyhow!(JuizError::TransformNotWritableError {
                identifier: format!("{source} -> {destination}"),
                container: caller.to_string(),
                message: "not a writer".to_owned(),
            }));
        }
        if source == "missing" {
            return Err(anyhow!(JuizError::TransformNotFoundError {
                identifier: format!("{source} -> {destination}"),
            }));
        }
        *self.transform_written.lock().unwrap() =
            Some((caller.name.clone(), source.to_owned(), destination.to_owned(), *value));
        Ok(TransformStamped::new(
            &format!("ns/{source}"),
            &format!("ns/{destination}"),
            Time { sec: 7, nanosec: 8 },
            *value,
        ))
    }
}

fn lidar_on_robot() -> Pose {
    Pose::new(Vector3::new(0.2, 0.0, 0.4), Quaternion::new(0.0, 0.0, 0.0, 1.0))
}

fn camera_id() -> ContainerIdentifier {
    "core://core/container/camera0::part".to_owned().try_into().unwrap()
}

fn attached_handle(registry: &Arc<FakeRegistry>) -> ContainerPoseHandle {
    let handle = ContainerPoseHandle::new(camera_id());
    let r: Arc<dyn TransformationsRegistry> = registry.clone();
    handle.attach(Arc::downgrade(&r)).unwrap();
    // 登録所は呼び手 (テスト) が持つ
    std::mem::forget(r);
    handle
}

#[test]
fn layout_matches_c_header() {
    // juiz_sdk.h の構造体と同じ大きさ
    assert_eq!(std::mem::size_of::<JuizVector3>(), 24);
    assert_eq!(std::mem::size_of::<JuizQuaternion>(), 32);
    assert_eq!(std::mem::size_of::<JuizPose>(), 56);
    assert_eq!(std::mem::size_of::<JuizTime>(), 16);
    assert_eq!(std::mem::size_of::<JuizPoseStamped>(), 72);
    assert_eq!(std::mem::size_of::<JuizTransform>(), 56);
    assert_eq!(std::mem::size_of::<JuizTransformStamped>(), 72);
}

#[test]
fn c_functions_read_and_write_through_capsule_map() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let mut cm = CapsuleMap::new();
    cm.set_pose_port(owner.addr());
    let map = CString::new("map").unwrap();

    let mut out = JuizPoseStamped::default();
    let code = unsafe { juiz_container_pose_get(&mut cm, map.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_OK);
    assert_eq!(out.pose, JuizPose::from(&camera_in_map()));
    assert_eq!((out.stamp.sec, out.stamp.nanosec), (12, 34));

    // 書く値は正規化される (§3.1)
    let base0 = CString::new("base0").unwrap();
    let pose = JuizPose {
        position: JuizVector3 {
            x: 0.2,
            y: 0.0,
            z: 0.3,
        },
        orientation: JuizQuaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 2.0,
        },
    };
    let code = unsafe { juiz_container_pose_set(&mut cm, base0.as_ptr(), &pose, &mut out) };
    assert_eq!(code, JUIZ_OK);
    let (r, p) = registry.written.lock().unwrap().clone().unwrap();
    assert_eq!(r, "base0");
    assert_eq!(p.orientation.w, 1.0);
    assert_eq!(out.pose.orientation.w, 1.0);
    assert_eq!(out.pose.position.x, 0.2);
}

#[test]
fn c_functions_report_error_codes() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let mut cm = CapsuleMap::new();
    let map = CString::new("map").unwrap();
    let mut out = JuizPoseStamped::default();

    // 口が無い
    let code = unsafe { juiz_container_pose_get(&mut cm, map.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR);

    cm.set_pose_port(owner.addr());
    let nowhere = CString::new("nowhere").unwrap();
    let code = unsafe { juiz_container_pose_get(&mut cm, nowhere.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_NO_COMMON_ANCHOR_ERROR);
    let msg = unsafe { CStr::from_ptr(juiz_container_pose_last_error()) }
        .to_string_lossy()
        .into_owned();
    assert!(msg.contains("nowhere"), "{msg}");

    let root = CString::new("root").unwrap();
    let code = unsafe {
        juiz_container_pose_set(&mut cm, root.as_ptr(), &JuizPose::default(), &mut out)
    };
    assert_eq!(code, JUIZ_POSE_NOT_WRITABLE_ERROR);

    let bad = JuizPose {
        orientation: JuizQuaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        ..Default::default()
    };
    let code = unsafe { juiz_container_pose_set(&mut cm, map.as_ptr(), &bad, &mut out) };
    assert_eq!(code, JUIZ_INVALID_POSE_VALUE_ERROR);

    let code = unsafe { juiz_container_pose_get(&mut cm, std::ptr::null(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_ERROR);

    // 成功するとエラー文は空に戻る
    let code = unsafe { juiz_container_pose_get(&mut cm, map.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_OK);
    assert!(unsafe { CStr::from_ptr(juiz_container_pose_last_error()) }
        .to_bytes()
        .is_empty());
}

#[test]
fn detached_handle_reports_not_attached() {
    let handle = ContainerPoseHandle::new(camera_id());
    let owner = PosePortOwner::new(handle);
    let mut cm = CapsuleMap::new();
    cm.set_pose_port(owner.addr());
    let map = CString::new("map").unwrap();
    let code = unsafe { juiz_container_pose_get(&mut cm, map.as_ptr(), std::ptr::null_mut()) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR);
}

#[test]
fn port_registry_gives_rust_errors_and_values() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    // プラグインの中の Container (つながっていない) に、呼び出しの間だけ口をつなぐ
    let plugin_side = ContainerPoseHandle::new(camera_id());
    assert!(plugin_side.pose("map").is_err());
    {
        let _a = PortAttachment::attach(plugin_side.clone(), owner.addr()).unwrap();
        let p = plugin_side.pose("map").unwrap();
        assert_eq!(p.reference(), "map");
        assert!(p.pose.approx_eq(&camera_in_map(), 1e-12));
        let e = plugin_side.pose("nowhere").unwrap_err();
        assert!(matches!(
            e.downcast_ref::<JuizError>(),
            Some(JuizError::NoCommonAnchorError { .. })
        ));
        let p = plugin_side
            .set_pose("base0", &Pose::new(Vector3::new(0.2, 0.0, 0.3), Quaternion::identity()))
            .unwrap();
        assert_eq!(p.reference(), "base0");
        let e = plugin_side.set_pose("root", &Pose::new(Vector3::new(0.0, 0.0, 0.0), Quaternion::identity())).unwrap_err();
        assert!(matches!(
            e.downcast_ref::<JuizError>(),
            Some(JuizError::PoseNotWritableError { .. })
        ));
    }
    // 外した後はつながっていない
    let e = plugin_side.pose("map").unwrap_err();
    assert!(matches!(
        e.downcast_ref::<JuizError>(),
        Some(JuizError::TransformationsNotAttachedError { .. })
    ));
    // 口が 0 ならつながない
    let _a = PortAttachment::attach(plugin_side.clone(), 0).unwrap();
    assert!(plugin_side.pose("map").is_err());
}

#[test]
fn unknown_port_version_is_not_used() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let mut port = JuizPosePort {
        abi_version: 99,
        struct_size: std::mem::size_of::<JuizPosePort>() as u32,
        ctx: std::ptr::null(),
        get: None,
        set: None,
        lookup: None,
        set_transform: None,
    };
    assert!(unsafe { port_from_addr(&mut port as *mut JuizPosePort as usize) }.is_none());
    assert!(unsafe { port_from_addr(owner.addr()) }.is_some());
}

#[test]
fn c_lookup_reads_another_container() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let mut cm = CapsuleMap::new();
    let lidar = CString::new("lidar0").unwrap();
    let robot = CString::new("robot0").unwrap();
    let mut out = JuizPoseStamped::default();

    // 口が無い
    let code = unsafe { juiz_container_pose_lookup(&mut cm, lidar.as_ptr(), robot.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR);

    cm.set_pose_port(owner.addr());
    let code = unsafe { juiz_container_pose_lookup(&mut cm, lidar.as_ptr(), robot.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_OK);
    assert_eq!(out.pose, JuizPose::from(&lidar_on_robot()));
    assert_eq!((out.stamp.sec, out.stamp.nanosec), (5, 6));

    let nowhere = CString::new("nowhere").unwrap();
    let code = unsafe { juiz_container_pose_lookup(&mut cm, lidar.as_ptr(), nowhere.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_NO_COMMON_ANCHOR_ERROR);
    let code = unsafe { juiz_container_pose_lookup(&mut cm, std::ptr::null(), robot.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_ERROR);
    let msg = unsafe { CStr::from_ptr(juiz_container_pose_last_error()) }
        .to_string_lossy()
        .into_owned();
    assert!(msg.contains("target"), "{msg}");
}

#[test]
fn readonly_port_allows_lookup() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new_readonly(attached_handle(&registry));
    let mut cm = CapsuleMap::new();
    cm.set_pose_port(owner.addr());
    let lidar = CString::new("lidar0").unwrap();
    let robot = CString::new("robot0").unwrap();
    let mut out = JuizPoseStamped::default();
    let code = unsafe { juiz_container_pose_lookup(&mut cm, lidar.as_ptr(), robot.as_ptr(), &mut out) };
    assert_eq!(code, JUIZ_OK);
}

#[test]
fn port_registry_lookup_goes_through_host_handle() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let plugin_side = ContainerPoseHandle::new(camera_id());
    let _a = PortAttachment::attach(plugin_side.clone(), owner.addr()).unwrap();
    // 呼び出し元はホストの Container (camera0) になる
    let p = plugin_side.lookup("lidar0", "robot0").unwrap();
    assert_eq!(p.reference(), "robot0<-lidar0@camera0");
    assert!(p.pose.approx_eq(&lidar_on_robot(), 1e-12));
    let e = plugin_side.reader().lookup("lidar0", "nowhere").unwrap_err();
    assert!(matches!(
        e.downcast_ref::<JuizError>(),
        Some(JuizError::NoCommonAnchorError { container, .. }) if container == "lidar0"
    ));
}

#[test]
fn old_host_port_without_lookup_is_still_used() {
    // lookup を足す前のホストの表 (struct_size が小さい)。読み書きは使え、lookup だけエラー
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let host = unsafe { port_from_addr(owner.addr()) }.unwrap();
    let mut old = JuizPosePort {
        abi_version: POSE_PORT_ABI_VERSION,
        struct_size: POSE_PORT_MIN_STRUCT_SIZE as u32,
        ctx: host.ctx,
        get: host.get,
        set: host.set,
        lookup: host.lookup,
        set_transform: host.set_transform,
    };
    let addr = &mut old as *mut JuizPosePort as usize;
    let port = unsafe { port_from_addr(addr) };
    assert!(port.is_some());
    assert!(port_get(port, "map").is_ok());
    let f = port_lookup(port, "lidar0", "robot0").unwrap_err();
    assert_eq!(f.code, JUIZ_TRANSFORMATIONS_ERROR);
    assert!(f.message.contains("too old"), "{}", f.message);

    // さらに小さい表は受け付けない
    old.struct_size = (POSE_PORT_MIN_STRUCT_SIZE - 8) as u32;
    assert!(unsafe { port_from_addr(addr) }.is_none());
}

fn odom_offset() -> JuizTransform {
    JuizTransform {
        translation: JuizVector3 {
            x: 0.1,
            y: -0.2,
            z: 0.0,
        },
        // 長さ 2 の四元数 (正規化して書かれる)
        rotation: JuizQuaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 2.0,
        },
    }
}

#[test]
fn c_set_transform_writes_through_capsule_map() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let mut cm = CapsuleMap::new();
    let map = CString::new("map").unwrap();
    let odom = CString::new("odom").unwrap();
    let mut out = JuizTransformStamped::default();

    // 口が無い
    let code = unsafe { juiz_container_transform_set(&mut cm, map.as_ptr(), odom.as_ptr(), &odom_offset(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR);

    cm.set_pose_port(owner.addr());
    let code = unsafe { juiz_container_transform_set(&mut cm, map.as_ptr(), odom.as_ptr(), &odom_offset(), &mut out) };
    assert_eq!(code, JUIZ_OK);
    let (caller, s, d, t) = registry.transform_written.lock().unwrap().clone().unwrap();
    assert_eq!((caller.as_str(), s.as_str(), d.as_str()), ("camera0", "map", "odom"));
    assert_eq!(t.rotation.w, 1.0);
    assert_eq!(out.transform.translation.x, 0.1);
    assert_eq!(out.transform.rotation.w, 1.0);
    assert_eq!((out.stamp.sec, out.stamp.nanosec), (7, 8));

    let forbidden = CString::new("forbidden").unwrap();
    let code = unsafe { juiz_container_transform_set(&mut cm, forbidden.as_ptr(), odom.as_ptr(), &odom_offset(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORM_NOT_WRITABLE_ERROR);
    let msg = unsafe { CStr::from_ptr(juiz_container_pose_last_error()) }
        .to_string_lossy()
        .into_owned();
    assert!(msg.contains("not a writer"), "{msg}");

    let missing = CString::new("missing").unwrap();
    let code = unsafe { juiz_container_transform_set(&mut cm, missing.as_ptr(), odom.as_ptr(), &odom_offset(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORM_NOT_FOUND_ERROR);

    let bad = JuizTransform {
        rotation: JuizQuaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
        },
        ..Default::default()
    };
    let code = unsafe { juiz_container_transform_set(&mut cm, map.as_ptr(), odom.as_ptr(), &bad, &mut out) };
    assert_eq!(code, JUIZ_INVALID_POSE_VALUE_ERROR);

    let code = unsafe { juiz_container_transform_set(&mut cm, map.as_ptr(), std::ptr::null(), &odom_offset(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_ERROR);
    let code = unsafe { juiz_container_transform_set(&mut cm, map.as_ptr(), odom.as_ptr(), std::ptr::null(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORMATIONS_ERROR);
}

#[test]
fn readonly_port_refuses_set_transform() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new_readonly(attached_handle(&registry));
    let mut cm = CapsuleMap::new();
    cm.set_pose_port(owner.addr());
    let map = CString::new("map").unwrap();
    let odom = CString::new("odom").unwrap();
    let mut out = JuizTransformStamped::default();
    let code = unsafe { juiz_container_transform_set(&mut cm, map.as_ptr(), odom.as_ptr(), &odom_offset(), &mut out) };
    assert_eq!(code, JUIZ_TRANSFORM_NOT_WRITABLE_ERROR);
    assert!(registry.transform_written.lock().unwrap().is_none());
}

#[test]
fn port_registry_set_transform_goes_through_host_handle() {
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let plugin_side = ContainerPoseHandle::new(camera_id());
    let _a = PortAttachment::attach(plugin_side.clone(), owner.addr()).unwrap();
    let value: Transform = (&odom_offset()).into();
    let t = plugin_side.set_transform("map", "odom", &value).unwrap();
    // 端点の名前はホストが返したもの
    assert_eq!((t.source(), t.destination()), ("ns/map", "ns/odom"));
    assert_eq!(t.transform.rotation.w, 1.0);
    let (caller, _, _, _) = registry.transform_written.lock().unwrap().clone().unwrap();
    assert_eq!(caller, "camera0");
    let e = plugin_side.set_transform("forbidden", "odom", &value).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<JuizError>(),
        Some(JuizError::TransformNotWritableError { .. })
    ));
    let e = plugin_side.set_transform("missing", "odom", &value).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<JuizError>(),
        Some(JuizError::TransformNotFoundError { .. })
    ));
}

#[test]
fn old_host_port_without_set_transform_is_still_used() {
    // set_transform を足す前 (#409) のホストの表。lookup までは使え、set_transform だけエラー
    let registry = FakeRegistry::new();
    let owner = PosePortOwner::new(attached_handle(&registry));
    let host = unsafe { port_from_addr(owner.addr()) }.unwrap();
    let old = JuizPosePort {
        abi_version: POSE_PORT_ABI_VERSION,
        struct_size: std::mem::offset_of!(JuizPosePort, set_transform) as u32,
        ctx: host.ctx,
        get: host.get,
        set: host.set,
        lookup: host.lookup,
        set_transform: host.set_transform,
    };
    let port = unsafe { port_from_addr(&old as *const JuizPosePort as usize) };
    assert!(port.is_some());
    assert!(port_lookup(port, "lidar0", "robot0").is_ok());
    let f = port_set_transform(port, "map", "odom", &odom_offset()).unwrap_err();
    assert_eq!(f.code, JUIZ_TRANSFORMATIONS_ERROR);
    assert!(f.message.contains("too old"), "{}", f.message);
}
