//! geometry (Vec3 / Quaternion / Transform3D) のテスト

use approx::assert_relative_eq;
use juiz_sdk::geometry::{
    euler_xyz_from_quaternion, quaternion_from_euler_xyz, Orientation3D, Point3D, Pose3D,
    Quaternion, Transform3D, Vec3,
};
use nalgebra::{Point3, UnitQuaternion, Vector3, Vector4};
use std::f64::consts::FRAC_PI_2;

#[test]
fn vec3_construction_and_addition() {
    let z = Vec3::<f64>::zero();
    assert_eq!((z.x, z.y, z.z), (0.0, 0.0, 0.0));
    let s = Vec3::new(1.0, 2.0, 3.0) + Vec3::new(0.5, 0.5, 0.5);
    assert_eq!((s.x, s.y, s.z), (1.5, 2.5, 3.5));
}

#[test]
fn vec3_nalgebra_conversions() {
    let p: Point3D<f64> = Vector3::new(1.0, 2.0, 3.0).into();
    assert_eq!((p.x, p.y, p.z), (1.0, 2.0, 3.0));
    let v: Vector3<f64> = p.into();
    assert_eq!(v, Vector3::new(1.0, 2.0, 3.0));

    let p: Vec3<f64> = Point3::new(4.0, 5.0, 6.0).into();
    assert_eq!((p.x, p.y, p.z), (4.0, 5.0, 6.0));
    let np: Point3<f64> = p.into();
    assert_eq!(np, Point3::new(4.0, 5.0, 6.0));
}

#[test]
fn quaternion_conversions() {
    let q = Quaternion::<f64>::zero();
    assert_eq!((q.x, q.y, q.z, q.w), (0.0, 0.0, 0.0, 1.0));

    let o: Orientation3D<f64> = Vector4::new(0.1, 0.2, 0.3, 0.4).into();
    assert_eq!((o.x, o.y, o.z, o.w), (0.1, 0.2, 0.3, 0.4));
    let v: Vector4<f64> = o.into();
    assert_eq!(v, Vector4::new(0.1, 0.2, 0.3, 0.4));

    let uq = UnitQuaternion::from_euler_angles(0.0, 0.0, FRAC_PI_2);
    let q: Quaternion<f64> = uq.into();
    let back: UnitQuaternion<f64> = q.into();
    assert_relative_eq!(back.angle_to(&uq), 0.0, epsilon = 1e-12);
}

#[test]
fn euler_round_trip_for_single_axis_rotation() {
    for (r, p, y) in [(FRAC_PI_2, 0.0, 0.0), (0.0, 0.5, 0.0), (0.0, 0.0, -0.7)] {
        let e = euler_xyz_from_quaternion(quaternion_from_euler_xyz(r, p, y));
        assert_relative_eq!(e.x, r, epsilon = 1e-9);
        assert_relative_eq!(e.y, p, epsilon = 1e-9);
        assert_relative_eq!(e.z, y, epsilon = 1e-9);
    }
}

#[test]
fn transform_composition() {
    let t = Transform3D::<f64>::zero();
    assert_eq!((t.linear.x, t.angular.w), (0.0, 1.0));

    // 回転なし同士なら並進が足し合わされる
    let a = Transform3D::new(Vec3::new(1.0, 2.0, 3.0), Quaternion::zero());
    let b = Transform3D::new(Vec3::new(0.5, 0.5, 0.5), Quaternion::zero());
    let c = a * b;
    assert_relative_eq!(c.linear.x, 1.5);
    assert_relative_eq!(c.linear.y, 2.5);
    assert_relative_eq!(c.linear.z, 3.5);
    assert_relative_eq!(c.angular.w, 1.0);

    // Z 軸 90 度回転を後ろに掛けると、前の並進が回転される
    let a = Transform3D::new(Vec3::new(1.0, 0.0, 0.0), Quaternion::zero());
    let b = Transform3D::new(
        Vec3::new(0.0, 0.0, 0.0),
        quaternion_from_euler_xyz(0.0, 0.0, FRAC_PI_2),
    );
    let c = a * b;
    assert_relative_eq!(c.linear.x, 0.0, epsilon = 1e-12);
    assert_relative_eq!(c.linear.y, 1.0, epsilon = 1e-12);
    assert_relative_eq!(c.linear.z, 0.0, epsilon = 1e-12);
}

#[test]
fn pose_serializes_to_json() {
    let pose = Pose3D {
        position: Point3D::new(1.0f64, 2.0, 3.0),
        orientation: Orientation3D::zero(),
    };
    let v = juiz_sdk::serde_json::to_value(&pose).unwrap();
    assert_eq!(v["position"]["y"], 2.0);
    assert_eq!(v["orientation"]["w"], 1.0);
    let back: Pose3D<f64> = juiz_sdk::serde_json::from_value(v).unwrap();
    assert_eq!(back.position.z, 3.0);
}
