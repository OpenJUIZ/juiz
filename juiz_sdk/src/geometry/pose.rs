// use quaternion_core::Quaternion;
use num_traits::{Float, FloatConst};
use quaternion_core::{RotationSequence, RotationType};

use std::ops::{Add, Mul};

use serde::{Deserialize, Serialize};
use nalgebra::{base::{Vector3, Vector4}, RealField, UnitQuaternion};

#[derive(Serialize, Deserialize)]
pub struct Vec3<T: Float> {
    pub x: T,
    pub y: T,
    pub z: T,
}

#[allow(unused)]
impl<T: Float> Vec3<T> {

    pub fn zero() -> Self {
        Self{x: T::zero(), y: T::zero(), z: T::zero()}
    }

    pub fn new(x: T, y: T, z: T) -> Self {
        Self{x, y, z}
    }
}

impl<T: Float> Add for Vec3<T> {
    type Output = Vec3<T>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x+rhs.x, self.y+rhs.y, self.z+rhs.z)
    }
}

pub type Point3D<T> = Vec3<T>;

/// ```
/// use juiz_sdk::geometry::Point3D;
/// use nalgebra::base::Vector3;
/// let vec = Vector3::<f32>::new(0.1, 0.2, 0.3);
/// let p: Point3D<f32> = vec.into();
/// assert_eq!(p.x, 0.1);
/// assert_eq!(p.y, 0.2);
/// assert_eq!(p.z, 0.3);
/// ```
impl<T: Float> From<Vector3<T>> for Point3D<T> {
    fn from(value: Vector3<T>) -> Self {
        Point3D{
            x: value.data.0[0][0],
            y: value.data.0[0][1],
            z: value.data.0[0][2],
        }
    }
}

impl<T: Float> Into<Vector3<T>> for Point3D<T> {
    fn into(self) -> Vector3<T> {
        Vector3::new(self.x, self.y, self.z)
    }
}


impl<T: 'static + Float + std::fmt::Debug> From<nalgebra::geometry::Point3<T>> for Vec3<T> {
    fn from(value: nalgebra::geometry::Point3<T>) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z
        }
    }
}

