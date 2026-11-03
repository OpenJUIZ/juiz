//! ContainerPose の口 (transformations.md §12.3) のテスト。
//! ホストの登録所の代わりに、決まった値を返す登録所を使う

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use super::*;
use crate::cabi::JUIZ_OK;
use crate::geometry::Pose;

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// 書いた値を覚えて返す登録所。基準 "nowhere" は NoCommonAnchorError、"root" は PoseNotWritableError
struct FakeRegistry {
    written: Mutex<Option<(String, Pose)>>,
}

impl FakeRegistry {
    fn new() -> Arc<Self> {
        Arc::new(FakeRegistry {
            written: Mutex::new(None),
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
    };
    assert!(unsafe { port_from_addr(&mut port as *mut JuizPosePort as usize) }.is_none());
    assert!(unsafe { port_from_addr(owner.addr()) }.is_some());
}
