//! 位置姿勢と座標変換の値と計算 (docs/spec/transformations.md §3)
//!
//! すべて f64。辞書 (JSON) の形は `juiz_std.geometry` の `Vector3` / `Point3D` / `Quaternion` /
//! `Pose` / `Transform` と同じ。四元数の並びは (x, y, z, w)。
//!
//! Transform(P→C) = (t, q) は「子 C の座標を親 P の座標へ写す」変換 (ROS と同じ):
//! `p_P = q · p_C + t`。この値は「P を基準にした C の ContainerPose」と同じ数になるので、
//! [`Pose`] と [`Transform`] は同じ計算 (合成・逆) を持ち、互いに変換できる。

use std::ops::{Add, Mul, Neg, Sub};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// 正規化できない四元数とみなす長さ (spec §3.1)
pub const QUATERNION_NORM_EPSILON: f64 = 1e-12;

fn invalid(message: String) -> anyhow::Error {
    anyhow!(JuizError::InvalidPoseValueError { message })
}

/// 空間のベクトル (`juiz_std.geometry.Vector3`)。点 (`Point3D`) としても使う
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// 空間の点 (`juiz_std.geometry.Point3D`)。辞書の形は [`Vector3`] と同じ
pub type Point3D = Vector3;

impl Vector3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub const fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    pub fn dot(&self, rhs: &Vector3) -> f64 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn cross(&self, rhs: &Vector3) -> Vector3 {
        Vector3::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub fn norm(&self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn scale(&self, s: f64) -> Vector3 {
        Vector3::new(self.x * s, self.y * s, self.z * s)
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// NaN・無限大を含めばエラー (`InvalidPoseValueError`)
    pub fn validate(&self) -> JuizResult<()> {
        if self.is_finite() {
            Ok(())
        } else {
            Err(invalid(format!("vector {self:?} contains NaN or infinity")))
        }
    }
}

impl Add for Vector3 {
    type Output = Vector3;
    fn add(self, rhs: Self) -> Self::Output {
        Vector3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vector3 {
    type Output = Vector3;
    fn sub(self, rhs: Self) -> Self::Output {
        Vector3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Neg for Vector3 {
    type Output = Vector3;
    fn neg(self) -> Self::Output {
        Vector3::new(-self.x, -self.y, -self.z)
    }
}

/// 回転を表す四元数 (`juiz_std.geometry.Quaternion`)。並びは (x, y, z, w)、既定値は回転なし
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quaternion {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Default for Quaternion {
    fn default() -> Self {
        Self::identity()
    }
}

impl Quaternion {
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }

    /// 回転なし (0, 0, 0, 1)
    pub const fn identity() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    /// 軸 `axis` まわりに `angle` [rad] 回す回転。軸は正規化して使う
    pub fn from_axis_angle(axis: Vector3, angle: f64) -> JuizResult<Self> {
        axis.validate()?;
        let n = axis.norm();
        if n < QUATERNION_NORM_EPSILON || !angle.is_finite() {
            return Err(invalid(format!(
                "axis {axis:?} / angle {angle} can not make a rotation"
            )));
        }
        let (s, c) = (angle / 2.0).sin_cos();
        let a = axis.scale(s / n);
        Ok(Self::new(a.x, a.y, a.z, c))
    }

    /// ROS と同じ roll (x)・pitch (y)・yaw (z) [rad] から作る (固定軸 X→Y→Z の順に回す、q = q_z ⊗ q_y ⊗ q_x)
    pub fn from_rpy(roll: f64, pitch: f64, yaw: f64) -> Self {
        let (sr, cr) = (roll / 2.0).sin_cos();
        let (sp, cp) = (pitch / 2.0).sin_cos();
        let (sy, cy) = (yaw / 2.0).sin_cos();
        Self::new(
            sr * cp * cy - cr * sp * sy,
            cr * sp * cy + sr * cp * sy,
            cr * cp * sy - sr * sp * cy,
            cr * cp * cy + sr * sp * sy,
        )
    }

    /// [`Quaternion::from_rpy`] の逆。(roll, pitch, yaw) [rad] を返す
    pub fn to_rpy(&self) -> (f64, f64, f64) {
        let q = self.normalized().unwrap_or_default();
        let roll = (2.0 * (q.w * q.x + q.y * q.z)).atan2(1.0 - 2.0 * (q.x * q.x + q.y * q.y));
        let sinp = (2.0 * (q.w * q.y - q.z * q.x)).clamp(-1.0, 1.0);
        let pitch = sinp.asin();
        let yaw = (2.0 * (q.w * q.z + q.x * q.y)).atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z));
        (roll, pitch, yaw)
    }

    pub fn norm(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt()
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }

    /// 長さ 1 にしたもの。長さが 0 (1e-12 未満)・NaN・無限大はエラー (`InvalidPoseValueError`)。
    /// w ≥ 0 へのそろえはしない (spec §3.1)
    pub fn normalized(&self) -> JuizResult<Self> {
        if !self.is_finite() {
            return Err(invalid(format!(
                "quaternion {self:?} contains NaN or infinity"
            )));
        }
        let n = self.norm();
        if n < QUATERNION_NORM_EPSILON {
            return Err(invalid(format!("quaternion {self:?} has zero length")));
        }
        Ok(Self::new(self.x / n, self.y / n, self.z / n, self.w / n))
    }

    /// 共役 (−x, −y, −z, w)。単位四元数なら逆回転
    pub fn conjugate(&self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }

    /// 逆回転。単位四元数を前提に共役を返す
    pub fn inverse(&self) -> Self {
        self.conjugate()
    }

    /// 点・ベクトルを回す (q · v · q⁻¹)。単位四元数を前提とする
    pub fn rotate(&self, v: &Vector3) -> Vector3 {
        let u = Vector3::new(self.x, self.y, self.z);
        let t = u.cross(v).scale(2.0);
        *v + t.scale(self.w) + u.cross(&t)
    }

    /// 2 つが同じ回転を表すときの角度の差 [rad] (q と −q は同じ回転とみなす)
    pub fn angle_to(&self, other: &Quaternion) -> f64 {
        let a = self.normalized().unwrap_or_default();
        let b = other.normalized().unwrap_or_default();
        let d = (a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w)
            .abs()
            .min(1.0);
        2.0 * d.acos()
    }
}

/// ハミルトン積 (self ⊗ rhs)。rhs の回転の後に self の回転をする
impl Mul for Quaternion {
    type Output = Quaternion;
    fn mul(self, r: Self) -> Self::Output {
        let l = self;
        Quaternion::new(
            l.w * r.x + l.x * r.w + l.y * r.z - l.z * r.y,
            l.w * r.y - l.x * r.z + l.y * r.w + l.z * r.x,
            l.w * r.z + l.x * r.y - l.y * r.x + l.z * r.w,
            l.w * r.w - l.x * r.x - l.y * r.y - l.z * r.z,
        )
    }
}

/// 座標変換 (`juiz_std.geometry.Transform`)。子の座標を親の座標へ写す: `p_親 = rotation · p_子 + translation`
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub translation: Vector3,
    pub rotation: Quaternion,
}

impl Transform {
    pub const fn new(translation: Vector3, rotation: Quaternion) -> Self {
        Self {
            translation,
            rotation,
        }
    }

    /// 単位 ((0, 0, 0), (0, 0, 0, 1))
    pub const fn identity() -> Self {
        Self::new(Vector3::zero(), Quaternion::identity())
    }

    /// 合成 self ∘ rhs = (t1 + q1 · t2, q1 ⊗ q2) (spec §3.3)
    pub fn compose(&self, rhs: &Transform) -> Transform {
        Transform::new(
            self.translation + self.rotation.rotate(&rhs.translation),
            self.rotation * rhs.rotation,
        )
    }

    /// 逆 (−(q⁻¹ · t), q⁻¹) (spec §3.3)。回転は単位四元数を前提とする
    pub fn inverse(&self) -> Transform {
        let qi = self.rotation.inverse();
        Transform::new(-qi.rotate(&self.translation), qi)
    }

    /// 子の座標の点を親の座標へ写す
    pub fn transform_point(&self, p: &Vector3) -> Vector3 {
        self.rotation.rotate(p) + self.translation
    }

    /// 保存用に検査・正規化したもの。NaN・無限大、長さ 0 の四元数はエラー (`InvalidPoseValueError`)
    pub fn normalized(&self) -> JuizResult<Transform> {
        self.translation.validate()?;
        Ok(Transform::new(
            self.translation,
            self.rotation.normalized()?,
        ))
    }

    /// 並進の差と回転の角度の差がどちらも `eps` 以下か (四元数は q と −q を同じとみなす)
    pub fn approx_eq(&self, other: &Transform, eps: f64) -> bool {
        (self.translation - other.translation).norm() <= eps
            && self.rotation.angle_to(&other.rotation) <= eps
    }
}

impl Mul for Transform {
    type Output = Transform;
    fn mul(self, rhs: Self) -> Self::Output {
        self.compose(&rhs)
    }
}

/// 空間の姿勢 (`juiz_std.geometry.Pose`)。ContainerPose の値
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pose {
    pub position: Point3D,
    pub orientation: Quaternion,
}

impl Pose {
    pub const fn new(position: Point3D, orientation: Quaternion) -> Self {
        Self {
            position,
            orientation,
        }
    }

    /// 単位 (原点・回転なし)
    pub const fn identity() -> Self {
        Self::new(Vector3::zero(), Quaternion::identity())
    }

    /// 同じ数の [`Transform`]
    pub fn to_transform(&self) -> Transform {
        Transform::new(self.position, self.orientation)
    }

    /// 合成 self ∘ rhs (spec §3.3)。`pose_A(C) = pose_A(P) ∘ Transform(P→C)` に使う
    pub fn compose(&self, rhs: &Transform) -> Pose {
        self.to_transform().compose(rhs).into()
    }

    /// 逆 (spec §3.3)
    pub fn inverse(&self) -> Pose {
        self.to_transform().inverse().into()
    }

    /// `self` を基準 R の姿勢 pose_A(R)、`other` を pose_A(C) として、R から見た C の姿勢
    /// `pose_R(C) = pose_A(R)⁻¹ ∘ pose_A(C)` を返す (spec §3.4)
    pub fn relative(&self, other: &Pose) -> Pose {
        self.inverse().compose(&other.to_transform())
    }

    /// 保存用に検査・正規化したもの (`InvalidPoseValueError`)
    pub fn normalized(&self) -> JuizResult<Pose> {
        Ok(self.to_transform().normalized()?.into())
    }

    /// 位置の差と回転の角度の差がどちらも `eps` 以下か
    pub fn approx_eq(&self, other: &Pose, eps: f64) -> bool {
        self.to_transform().approx_eq(&other.to_transform(), eps)
    }
}

impl From<Transform> for Pose {
    fn from(t: Transform) -> Self {
        Pose::new(t.translation, t.rotation)
    }
}

impl From<Pose> for Transform {
    fn from(p: Pose) -> Self {
        p.to_transform()
    }
}

impl Mul<Transform> for Pose {
    type Output = Pose;
    fn mul(self, rhs: Transform) -> Self::Output {
        self.compose(&rhs)
    }
}

impl Mul<Pose> for Pose {
    type Output = Pose;
    fn mul(self, rhs: Pose) -> Self::Output {
        self.compose(&rhs.to_transform())
    }
}

/// 辞書との行き来。足りないキーは juiz_std の既定値 (0、回転なし) で埋める
macro_rules! value_conversions {
    ($t:ty, $name:expr) => {
        impl From<$t> for Value {
            fn from(v: $t) -> Value {
                serde_json::to_value(v).expect("geometry values always serialize")
            }
        }

        impl From<$t> for Capsule {
            fn from(v: $t) -> Capsule {
                Value::from(v).into()
            }
        }

        impl TryFrom<&Value> for $t {
            type Error = anyhow::Error;
            fn try_from(value: &Value) -> Result<Self, Self::Error> {
                <$t>::deserialize(value).map_err(|e| {
                    anyhow!(JuizError::DataConversionError {
                        message: format!("{} can not be read from {value}: {e}", $name)
                    })
                })
            }
        }

        impl TryFrom<Value> for $t {
            type Error = anyhow::Error;
            fn try_from(value: Value) -> Result<Self, Self::Error> {
                <$t>::try_from(&value)
            }
        }
    };
}

value_conversions!(Vector3, "Vector3");
value_conversions!(Quaternion, "Quaternion");
value_conversions!(Transform, "Transform");
value_conversions!(Pose, "Pose");
