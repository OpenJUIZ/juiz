//! Rust の ContainerProcess から ContainerPose を読み書きするテスト用コンポーネント
//! (Transformations Step 5、issue #372、juiz_core/tests/integration/rust_transformations_test.rs)。
//!
//! C ABI (既定) で読むと、ホストはコンテナプロセスの引数のフレームに ContainerPose の口を載せ、
//! グルーがプラグインの中の ContainerImpl につなぐ (docs/spec/rust-plugin-abi.md §8.4)。
//! Rust ABI (conf の `abi: rust`) ではホストの ContainerImpl がそのまま渡る。どちらでも書き方は同じ。

use juiz_sdk::geometry::{Pose, Quaternion, Transform, Vector3};
use juiz_sdk::prelude::*;

pub struct RustPart {
    pub id: i64,
}

#[juiz_component_container]
fn rust_part() -> JuizResult<Box<RustPart>> {
    Ok(Box::new(RustPart { id: 0 }))
}

/// 基準 reference から見た自分の ContainerPose (juiz_std.geometry.PoseStamped の辞書)
#[juiz_component_container_process(container_type = "rust_part" arguments = { default = { reference = "map" } })]
fn rust_part_pose(c: &mut ContainerImpl<RustPart>, reference: String) -> JuizResult<Capsule> {
    let p = c.pose(&reference)?;
    Ok(p.into())
}

/// 自分を基準 reference から見て (x, y, z)・(qx, qy, qz, qw) にする
#[juiz_component_container_process(container_type = "rust_part" arguments = {
    default = { reference = "map" x = 0.0 y = 0.0 z = 0.0 qx = 0.0 qy = 0.0 qz = 0.0 qw = 1.0 }
})]
fn rust_part_set_pose(
    c: &mut ContainerImpl<RustPart>,
    reference: String,
    x: f64,
    y: f64,
    z: f64,
    qx: f64,
    qy: f64,
    qz: f64,
    qw: f64,
) -> JuizResult<Capsule> {
    let p = c.set_pose(
        &reference,
        Pose::new(Vector3::new(x, y, z), Quaternion::new(qx, qy, qz, qw)),
    )?;
    Ok(p.into())
}

/// 読み書きを試し、エラーの種類 (JuizError の名前) か "ok" を返す
#[juiz_component_container_process(container_type = "rust_part" arguments = { default = { op = "pose" reference = "map" } })]
fn rust_part_try(c: &mut ContainerImpl<RustPart>, op: String, reference: String) -> JuizResult<Capsule> {
    let r = match op.as_str() {
        "pose" => c.pose(&reference).map(|_| ()),
        "set_pose" => c
            .set_pose(&reference, Pose::new(Vector3::new(1.0, 0.0, 0.0), Quaternion::identity()))
            .map(|_| ()),
        "set_pose_zero" => c
            .set_pose(&reference, Pose::new(Vector3::new(0.0, 0.0, 0.0), Quaternion::new(0.0, 0.0, 0.0, 0.0)))
            .map(|_| ()),
        _ => Ok(()),
    };
    let name = match r {
        Ok(()) => "ok".to_owned(),
        Err(e) => match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
            Some(JuizError::NoCommonAnchorError { .. }) => "NoCommonAnchorError".to_owned(),
            Some(JuizError::PoseNotWritableError { .. }) => "PoseNotWritableError".to_owned(),
            Some(JuizError::InvalidPoseValueError { .. }) => "InvalidPoseValueError".to_owned(),
            Some(JuizError::TransformationsNotAttachedError { .. }) => {
                "TransformationsNotAttachedError".to_owned()
            }
            _ => format!("other: {e:#}"),
        },
    };
    Ok(jvalue!(name).into())
}

/// readonly の ContainerProcess (#377)。sleep_ms だけ待ってから、基準 reference から見た自分の ContainerPose を返す。
/// Container を &ContainerImpl で受け取るので readonly になり、同じ Container の readonly 同士は並行に走る
#[juiz_component_container_process(container_type = "rust_part" arguments = { default = { reference = "map" sleep_ms = 0 } })]
fn rust_part_peek(c: &ContainerImpl<RustPart>, reference: String, sleep_ms: i64) -> JuizResult<Capsule> {
    std::thread::sleep(std::time::Duration::from_millis(sleep_ms.max(0) as u64));
    let p = c.pose(&reference)?;
    Ok(p.into())
}

