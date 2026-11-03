//! geometry (Vector3 / Quaternion / Pose / Transform) のテスト。
//! 数値の例は docs/spec/transformations.md §3.5 (全言語のテストで同じ値を確かめる)

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

use juiz_sdk::geometry::{
    Pose, PoseStamped, Quaternion, Time, Transform, TransformStamped, Vector3,
};
use juiz_sdk::prelude::*;

const EPS: f64 = 1e-9;
const S: f64 = 0.7071067811865476;

fn assert_vec(v: Vector3, x: f64, y: f64, z: f64) {
    assert!(
        (v - Vector3::new(x, y, z)).norm() <= EPS,
        "{v:?} != ({x}, {y}, {z})"
    );
}

fn assert_rot(q: Quaternion, x: f64, y: f64, z: f64, w: f64) {
    assert!(
        q.angle_to(&Quaternion::new(x, y, z, w)) <= EPS,
        "{q:?} != ({x}, {y}, {z}, {w})"
    );
}

/// §3.5 の木: map → base0 → camera0
fn map_base0() -> Transform {
    Transform::new(Vector3::new(1.0, 0.0, 0.0), Quaternion::new(0.0, 0.0, S, S))
}

fn base0_camera0() -> Transform {
    Transform::new(Vector3::new(0.1, 0.0, 0.3), Quaternion::identity())
}

#[test]
fn spec_example_poses() {
    // pose_map(map) = 単位、pose_map(C) = pose_map(P) ∘ Transform(P→C)
    let pose_map_base0 = Pose::identity() * map_base0();
    let pose_map_camera0 = pose_map_base0 * base0_camera0();

    // pose_map(camera0)
    assert_vec(pose_map_camera0.position, 1.0, 0.1, 0.3);
    assert_rot(pose_map_camera0.orientation, 0.0, 0.0, S, S);

    // pose_base0(camera0) = pose_map(base0)⁻¹ ∘ pose_map(camera0)
    let p = pose_map_base0.relative(&pose_map_camera0);
    assert_vec(p.position, 0.1, 0.0, 0.3);
    assert_rot(p.orientation, 0.0, 0.0, 0.0, 1.0);

    // pose_camera0(map) = pose_map(camera0)⁻¹ ∘ pose_map(map)
    let p = pose_map_camera0.relative(&Pose::identity());
    assert_vec(p.position, -0.1, 1.0, -0.3);
    assert_rot(p.orientation, 0.0, 0.0, -S, S);
}

#[test]
fn spec_example_set_pose_rewrites_parent_transform() {
    // camera0 に set_pose("map", (1, 0.2, 0.3), z90°) をすると
    // Transform(base0→camera0) = pose_map(base0)⁻¹ ∘ pose_map(map) ∘ p になる (§7.2 の 4)
    let pose_map_base0 = Pose::identity() * map_base0();
    let p = Pose::new(Vector3::new(1.0, 0.2, 0.3), Quaternion::new(0.0, 0.0, S, S));
    let t = pose_map_base0.inverse() * Pose::identity() * p.to_transform();
    assert_vec(t.position, 0.2, 0.0, 0.3);
    assert_rot(t.orientation, 0.0, 0.0, 0.0, 1.0);
}

#[test]
fn compose_and_inverse_follow_spec_formulas() {
    let a = Transform::new(
        Vector3::new(1.0, 2.0, 3.0),
        Quaternion::from_rpy(0.3, -0.2, 1.1),
    );
    let b = Transform::new(
        Vector3::new(-0.5, 0.4, 0.2),
        Quaternion::from_rpy(-1.0, 0.5, 0.25),
    );

    // (t1, q1) ∘ (t2, q2) = (t1 + q1 · t2, q1 ⊗ q2)
    let c = a * b;
    assert!((c.translation - (a.translation + a.rotation.rotate(&b.translation))).norm() <= EPS);
    assert!(c.rotation.angle_to(&(a.rotation * b.rotation)) <= EPS);

    // 点を写す: (a ∘ b)(p) = a(b(p))
    let p = Vector3::new(0.7, -0.1, 2.0);
    assert!((c.transform_point(&p) - a.transform_point(&b.transform_point(&p))).norm() <= EPS);

    // 逆との合成は単位
    assert!((a * a.inverse()).approx_eq(&Transform::identity(), EPS));
    assert!((a.inverse() * a).approx_eq(&Transform::identity(), EPS));
    // 結合則
    assert!(((a * b) * a).approx_eq(&(a * (b * a)), EPS));
}

#[test]
fn rotation_direction_is_child_to_parent() {
    // 親から見て z 軸まわり 90° 回った子の x 軸は、親の y 軸
    let t = Transform::new(
        Vector3::zero(),
        Quaternion::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), FRAC_PI_2).unwrap(),
    );
    assert_vec(
        t.transform_point(&Vector3::new(1.0, 0.0, 0.0)),
        0.0,
        1.0,
        0.0,
    );
    assert_rot(t.rotation, 0.0, 0.0, S, S);
}