impl<T: 'static + Float + std::fmt::Debug> Into<nalgebra::geometry::Point3<T>> for Vec3<T> {
    fn into(self) -> nalgebra::geometry::Point3<T> {
        nalgebra::geometry::Point3::new(self.x, self.y, self.z)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Vec4<T: Float> {
    pub x: T,
    pub y: T,
    pub z: T,
    pub w: T,
}

#[allow(unused)]
impl<T: Float> Vec4<T> {

    pub fn zero() -> Self {
        Self{x:T::zero(), y:T::zero(), z:T::zero(), w:T::one()}
    }

    pub fn new(x:T, y:T, z:T, w:T) -> Self {
        Self{x, y, z, w}
    }
}

pub type Quaternion<T> = Vec4<T>;
pub type Orientation3D<T> = Quaternion<T>;

/// ```
/// use juiz_sdk::geometry::Orientation3D;
/// use nalgebra::base::Vector4;
/// let vec = Vector4::<f32>::new(0.1, 0.2, 0.3, 0.4);
/// let p: Orientation3D<f32> = vec.into();
/// assert_eq!(p.x, 0.1);
/// assert_eq!(p.y, 0.2);
/// assert_eq!(p.z, 0.3);
/// assert_eq!(p.w, 0.4);
/// ```
impl<T: Float> From<Vector4<T>> for Orientation3D<T> {
    fn from(value: Vector4<T>) -> Self {
        Self {
            x: value.data.0[0][0],
            y: value.data.0[0][1],
            z: value.data.0[0][2],
            w: value.data.0[0][3],
        }
    }
}

impl<T: Float> Into<Vector4<T>> for Orientation3D<T> {
    fn into(self) -> Vector4<T> {
        Vector4::new(self.x, self.y, self.z, self.w)
    }
}

impl<T: Float+RealField> Into<UnitQuaternion<T>> for Quaternion<T> {
    fn into(self) -> UnitQuaternion<T> {
        UnitQuaternion::from_quaternion(nalgebra::Quaternion::<T>::new(self.w, self.x, self.y, self.z))
    }
}

impl<T: Float+RealField> From<UnitQuaternion<T>> for Quaternion<T> {
    fn from(value: UnitQuaternion<T>) -> Self {
        Quaternion::<T>{
            x: value.i,
            y: value.j,
            z: value.k,
            w: value.w
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Pose3D<T: Float> {
    pub position: Point3D<T>,
    pub orientation: Orientation3D<T>,
}

#[derive(Serialize, Deserialize)]
pub struct Transform3D<T: Float> {
    pub linear: Vec3<T>,
    pub angular: Quaternion<T>,
}

#[allow(unused)]
impl<T: Float> Transform3D<T> {

    pub fn zero() -> Self {
        Self{linear:Vec3::<T>::zero(), angular: Quaternion::<T>::zero()}
    }

    pub fn new(linear: Vec3<T>, angular: Quaternion<T>) -> Self {
        Self{linear, angular}
    }
}


#[allow(unused)]
pub fn quaternion_from_euler_xyz<F>(roll:F, pitch: F, yaw: F) -> Quaternion<F> where F: RealField + Float {
    let q = UnitQuaternion::from_euler_angles(roll, pitch, yaw);
    Quaternion::<F>::new(q.i, q.j, q.k, q.w)
}

#[allow(unused)]
pub fn euler_xyz_from_quaternion<F>(q: Quaternion<F>) -> Vec3<F> where F: FloatConst + Float {
    //let qq : UnitQuaternion<F> = q.into();
    //let (roll, pitch, yaw) = qq.euler_angles();
    let e = quaternion_core::to_euler_angles(RotationType::Intrinsic, RotationSequence::XYZ,  (q.w, [q.x, q.y, q.z]));
    Vec3::<F>::new(e[0], e[1], e[2])
}



#[allow(unused)]
pub use quaternion_from_euler_xyz as q_from_e;

/// ```
/// use approx::assert_relative_eq;
/// use juiz_sdk::geometry::{Transform3D, Vec3, Quaternion, quaternion_from_euler_xyz, euler_xyz_from_quaternion};
/// use nalgebra::RealField;
/// let vec1 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0.2, 0.3), Quaternion::<f32>::zero());
/// let vec2 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0.2, 0.3), Quaternion::<f32>::zero());
/// let vec3 = vec1 * vec2;
/// assert_eq!(vec3.linear.x, 0.2);
/// assert_eq!(vec3.linear.y, 0.4);
/// assert_eq!(vec3.linear.z, 0.6);
/// 
/// let vec4 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0., 0.), quaternion_from_euler_xyz(0., std::f32::consts::FRAC_PI_2, 0.));
/// let vec5 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0., 0.), Quaternion::<f32>::zero());
/// let vec6 = vec5 * vec4;
/// assert_relative_eq!(vec6.linear.x, 0.1);
/// assert_relative_eq!(vec6.linear.y, 0.0);
/// assert_relative_eq!(vec6.linear.z, -0.1);
/// let euler = euler_xyz_from_quaternion(vec6.angular);
/// assert_relative_eq!(euler.x, 0.0);
/// assert_relative_eq!(euler.y, std::f32::consts::FRAC_PI_2, epsilon = f32::EPSILON);
/// assert_relative_eq!(euler.z, 0.0);
/// 
/// let vec7 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0., 0.), quaternion_from_euler_xyz(0., std::f32::consts::FRAC_PI_2, 0.));
/// let vec8 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0., 0.), quaternion_from_euler_xyz(0., 0., std::f32::consts::FRAC_PI_2));
/// let vec9 = Transform3D::<f32>::new(Vec3::<f32>::new(0.1, 0., 0.), Quaternion::<f32>::zero());
/// let vec10 = vec9 * vec8 * vec7;
/// assert_relative_eq!(vec10.linear.x, 0.1, epsilon = f32::EPSILON);
/// assert_relative_eq!(vec10.linear.y, 0.1, epsilon = f32::EPSILON);
/// assert_relative_eq!(vec10.linear.z, -0.1, epsilon = f32::EPSILON);
/// let euler2 = euler_xyz_from_quaternion(vec10.angular);
/// assert_relative_eq!(euler2.x, -std::f32::consts::FRAC_PI_2, epsilon = f32::EPSILON*4.0);
/// assert_relative_eq!(euler2.y, std::f32::consts::FRAC_PI_2, epsilon = f32::EPSILON);
/// assert_relative_eq!(euler2.z, 0., epsilon = f32::EPSILON);
/// ```
impl<T: Float+RealField> Mul for Transform3D<T> {
    type Output = Transform3D<T>;

    fn mul(self, rhs: Self) -> Self::Output {
        let q1: UnitQuaternion<T> = self.angular.into();
        let q2: UnitQuaternion<T> = rhs.angular.into();
        Self {
            linear: rhs.linear + q2.transform_point(&self.linear.into()).into(),
            angular: q1.rotation_to(&q2).into(),
        }
    }
}