//! 他言語・C ABI のプラグインの ContainerProcess から ContainerPose を読み書きする口
//! (docs/spec/transformations.md §12.3、issue #372)。
//!
//! ホスト (juiz_core) は C ABI のプラグインの ContainerProcess を呼ぶとき、呼び出し元 Container の
//! [`ContainerPoseHandle`] を包んだ [`JuizPosePort`] (C の関数ポインタの表) を作り、そのアドレスを
//! 呼び出しの引数に隠して載せる。
//!
//! - C++・Haskell: `CapsuleMap` の見えない欄 ([`CapsuleMap::pose_port`])。プラグインは libjuiz_sdk の
//!   [`juiz_container_pose_get`] / [`juiz_container_pose_set`] に `capsule_map*` を渡して読み書きする。
//! - Rust の C ABI プラグイン: 引数のフレームのヘッダの `pose_port`。グルーが [`PortRegistry`] を
//!   プラグインの中の `ContainerImpl` につなぐので、利用者は今まで通り `c.pose("map")` で読める。
//!
//! 境界を越えるのは `#[repr(C)]` の構造体と C の関数だけで、Rust の型 (`anyhow::Error`・`String`・`Arc`) は
//! 越えない。プラグインの juiz_sdk (libjuiz_sdk・プラグインに静的にリンクされた juiz_sdk) とホストの juiz_sdk は
//! 別の複製なので、エラーの種類も型ではなくコード (`JUIZ_*_ERROR`、-9xx) で運ぶ。

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use anyhow::anyhow;

use super::handle::{ContainerPoseHandle, TransformationsRegistry};
use crate::cabi::{JuizBytes, JuizBytesSink, JUIZ_OK};
use crate::geometry::{Header, Pose, PoseStamped, Quaternion, Time, Vector3};
use crate::prelude::*;

/// [`JuizPosePort`] の版。並びと型を変えるときに上げる (後ろに足すだけなら `struct_size` で見分ける)
pub const POSE_PORT_ABI_VERSION: u32 = 1;

// ---- エラーコード (bindings/cjuiz/include/juiz_sdk.h と同じ値) ----

/// Container が System の Transformations 登録所につながっていない。口が無い (古いホスト・ContainerProcess の外) ときも
pub const JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR: i64 = -901;
/// 基準と Container の両方を含む Anchor がない
pub const JUIZ_NO_COMMON_ANCHOR_ERROR: i64 = -902;
/// 根の Anchor・基準自身・基準が子孫の Container への書き込み
pub const JUIZ_POSE_NOT_WRITABLE_ERROR: i64 = -903;
/// 長さ 0 の四元数、NaN・無限大
pub const JUIZ_INVALID_POSE_VALUE_ERROR: i64 = -904;
/// そのほかの失敗 (引数が null、ロック、panic など)
pub const JUIZ_TRANSFORMATIONS_ERROR: i64 = -905;

// ---- 構造体 (juiz_sdk.h と同じ並び) ----

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JuizVector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// 四元数。並びは (x, y, z, w) (§3.1)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JuizQuaternion {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Default for JuizQuaternion {
    fn default() -> Self {
        JuizQuaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JuizPose {
    pub position: JuizVector3,
    pub orientation: JuizQuaternion,
}

/// 時刻 (`juiz_std.common.Time`)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JuizTime {
    pub sec: i64,
    /// 秒未満 [ns] (0〜999999999)
    pub nanosec: i32,
    pub reserved: u32,
}

/// 時刻つきの ContainerPose。基準 (`header.frame_id`) は読み書きで渡した基準なので持たない
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JuizPoseStamped {
    pub pose: JuizPose,
    pub stamp: JuizTime,
}

impl From<&Pose> for JuizPose {
    fn from(p: &Pose) -> Self {
        JuizPose {
            position: JuizVector3 {
                x: p.position.x,
                y: p.position.y,
                z: p.position.z,
            },
            orientation: JuizQuaternion {
                x: p.orientation.x,
                y: p.orientation.y,
                z: p.orientation.z,
                w: p.orientation.w,
            },
        }
    }
}