/// readonly でない ContainerProcess。sleep_ms だけ待って id を返す (並行に走らないことの比較用)
#[juiz_component_container_process(container_type = "rust_part" arguments = { default = { sleep_ms = 0 } })]
fn rust_part_sleep(c: &mut ContainerImpl<RustPart>, sleep_ms: i64) -> JuizResult<Capsule> {
    std::thread::sleep(std::time::Duration::from_millis(sleep_ms.max(0) as u64));
    c.t.id += 1;
    Ok(jvalue!(c.t.id).into())
}

/// readonly の中から読み書きを試す (#377)。&ContainerImpl では set_pose を呼べないので、
/// 口 (pose_handle) を取り出して書こうとする。PoseNotWritableError になる
#[juiz_component_container_process(container_type = "rust_part" arguments = { default = { op = "pose" reference = "map" } })]
fn rust_part_try_readonly(c: &ContainerImpl<RustPart>, op: String, reference: String) -> JuizResult<Capsule> {
    let r = match op.as_str() {
        "pose" => c.pose(&reference).map(|_| ()),
        "set_pose" => c
            .pose_handle()
            .set_pose(&reference, &Pose::new(Vector3::new(1.0, 0.0, 0.0), Quaternion::identity()))
            .map(|_| ()),
        // #410: Transform の書き込みも止まる (TransformNotWritableError)
        "set_transform" => c
            .pose_handle()
            .set_transform(&reference, "base0", &Transform::identity())
            .map(|_| ()),
        _ => Ok(()),
    };
    let name = match r {
        Ok(()) => "ok".to_owned(),
        Err(e) => match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
            Some(JuizError::PoseNotWritableError { .. }) => "PoseNotWritableError".to_owned(),
            Some(JuizError::TransformNotWritableError { .. }) => "TransformNotWritableError".to_owned(),
            _ => format!("other: {e:#}"),
        },
    };
    Ok(jvalue!(name).into())
}

/// 自分以外の Container target を基準 reference から見た ContainerPose (#409、tf の lookupTransform)。
/// 読み出しだけなので readonly (&ContainerImpl) で呼べる。木の外れは "NoCommonAnchorError" を返す
#[juiz_component_container_process(container_type = "rust_part" arguments = { default = { target = "base0" reference = "map" } })]
fn rust_part_lookup(c: &ContainerImpl<RustPart>, target: String, reference: String) -> JuizResult<Capsule> {
    match c.lookup(&target, &reference) {
        Ok(p) => Ok(p.into()),
        Err(e) => match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
            Some(JuizError::NoCommonAnchorError { .. }) => Ok(jvalue!("NoCommonAnchorError").into()),
            _ => Err(e),
        },
    }
}

/// Transform (source → destination) の値を (x, y, z)・(qx, qy, qz, qw) にする (#410)。書いた後の
/// TransformStamped の辞書を返す。書けない・無いときは "TransformNotWritableError" / "TransformNotFoundError" を返す
#[juiz_component_container_process(container_type = "rust_part" arguments = {
    default = { source = "map" destination = "base0" x = 0.0 y = 0.0 z = 0.0 qx = 0.0 qy = 0.0 qz = 0.0 qw = 1.0 }
})]
fn rust_part_set_transform(
    c: &mut ContainerImpl<RustPart>,
    source: String,
    destination: String,
    x: f64,
    y: f64,
    z: f64,
    qx: f64,
    qy: f64,
    qz: f64,
    qw: f64,
) -> JuizResult<Capsule> {
    let value = Transform::new(Vector3::new(x, y, z), Quaternion::new(qx, qy, qz, qw));
    match c.set_transform(&source, &destination, value) {
        Ok(t) => Ok(t.into()),
        Err(e) => match e.chain().find_map(|c| c.downcast_ref::<JuizError>()) {
            Some(JuizError::TransformNotWritableError { .. }) => Ok(jvalue!("TransformNotWritableError").into()),
            Some(JuizError::TransformNotFoundError { .. }) => Ok(jvalue!("TransformNotFoundError").into()),
            _ => Err(e),
        },
    }
}

juiz_component_manifest!(
    component_name = "pose_part_component"
    containers = {
        rust_part = [
            rust_part_pose,
            rust_part_set_pose,
            rust_part_try,
            rust_part_peek,
            rust_part_sleep,
            rust_part_try_readonly,
            rust_part_lookup,
            rust_part_set_transform
        ]
    }
);