#[test]
fn rpy_round_trip_and_matches_axis_angle() {
    for (r, p, y) in [(0.3, -0.2, 1.1), (FRAC_PI_4, 0.0, 0.0), (0.0, 0.0, -0.7)] {
        let q = Quaternion::from_rpy(r, p, y);
        let (r2, p2, y2) = q.to_rpy();
        assert!((r - r2).abs() <= EPS && (p - p2).abs() <= EPS && (y - y2).abs() <= EPS);
    }
    // 固定軸 X→Y→Z の順に回す: q = q_z ⊗ q_y ⊗ q_x
    let qx = Quaternion::from_axis_angle(Vector3::new(1.0, 0.0, 0.0), 0.3).unwrap();
    let qy = Quaternion::from_axis_angle(Vector3::new(0.0, 1.0, 0.0), -0.2).unwrap();
    let qz = Quaternion::from_axis_angle(Vector3::new(0.0, 0.0, 1.0), 1.1).unwrap();
    assert!(Quaternion::from_rpy(0.3, -0.2, 1.1).angle_to(&(qz * qy * qx)) <= EPS);
}

#[test]
fn normalize_and_reject_invalid_values() {
    let q = Quaternion::new(0.0, 0.0, 2.0, 2.0).normalized().unwrap();
    assert!((q.norm() - 1.0).abs() <= EPS);
    assert_rot(q, 0.0, 0.0, S, S);

    // w ≥ 0 へのそろえはしない
    let q = Quaternion::new(0.0, 0.0, 0.0, -2.0).normalized().unwrap();
    assert_eq!(q.w, -1.0);
    // q と −q は同じ回転
    assert!(q.angle_to(&Quaternion::identity()) <= EPS);

    for bad in [
        Quaternion::new(0.0, 0.0, 0.0, 0.0),
        Quaternion::new(0.0, 0.0, 0.0, 1e-13),
        Quaternion::new(f64::NAN, 0.0, 0.0, 1.0),
        Quaternion::new(0.0, f64::INFINITY, 0.0, 1.0),
    ] {
        let err = bad.normalized().unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::InvalidPoseValueError { .. })
        ));
    }

    let bad = Pose::new(Vector3::new(f64::NAN, 0.0, 0.0), Quaternion::identity());
    assert!(matches!(
        bad.normalized().unwrap_err().downcast_ref::<JuizError>(),
        Some(JuizError::InvalidPoseValueError { .. })
    ));
    assert!(Quaternion::from_axis_angle(Vector3::zero(), 1.0).is_err());
}

#[test]
fn values_use_juiz_std_dictionary_shape() {
    let pose = Pose::new(Vector3::new(1.0, 2.0, 3.0), Quaternion::identity());
    let v: Value = pose.into();
    assert_eq!(
        v,
        jvalue!({"position": {"x": 1.0, "y": 2.0, "z": 3.0}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}})
    );
    assert_eq!(Pose::try_from(&v).unwrap(), pose);

    let t: Value = map_base0().into();
    assert_eq!(t["translation"]["x"], 1.0);
    assert_eq!(t["rotation"]["w"], S);

    // 足りないキーは juiz_std の既定値
    let p = Pose::try_from(jvalue!({"position": {"y": 2.0}})).unwrap();
    assert_eq!(
        p,
        Pose::new(Vector3::new(0.0, 2.0, 0.0), Quaternion::identity())
    );
    let t = Transform::try_from(jvalue!({})).unwrap();
    assert_eq!(t, Transform::identity());

    // 形が違えばエラー
    let err = Pose::try_from(jvalue!({"position": "here"})).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::DataConversionError { .. })
    ));
}

#[test]
fn stamped_values_use_juiz_std_dictionary_shape() {
    let ps = PoseStamped::new("map", Time::new(10, 5), Pose::identity());
    let v: Value = ps.clone().into();
    assert_eq!(
        v["header"],
        jvalue!({"stamp": {"sec": 10, "nanosec": 5}, "frame_id": "map"})
    );
    assert_eq!(v["pose"]["orientation"]["w"], 1.0);
    assert_eq!(PoseStamped::try_from(&v).unwrap(), ps);
    assert_eq!(ps.reference(), "map");

    let ts = TransformStamped::new("base0", "camera0", Time::new(1, 0), base0_camera0());
    let v: Value = ts.clone().into();
    assert_eq!(v["header"]["frame_id"], "base0");
    assert_eq!(v["child_frame_id"], "camera0");
    assert_eq!(v["transform"]["translation"]["z"], 0.3);
    assert_eq!(TransformStamped::try_from(v).unwrap(), ts);
    assert_eq!((ts.source(), ts.destination()), ("base0", "camera0"));

    // Capsule にもなる (ContainerProcess の戻り値)
    let c: Capsule = ps.into();
    assert_eq!(c.as_value().unwrap()["header"]["frame_id"], "map");
}

#[test]
fn time_now_and_before_epoch() {
    let now = Time::now();
    assert!(now.sec > 1_700_000_000);
    assert!((0..1_000_000_000).contains(&now.nanosec));

    let before = std::time::UNIX_EPOCH - std::time::Duration::from_millis(1500);
    let t: Time = before.into();
    assert_eq!(t, Time::new(-2, 500_000_000));
    assert!((t.as_secs_f64() + 1.5).abs() <= EPS);
}