impl From<&JuizPose> for Pose {
    fn from(p: &JuizPose) -> Self {
        Pose::new(
            Vector3::new(p.position.x, p.position.y, p.position.z),
            Quaternion::new(
                p.orientation.x,
                p.orientation.y,
                p.orientation.z,
                p.orientation.w,
            ),
        )
    }
}

impl From<&PoseStamped> for JuizPoseStamped {
    fn from(p: &PoseStamped) -> Self {
        JuizPoseStamped {
            pose: (&p.pose).into(),
            stamp: JuizTime {
                sec: p.header.stamp.sec,
                nanosec: p.header.stamp.nanosec,
                reserved: 0,
            },
        }
    }
}

impl JuizPoseStamped {
    /// 基準の名前 `frame_id` を付けて [`PoseStamped`] にする
    pub fn to_pose_stamped(&self, frame_id: &str) -> PoseStamped {
        PoseStamped {
            header: Header {
                stamp: Time {
                    sec: self.stamp.sec,
                    nanosec: self.stamp.nanosec,
                },
                frame_id: frame_id.to_owned(),
            },
            pose: (&self.pose).into(),
        }
    }
}

/// 読む関数。`reference` は基準 Container の名前か ID (UTF-8)。成功なら 0 を返し、`out` に値を、
/// `frame_id` (null 可) に基準の名前 (`header.frame_id`) を書く。失敗なら負のコードを返し、`error` (null 可) に文を書く
pub type JuizPosePortGetFn = extern "C" fn(
    ctx: *const c_void,
    reference: JuizBytes,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64;

/// 書く関数。`pose` を検査・正規化して書き、書いた後の値を `out` に書く。ほかは [`JuizPosePortGetFn`] と同じ
pub type JuizPosePortSetFn = extern "C" fn(
    ctx: *const c_void,
    reference: JuizBytes,
    pose: *const JuizPose,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64;

/// 別の Container を読む関数 (§7.4、#409)。`target` は読みたい Container の名前か ID (UTF-8)。
/// 名前は呼び出し元 Container のネームスペースで解決する。ほかは [`JuizPosePortGetFn`] と同じ
pub type JuizPosePortLookupFn = extern "C" fn(
    ctx: *const c_void,
    target: JuizBytes,
    reference: JuizBytes,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64;

/// 呼び出し元 Container の ContainerPose の口 (§12.3)。ホストが作り、呼び出しの間だけ有効
#[repr(C)]
pub struct JuizPosePort {
    /// [`POSE_PORT_ABI_VERSION`]
    pub abi_version: u32,
    /// `size_of::<JuizPosePort>()`。後ろに項目を足すときに使う
    pub struct_size: u32,
    pub ctx: *const c_void,
    pub get: Option<JuizPosePortGetFn>,
    pub set: Option<JuizPosePortSetFn>,
    /// #409 で足した。`struct_size` がこの項目を含まない古いホストの口では使わない
    pub lookup: Option<JuizPosePortLookupFn>,
}

/// 版 1 の最初の表の大きさ (`lookup` を足す前)。これより小さい表は受け付けない
pub const POSE_PORT_MIN_STRUCT_SIZE: usize = std::mem::offset_of!(JuizPosePort, lookup);

impl JuizPosePort {
    /// `lookup` を持つ口なら、その関数 (古いホストの口では None)
    pub fn lookup_fn(&self) -> Option<JuizPosePortLookupFn> {
        if (self.struct_size as usize) < std::mem::offset_of!(JuizPosePort, lookup) + std::mem::size_of::<Option<JuizPosePortLookupFn>>() {
            return None;
        }
        self.lookup
    }
}

// ---- ホストの側 ----

/// ホストが ContainerProcess を呼ぶ間持つ口 (§12.3)。[`PosePortOwner::addr`] を引数に載せる。
/// 落とすと口は無効になるので、呼び出しが終わるまで持つこと
pub struct PosePortOwner {
    port: Box<JuizPosePort>,
    _handle: Box<ContainerPoseHandle>,
}

impl PosePortOwner {
    pub fn new(handle: ContainerPoseHandle) -> Self {
        Self::with_set(handle, host_port_set)
    }

    /// readonly の ContainerProcess に渡す口 (§14、#377)。書き込みは `PoseNotWritableError` (JUIZ_POSE_NOT_WRITABLE_ERROR)
    pub fn new_readonly(handle: ContainerPoseHandle) -> Self {
        Self::with_set(handle, host_port_set_readonly)
    }

    fn with_set(handle: ContainerPoseHandle, set: JuizPosePortSetFn) -> Self {
        let handle = Box::new(handle);
        let port = Box::new(JuizPosePort {
            abi_version: POSE_PORT_ABI_VERSION,
            struct_size: std::mem::size_of::<JuizPosePort>() as u32,
            ctx: &*handle as *const ContainerPoseHandle as *const c_void,
            get: Some(host_port_get),
            set: Some(set),
            lookup: Some(host_port_lookup),
        });
        PosePortOwner {
            port,
            _handle: handle,
        }
    }

    /// 口のアドレス。`CapsuleMap::set_pose_port` やフレームのヘッダの `pose_port` に載せる
    pub fn addr(&self) -> usize {
        &*self.port as *const JuizPosePort as usize
    }
}

/// ホストのエラーを C のコードにする (ホストの juiz_sdk の型なので downcast できる)
pub fn transformations_error_code(e: &anyhow::Error) -> i64 {
    match e.downcast_ref::<JuizError>() {
        Some(JuizError::TransformationsNotAttachedError { .. }) => {
            JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR
        }
        Some(JuizError::NoCommonAnchorError { .. }) => JUIZ_NO_COMMON_ANCHOR_ERROR,
        Some(JuizError::PoseNotWritableError { .. }) => JUIZ_POSE_NOT_WRITABLE_ERROR,
        Some(JuizError::InvalidPoseValueError { .. }) => JUIZ_INVALID_POSE_VALUE_ERROR,
        _ => JUIZ_TRANSFORMATIONS_ERROR,
    }
}

/// コードの名前 (エラー文用)
pub fn transformations_error_name(code: i64) -> &'static str {
    match code {
        JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR => "TransformationsNotAttachedError",
        JUIZ_NO_COMMON_ANCHOR_ERROR => "NoCommonAnchorError",
        JUIZ_POSE_NOT_WRITABLE_ERROR => "PoseNotWritableError",
        JUIZ_INVALID_POSE_VALUE_ERROR => "InvalidPoseValueError",
        _ => "TransformationsError",
    }
}

fn sink_write(sink: *const JuizBytesSink, bytes: &[u8]) {
    if sink.is_null() {
        return;
    }
    // SAFETY: 呼び手は sink を呼び出しの間有効に保つ
    let sink = unsafe { &*sink };
    if let Some(write) = sink.write {
        write(sink.ctx, JuizBytes::from_slice(bytes));
    }
}

/// ホストの口の中身を catch_unwind で囲み、失敗をコードと文にする
fn host_guarded(
    error: *const JuizBytesSink,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    body: impl FnOnce() -> JuizResult<PoseStamped>,
) -> i64 {
    let r = catch_unwind(AssertUnwindSafe(body));
    let (code, message) = match r {
        Ok(Ok(p)) => {
            if !out.is_null() {
                // SAFETY: 呼び手は out を呼び出しの間有効に保つ
                unsafe { *out = (&p).into() };
            }
            sink_write(frame_id, p.header.frame_id.as_bytes());
            return JUIZ_OK;
        }
        Ok(Err(e)) => (transformations_error_code(&e), format!("{e:#}")),
        Err(_) => (
            JUIZ_TRANSFORMATIONS_ERROR,
            "panicked while reading or writing ContainerPose".to_owned(),
        ),
    };
    let _ = catch_unwind(AssertUnwindSafe(|| sink_write(error, message.as_bytes())));
    code
}

fn reference_str<'a>(reference: &JuizBytes) -> JuizResult<&'a str> {
    bytes_str(reference, "reference")
}

fn bytes_str<'a>(bytes: &JuizBytes, name: &str) -> JuizResult<&'a str> {
    // SAFETY: 呼び手は bytes を呼び出しの間有効に保つ
    let slice = unsafe { bytes.as_slice() }.ok_or_else(|| {
        anyhow!(JuizError::ArgumentError {
            message: format!("{name} is null")
        })
    })?;
    std::str::from_utf8(slice).map_err(|e| {
        anyhow!(JuizError::ArgumentError {
            message: format!("{name} is not UTF-8 ({e})")
        })
    })
}

extern "C" fn host_port_lookup(
    ctx: *const c_void,
    target: JuizBytes,
    reference: JuizBytes,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64 {
    host_guarded(error, out, frame_id, || {
        // SAFETY: ctx は PosePortOwner が持つ ContainerPoseHandle
        let handle = unsafe { &*(ctx as *const ContainerPoseHandle) };
        handle.lookup(bytes_str(&target, "target")?, reference_str(&reference)?)
    })
}

extern "C" fn host_port_get(
    ctx: *const c_void,
    reference: JuizBytes,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64 {
    host_guarded(error, out, frame_id, || {
        // SAFETY: ctx は PosePortOwner が持つ ContainerPoseHandle
        let handle = unsafe { &*(ctx as *const ContainerPoseHandle) };
        handle.pose(reference_str(&reference)?)
    })
}

extern "C" fn host_port_set(
    ctx: *const c_void,
    reference: JuizBytes,
    pose: *const JuizPose,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64 {
    host_guarded(error, out, frame_id, || {
        // SAFETY: ctx は PosePortOwner が持つ ContainerPoseHandle
        let handle = unsafe { &*(ctx as *const ContainerPoseHandle) };
        if pose.is_null() {
            return Err(anyhow!(JuizError::ArgumentError {
                message: "pose is null".to_owned()
            }));
        }
        // SAFETY: 呼び手は pose を呼び出しの間有効に保つ
        let pose: Pose = unsafe { &*pose }.into();
        handle.set_pose(reference_str(&reference)?, &pose)
    })
}

extern "C" fn host_port_set_readonly(
    ctx: *const c_void,
    reference: JuizBytes,
    _pose: *const JuizPose,
    out: *mut JuizPoseStamped,
    frame_id: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64 {
    host_guarded(error, out, frame_id, || {
        // SAFETY: ctx は PosePortOwner が持つ ContainerPoseHandle
        let handle = unsafe { &*(ctx as *const ContainerPoseHandle) };
        Err(anyhow!(JuizError::PoseNotWritableError {
            container: handle.identifier().to_string(),
            reference: reference_str(&reference).unwrap_or("").to_owned(),
            message: "the pose can not be written from a readonly container process".to_owned(),
        }))
    })
}

// ---- プラグインの側 ----

/// アドレスから口を取り出す。0・知らない版・小さすぎる表なら None
///
/// # Safety
/// addr は 0 か、ホストが作った [`JuizPosePort`] で呼び出しの間有効なもの
pub unsafe fn port_from_addr<'a>(addr: usize) -> Option<&'a JuizPosePort> {
    if addr == 0 {
        return None;
    }
    let port = &*(addr as *const JuizPosePort);
    // 後ろに項目を足した表 (lookup、#409) を知らない古いホストの口も受け付ける
    if port.abi_version != POSE_PORT_ABI_VERSION
        || (port.struct_size as usize) < POSE_PORT_MIN_STRUCT_SIZE
    {
        return None;
    }
    Some(port)
}

extern "C" fn collect_into_vec(ctx: *mut c_void, bytes: JuizBytes) {
    // SAFETY: ctx は呼び手の Vec<u8>、bytes は呼び出しの間有効
    let buf = unsafe { &mut *(ctx as *mut Vec<u8>) };
    if let Some(s) = unsafe { bytes.as_slice() } {
        buf.clear();
        buf.extend_from_slice(s);
    }
}

/// 口を通した呼び出しの失敗 (コードと、ホストが書いた文)
#[derive(Debug, Clone)]
pub struct PortFailure {
    pub code: i64,
    pub message: String,
}

impl PortFailure {
    fn not_attached() -> Self {
        PortFailure {
            code: JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR,
            message: "the container process was not called with a ContainerPose port \
                      (call it from inside a container process of a juiz that supports Transformations)"
                .to_owned(),
        }
    }

    /// プラグインの juiz_sdk の `JuizError` にする
    pub fn into_error(self, container: &str, reference: &str) -> anyhow::Error {
        let e = match self.code {
            JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR => JuizError::TransformationsNotAttachedError {
                container: container.to_owned(),
            },
            JUIZ_NO_COMMON_ANCHOR_ERROR => JuizError::NoCommonAnchorError {
                container: container.to_owned(),
                reference: reference.to_owned(),
            },
            JUIZ_POSE_NOT_WRITABLE_ERROR => JuizError::PoseNotWritableError {
                container: container.to_owned(),
                reference: reference.to_owned(),
                message: self.message.clone(),
            },
            JUIZ_INVALID_POSE_VALUE_ERROR => JuizError::InvalidPoseValueError {
                message: self.message.clone(),
            },
            _ => {
                return anyhow!(
                    "{} ({}): {}",
                    transformations_error_name(self.code),
                    self.code,
                    self.message
                )
            }
        };
        anyhow!(e).context(self.message)
    }
}

/// 口で読む。値と基準の名前を返す
pub fn port_get(port: Option<&JuizPosePort>, reference: &str) -> Result<(JuizPoseStamped, String), PortFailure> {
    let port = port.ok_or_else(PortFailure::not_attached)?;
    let get = port.get.ok_or_else(PortFailure::not_attached)?;
    let mut out = JuizPoseStamped::default();
    let mut frame_id: Vec<u8> = Vec::new();
    let mut error: Vec<u8> = Vec::new();
    let frame_sink = JuizBytesSink {
        ctx: &mut frame_id as *mut Vec<u8> as *mut c_void,
        write: Some(collect_into_vec),
    };
    let error_sink = JuizBytesSink {
        ctx: &mut error as *mut Vec<u8> as *mut c_void,
        write: Some(collect_into_vec),
    };
    let code = get(
        port.ctx,
        JuizBytes::from_slice(reference.as_bytes()),
        &mut out,
        &frame_sink,
        &error_sink,
    );
    finish(code, out, frame_id, error, reference)
}

/// 口で別の Container `target` を読む (§7.4)。値と基準の名前を返す。
/// `lookup` を持たない古いホストの口では `JUIZ_TRANSFORMATIONS_ERROR`
pub fn port_lookup(
    port: Option<&JuizPosePort>,
    target: &str,
    reference: &str,
) -> Result<(JuizPoseStamped, String), PortFailure> {
    let port = port.ok_or_else(PortFailure::not_attached)?;
    let lookup = port.lookup_fn().ok_or_else(|| PortFailure {
        code: JUIZ_TRANSFORMATIONS_ERROR,
        message: "the host of this container process is too old to support lookup \
                  (update juiz)"
            .to_owned(),
    })?;
    let mut out = JuizPoseStamped::default();
    let mut frame_id: Vec<u8> = Vec::new();
    let mut error: Vec<u8> = Vec::new();
    let frame_sink = JuizBytesSink {
        ctx: &mut frame_id as *mut Vec<u8> as *mut c_void,
        write: Some(collect_into_vec),
    };
    let error_sink = JuizBytesSink {
        ctx: &mut error as *mut Vec<u8> as *mut c_void,
        write: Some(collect_into_vec),
    };
    let code = lookup(
        port.ctx,
        JuizBytes::from_slice(target.as_bytes()),
        JuizBytes::from_slice(reference.as_bytes()),
        &mut out,
        &frame_sink,
        &error_sink,
    );
    finish(code, out, frame_id, error, reference)
}

/// 口で書く。書いた後の値と基準の名前を返す
pub fn port_set(
    port: Option<&JuizPosePort>,
    reference: &str,
    pose: &JuizPose,
) -> Result<(JuizPoseStamped, String), PortFailure> {
    let port = port.ok_or_else(PortFailure::not_attached)?;
    let set = port.set.ok_or_else(PortFailure::not_attached)?;
    let mut out = JuizPoseStamped::default();
    let mut frame_id: Vec<u8> = Vec::new();
    let mut error: Vec<u8> = Vec::new();
    let frame_sink = JuizBytesSink {
        ctx: &mut frame_id as *mut Vec<u8> as *mut c_void,
        write: Some(collect_into_vec),
    };
    let error_sink = JuizBytesSink {
        ctx: &mut error as *mut Vec<u8> as *mut c_void,
        write: Some(collect_into_vec),
    };
    let code = set(
        port.ctx,
        JuizBytes::from_slice(reference.as_bytes()),
        pose,
        &mut out,
        &frame_sink,
        &error_sink,
    );
    finish(code, out, frame_id, error, reference)
}

fn finish(
    code: i64,
    out: JuizPoseStamped,
    frame_id: Vec<u8>,
    error: Vec<u8>,
    reference: &str,
) -> Result<(JuizPoseStamped, String), PortFailure> {
    if code != JUIZ_OK {
        let message = if error.is_empty() {
            format!("returned {code} ({})", transformations_error_name(code))
        } else {
            String::from_utf8_lossy(&error).into_owned()
        };
        return Err(PortFailure { code, message });
    }
    let frame_id = if frame_id.is_empty() {
        reference.to_owned()
    } else {
        String::from_utf8_lossy(&frame_id).into_owned()
    };
    Ok((out, frame_id))
}

/// 口を通してホストの登録所に頼む [`TransformationsRegistry`]。Rust の C ABI プラグインのグルーが、
/// コンテナプロセスを呼ぶ間だけプラグインの中の `ContainerImpl` につなぐ (rust-plugin-abi.md §8.4)
pub struct PortRegistry {
    addr: usize,
}

// SAFETY: 口の関数はホストの ContainerPoseHandle (Send + Sync) を呼ぶだけ。口は呼び出しの間だけ使う
unsafe impl Send for PortRegistry {}
unsafe impl Sync for PortRegistry {}

impl PortRegistry {
    fn port(&self) -> Option<&JuizPosePort> {
        // SAFETY: addr はホストが作った口で、グルーはコンテナプロセスの呼び出しの間だけこの登録所をつなぐ
        unsafe { port_from_addr(self.addr) }
    }
}

impl TransformationsRegistry for PortRegistry {
    fn pose(&self, container: &ContainerIdentifier, reference: &str) -> JuizResult<PoseStamped> {
        let (p, frame_id) =
            port_get(self.port(), reference).map_err(|f| f.into_error(&container.to_string(), reference))?;
        Ok(p.to_pose_stamped(&frame_id))
    }

    fn set_pose(
        &self,
        container: &ContainerIdentifier,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        let (p, frame_id) = port_set(self.port(), reference, &pose.into())
            .map_err(|f| f.into_error(&container.to_string(), reference))?;
        Ok(p.to_pose_stamped(&frame_id))
    }

    fn lookup(
        &self,
        _caller: &ContainerIdentifier,
        target: &str,
        reference: &str,
    ) -> JuizResult<PoseStamped> {
        let (p, frame_id) =
            port_lookup(self.port(), target, reference).map_err(|f| f.into_error(target, reference))?;
        Ok(p.to_pose_stamped(&frame_id))
    }
}

/// プラグインの中の Container に、呼び出しの間だけ口をつなぐ。落とすと外す
pub struct PortAttachment {
    handle: ContainerPoseHandle,
    _registry: Option<Arc<dyn TransformationsRegistry>>,
}

impl PortAttachment {
    /// addr が 0 なら何もつながない (読み書きは `TransformationsNotAttachedError`)
    pub fn attach(handle: ContainerPoseHandle, addr: usize) -> JuizResult<Self> {
        if addr == 0 {
            return Ok(PortAttachment {
                handle,
                _registry: None,
            });
        }
        let registry: Arc<dyn TransformationsRegistry> = Arc::new(PortRegistry { addr });
        handle.attach(Arc::downgrade(&registry))?;
        Ok(PortAttachment {
            handle,
            _registry: Some(registry),
        })
    }
}

impl Drop for PortAttachment {
    fn drop(&mut self) {
        if self._registry.is_some() {
            let _ = self.handle.detach();
        }
    }
}

// ---- readonly のコンテナプロセス (§14、#377) ----
//
// readonly のコンテナプロセスは同じコンテナに対して並行に呼ばれる。呼び出しごとに口をつなぎ替えると、
// 先に終わった呼び出しが後の呼び出しの口を外してしまうので、コンテナには「このスレッドで呼び出し中の口」を
// 引く登録所を 1 つだけつなぎ、呼び出しの数を数えて最後の 1 つが終わったら外す。

thread_local! {
    /// このスレッドで呼び出し中の (コンテナのキー, 口のアドレス)。入れ子があるので積む
    static CALL_PORTS: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
}

/// コンテナのキー → (呼び出し中の数, つないだ登録所)
type SharedPorts = std::sync::Mutex<std::collections::HashMap<usize, (usize, Arc<dyn TransformationsRegistry>)>>;

fn shared_ports() -> &'static SharedPorts {
    static PORTS: std::sync::OnceLock<SharedPorts> = std::sync::OnceLock::new();
    PORTS.get_or_init(Default::default)
}

/// 呼び出したスレッドの口を引く [`TransformationsRegistry`]。別のスレッドからは `TransformationsNotAttachedError`
struct ThreadPortRegistry {
    key: usize,
}

impl ThreadPortRegistry {
    fn port(&self) -> Option<&JuizPosePort> {
        let addr = CALL_PORTS
            .try_with(|c| c.borrow().iter().rev().find(|(k, _)| *k == self.key).map(|(_, a)| *a))
            .ok()
            .flatten()?;
        // SAFETY: addr はこのスレッドで呼び出し中のコンテナプロセスにホストが渡した口で、呼び出しが終わるまで有効
        unsafe { port_from_addr(addr) }
    }
}

impl TransformationsRegistry for ThreadPortRegistry {
    fn pose(&self, container: &ContainerIdentifier, reference: &str) -> JuizResult<PoseStamped> {
        let (p, frame_id) =
            port_get(self.port(), reference).map_err(|f| f.into_error(&container.to_string(), reference))?;
        Ok(p.to_pose_stamped(&frame_id))
    }

    fn set_pose(
        &self,
        container: &ContainerIdentifier,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        let (p, frame_id) = port_set(self.port(), reference, &pose.into())
            .map_err(|f| f.into_error(&container.to_string(), reference))?;
        Ok(p.to_pose_stamped(&frame_id))
    }

    fn lookup(
        &self,
        _caller: &ContainerIdentifier,
        target: &str,
        reference: &str,
    ) -> JuizResult<PoseStamped> {
        let (p, frame_id) =
            port_lookup(self.port(), target, reference).map_err(|f| f.into_error(target, reference))?;
        Ok(p.to_pose_stamped(&frame_id))
    }
}

/// readonly のコンテナプロセスの呼び出しの間、プラグインの中の Container に口をつなぐ。落とすと外す。
/// 同じ Container の呼び出しが並行していても、最後の 1 つが終わるまで外さない
pub struct SharedPortAttachment {
    handle: ContainerPoseHandle,
    key: usize,
    attached: bool,
}

impl SharedPortAttachment {
    /// addr が 0 なら何もつながない (読み書きは `TransformationsNotAttachedError`)
    pub fn attach(handle: ContainerPoseHandle, addr: usize) -> JuizResult<Self> {
        let key = super::readonly::key_of(&handle);
        if addr == 0 {
            return Ok(SharedPortAttachment { handle, key, attached: false });
        }
        let mut ports = shared_ports().lock().map_err(|_| {
            anyhow!(JuizError::ObjectLockError {
                target: "SharedPortAttachment".to_owned()
            })
        })?;
        match ports.get_mut(&key) {
            Some((count, _)) => *count += 1,
            None => {
                let registry: Arc<dyn TransformationsRegistry> = Arc::new(ThreadPortRegistry { key });
                handle.attach(Arc::downgrade(&registry))?;
                ports.insert(key, (1, registry));
            }
        }
        let _ = CALL_PORTS.try_with(|c| c.borrow_mut().push((key, addr)));
        Ok(SharedPortAttachment { handle, key, attached: true })
    }
}

impl Drop for SharedPortAttachment {
    fn drop(&mut self) {
        if !self.attached {
            return;
        }
        let key = self.key;
        let _ = CALL_PORTS.try_with(|c| {
            let mut c = c.borrow_mut();
            if let Some(i) = c.iter().rposition(|(k, _)| *k == key) {
                c.remove(i);
            }
        });
        if let Ok(mut ports) = shared_ports().lock() {
            if let Some((count, _)) = ports.get_mut(&key) {
                *count -= 1;
                if *count == 0 {
                    ports.remove(&key);
                    let _ = self.handle.detach();
                }
            }
        }
    }
}

// ---- libjuiz_sdk の C の関数 (C++・Haskell のプラグインから呼ぶ、juiz_sdk.h) ----

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_last_error(message: &str) {
    let c = CString::new(message.replace('\0', " ")).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = c);
}

fn c_guarded(body: impl FnOnce() -> Result<(), PortFailure>) -> i64 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => {
            set_last_error("");
            JUIZ_OK
        }
        Ok(Err(f)) => {
            set_last_error(&f.message);
            f.code
        }
        Err(_) => {
            set_last_error("panicked while reading or writing ContainerPose");
            JUIZ_TRANSFORMATIONS_ERROR
        }
    }
}

fn null_argument(name: &str) -> PortFailure {
    PortFailure {
        code: JUIZ_TRANSFORMATIONS_ERROR,
        message: format!("{name} is null"),
    }
}

unsafe fn c_reference<'a>(reference: *const c_char) -> Result<&'a str, PortFailure> {
    c_string(reference, "reference")
}

unsafe fn c_string<'a>(s: *const c_char, name: &str) -> Result<&'a str, PortFailure> {
    if s.is_null() {
        return Err(null_argument(name));
    }
    CStr::from_ptr(s).to_str().map_err(|e| PortFailure {
        code: JUIZ_TRANSFORMATIONS_ERROR,
        message: format!("{name} is not UTF-8 ({e})"),
    })
}

/// ContainerProcess の引数 `cm` に載っている、呼び出し元 Container の ContainerPose を、基準 `reference`
/// (Container の名前か ID) から見て読む (transformations.md §12.3)。
/// 口が載っていない (ContainerProcess ではない、古いホスト) ときは `JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR`
#[no_mangle]
pub unsafe extern "C" fn juiz_container_pose_get(
    cm: *mut CapsuleMap,
    reference: *const c_char,
    out: *mut JuizPoseStamped,
) -> i64 {
    c_guarded(|| {
        let cm = cm.as_ref().ok_or_else(|| null_argument("capsule_map"))?;
        let reference = c_reference(reference)?;
        let (p, _) = port_get(port_from_addr(cm.pose_port()), reference)?;
        if !out.is_null() {
            *out = p;
        }
        Ok(())
    })
}

/// 呼び出し元 Container を、基準 `reference` から見て `pose` の位置姿勢にする (§7.2)。
/// 書き換わるのは親 → この Container の Transform。`out` (null 可) に書いた後の値を書く
#[no_mangle]
pub unsafe extern "C" fn juiz_container_pose_set(
    cm: *mut CapsuleMap,
    reference: *const c_char,
    pose: *const JuizPose,
    out: *mut JuizPoseStamped,
) -> i64 {
    c_guarded(|| {
        let cm = cm.as_ref().ok_or_else(|| null_argument("capsule_map"))?;
        let reference = c_reference(reference)?;
        let pose = pose.as_ref().ok_or_else(|| null_argument("pose"))?;
        let (p, _) = port_set(port_from_addr(cm.pose_port()), reference, pose)?;
        if !out.is_null() {
            *out = p;
        }
        Ok(())
    })
}

/// 呼び出し元 Container 以外の Container `target` (名前か ID) の、基準 `reference` から見た ContainerPose を読む
/// (§7.4、#409)。名前は呼び出し元 Container のネームスペースで解決する。読み出しだけなので
/// readonly ContainerProcess からも呼べる
#[no_mangle]
pub unsafe extern "C" fn juiz_container_pose_lookup(
    cm: *mut CapsuleMap,
    target: *const c_char,
    reference: *const c_char,
    out: *mut JuizPoseStamped,
) -> i64 {
    c_guarded(|| {
        let cm = cm.as_ref().ok_or_else(|| null_argument("capsule_map"))?;
        let target = c_string(target, "target")?;
        let reference = c_reference(reference)?;
        let (p, _) = port_lookup(port_from_addr(cm.pose_port()), target, reference)?;
        if !out.is_null() {
            *out = p;
        }
        Ok(())
    })
}

/// このスレッドで最後に呼んだ `juiz_container_pose_get` / `_set` / `_lookup` のエラー文 (成功なら空)。
/// 文字列は次にこのスレッドでそれらを呼ぶまで有効で、解放しない
#[no_mangle]
pub extern "C" fn juiz_container_pose_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr())
}

#[cfg(test)]
mod tests;
