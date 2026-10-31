//! Rust プラグインの C ABI (docs/spec/rust-plugin-abi.md、issue #291 / #329)
//!
//! ホスト (juiz) とプラグインの間で、Rust の型 (`String`・`Vec`・`Box<dyn …>`・`JuizResult`・`Capsule`) を
//! 渡さず、`#[repr(C)]` の小さな構造体と `extern "C"` の関数だけを渡す。値は JZF1 フレーム
//! (docs/spec/large-data.md §3) のヘッダと本体で運ぶ。そのため、プラグインはホストと違う rustc・違う
//! 依存クレートの版でビルドしてもよい。
//!
//! このモジュールには次のものがある。
//!
//! - 境界の構造体 ([`JuizRustEntry`]・[`JuizBytes`]・[`JuizBytesSink`]・[`JuizFrameIn`]・[`JuizFrameOut`] など、§3・§4)
//! - プラグイン側のグルー ([`call_process`]・[`create_container`]・[`call_container_process`] など、§7)。
//!   フレームから `CapsuleMap` を組み立て直し、利用者の Rust の関数を呼び、戻り値をフレームで書く。
//!   エラーと panic はエラーコード (§6.1) と文に変える
//!
//! juiz_macro は各ファクトリ `<symbol>` について `<symbol>_cabi` を生成し、それが返す表からこのグルーを呼ぶ。

use std::any::Any;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde::Serialize;

use crate::containers::ContainerImpl;
use crate::frame::{read_frame_parts, FrameLimits, OutgoingFrame};
use crate::prelude::*;

pub mod host;

/// この juiz_sdk が作る入口の表の ABI の版 (§9)
pub const ABI_VERSION: u32 = 1;

/// ファイルの中の印 (§9.3)。`juiz pack` はプラグインのバイト列からこれを探す
pub const ABI_MARKER: &str = "juiz_rust_abi=1;";
const ABI_MARKER_C: &[u8] = b"juiz_rust_abi=1;\0";

/// C ABI の入口のシンボル名に付ける接尾辞 (§3.1)
pub const CABI_SYMBOL_SUFFIX: &str = "_cabi";

/// 入口の表の種類 (§3.2)
pub const KIND_PROCESS: u32 = 1;
pub const KIND_CONTAINER: u32 = 2;
pub const KIND_CONTAINER_PROCESS: u32 = 3;
pub const KIND_COMPONENT: u32 = 4;

/// エラーコード (§6.1)
pub const JUIZ_OK: i64 = 0;
pub const JUIZ_RUST_CALL_ERROR: i64 = -801;
pub const JUIZ_RUST_PANIC: i64 = -802;
pub const JUIZ_RUST_FRAME_ERROR: i64 = -803;
pub const JUIZ_RUST_HOST_REFUSED: i64 = -804;
pub const JUIZ_RUST_NULL_ARGUMENT: i64 = -805;

/// エラーコードの名前。知らないコードは "unknown"
pub fn error_code_name(code: i64) -> &'static str {
    match code {
        JUIZ_OK => "JUIZ_OK",
        JUIZ_RUST_CALL_ERROR => "JUIZ_RUST_CALL_ERROR",
        JUIZ_RUST_PANIC => "JUIZ_RUST_PANIC",
        JUIZ_RUST_FRAME_ERROR => "JUIZ_RUST_FRAME_ERROR",
        JUIZ_RUST_HOST_REFUSED => "JUIZ_RUST_HOST_REFUSED",
        JUIZ_RUST_NULL_ARGUMENT => "JUIZ_RUST_NULL_ARGUMENT",
        _ => "unknown",
    }
}

/// このプラグインが作る入口の表の ABI の版を返す (§9.2)。
/// ホストの読み込みには使わない (表の `abi_version` を見る)。道具と試験のために置く
#[no_mangle]
pub extern "C" fn juiz_rust_abi_version() -> u32 {
    ABI_VERSION
}

// ---- 構造体 (§4) ----

/// 借りたバイト列。持ち主は渡した側で、呼び出しが終わるまで有効
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JuizBytes {
    pub ptr: *const u8,
    pub len: usize,
}

impl JuizBytes {
    pub fn from_slice(s: &[u8]) -> Self {
        JuizBytes {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }

    pub const fn empty() -> Self {
        JuizBytes {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    /// バイト列として読む。ptr が null なら、len が 0 のときだけ空として読む
    ///
    /// # Safety
    /// ptr は len バイトの読める領域を指し、返した参照を使い終わるまで有効でなければならない
    pub unsafe fn as_slice<'a>(&self) -> Option<&'a [u8]> {
        if self.ptr.is_null() {
            return if self.len == 0 { Some(&[]) } else { None };
        }
        Some(std::slice::from_raw_parts(self.ptr, self.len))
    }
}

/// 小さなバイト列 (manifest・info・エラー文) の受け取り口。ホストが用意する
#[repr(C)]
pub struct JuizBytesSink {
    pub ctx: *mut c_void,
    /// 1 回だけ呼ぶ。ホストは複製する
    pub write: Option<extern "C" fn(ctx: *mut c_void, bytes: JuizBytes)>,
}

/// 引数のフレーム。ホストが用意し、呼び出しが終わるまで有効
#[repr(C)]
pub struct JuizFrameIn {
    /// ヘッダの JSON (UTF-8)
    pub header: JuizBytes,
    /// 本体の配列。ヘッダの bodies と同じ順
    pub bodies: *const JuizBytes,
    pub body_count: usize,
    /// Native の配列 (§10.3)。ヘッダの `{"native": <番号>}` が添字で指す
    pub natives: *const JuizRustNative,
    pub native_count: usize,
}

/// 戻り値の書き込み口。ホストが用意する
#[repr(C)]
pub struct JuizFrameOut {
    pub ctx: *mut c_void,
    /// ヘッダの JSON を渡す。ホストは読んで、本体ごとに最終的な領域を確保する
    pub begin: Option<extern "C" fn(ctx: *mut c_void, header: JuizBytes) -> i64>,
    /// 本体 index の書き込み先を受け取る。長さはヘッダの説明から決まる
    pub body: Option<
        extern "C" fn(ctx: *mut c_void, index: usize, ptr: *mut *mut u8, len: *mut usize) -> i64,
    >,
    /// Native をハンドルのまま渡す (§10.2)。null なら、プラグインは Native を encode して書く
    pub native: Option<extern "C" fn(ctx: *mut c_void, index: usize, native: JuizRustNative) -> i64>,
    /// エラー文 (UTF-8)。入口が負の値を返すときに 1 回だけ呼ぶ
    pub error: Option<extern "C" fn(ctx: *mut c_void, message: JuizBytes)>,
}

/// Native の関数の表 (§10.1)
#[repr(C)]
pub struct JuizRustNativeVtable {
    pub abi_version: u32,
    pub struct_size: u32,
    /// 共通形式にして out に書く。ヘッダは kind: response で、result がその値
    pub encode: Option<extern "C" fn(handle: *const c_void, out: *const JuizFrameOut) -> i64>,
    /// 解放する。1 回だけ呼ぶ
    pub destroy: Option<extern "C" fn(handle: *mut c_void)>,
}

/// プラグインの中に置いたままの Native (§10.1)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JuizRustNative {
    pub handle: *mut c_void,
    pub vtable: *const JuizRustNativeVtable,
    /// {"name": …, "fingerprint": …, "abi": …} の JSON
    pub type_tag: JuizBytes,
    /// 0 Value / 1 Image
    pub kind: u32,
    /// 引数のとき: 1 なら持ち主がプラグインに移る。戻り値ではいつも 1 (持ち主がホストに移る)
    pub owned: u32,
}

/// manifest / info を書く関数
pub type JuizRustManifestFn = extern "C" fn(out: *const JuizBytesSink) -> i64;
/// Process の呼び出し
pub type JuizRustCallFn = extern "C" fn(args: *const JuizFrameIn, out: *const JuizFrameOut) -> i64;
/// コンテナの作成
pub type JuizRustCreateFn = extern "C" fn(
    manifest_json: JuizBytes,
    args: *const JuizFrameIn,
    handle: *mut *mut c_void,
    error: *const JuizBytesSink,
) -> i64;
/// コンテナの破棄
pub type JuizRustDestroyFn = extern "C" fn(handle: *mut c_void);
/// コンテナプロセスの呼び出し
pub type JuizRustContainerCallFn =
    extern "C" fn(handle: *mut c_void, args: *const JuizFrameIn, out: *const JuizFrameOut) -> i64;

/// 入口の表 (§3.2)。プラグイン側が static に置き、`<symbol>_cabi()` がそのポインタを返す。
/// 並びと型を変えるときは ABI の版を上げる (§9.1)
#[repr(C)]
#[derive(Debug)]
pub struct JuizRustEntry {
    /// この表の ABI の版。今は 1
    pub abi_version: u32,
    /// sizeof(JuizRustEntry)。後ろに項目を足すときに使う
    pub struct_size: u32,
    /// 1 Process / 2 Container / 3 ContainerProcess / 4 Component
    pub kind: u32,
    /// 0
    pub reserved: u32,
    /// "juiz_rust_abi=1;" (§9.3)
    pub abi_marker: *const c_char,
    /// manifest の JSON を out に書く。全種類
    pub manifest: Option<JuizRustManifestFn>,
    /// プラグインの情報の JSON を out に書く (§3.3)。全種類
    pub info: Option<JuizRustManifestFn>,
    /// kind 1: 呼び出し
    pub call: Option<JuizRustCallFn>,
    /// kind 2: コンテナの作成
    pub create: Option<JuizRustCreateFn>,
    /// kind 2: コンテナの破棄
    pub destroy: Option<JuizRustDestroyFn>,
    /// kind 3: コンテナプロセスの呼び出し
    pub container_call: Option<JuizRustContainerCallFn>,
}

// SAFETY: 表は static に置く読むだけのもので、abi_marker は静的な C 文字列を指す
unsafe impl Sync for JuizRustEntry {}
unsafe impl Send for JuizRustEntry {}

impl JuizRustEntry {
    const fn base(kind: u32, manifest: JuizRustManifestFn, info: JuizRustManifestFn) -> Self {
        JuizRustEntry {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<JuizRustEntry>() as u32,
            kind,
            reserved: 0,
            abi_marker: ABI_MARKER_C.as_ptr() as *const c_char,
            manifest: Some(manifest),
            info: Some(info),
            call: None,
            create: None,
            destroy: None,
            container_call: None,
        }
    }

    /// Process (kind 1) の表
    pub const fn process(
        manifest: JuizRustManifestFn,
        info: JuizRustManifestFn,
        call: JuizRustCallFn,
    ) -> Self {
        let mut e = Self::base(KIND_PROCESS, manifest, info);
        e.call = Some(call);
        e
    }

    /// Container (kind 2) の表
    pub const fn container(
        manifest: JuizRustManifestFn,
        info: JuizRustManifestFn,
        create: JuizRustCreateFn,
        destroy: JuizRustDestroyFn,
    ) -> Self {
        let mut e = Self::base(KIND_CONTAINER, manifest, info);
        e.create = Some(create);
        e.destroy = Some(destroy);
        e
    }

    /// ContainerProcess (kind 3) の表
    pub const fn container_process(
        manifest: JuizRustManifestFn,
        info: JuizRustManifestFn,
        container_call: JuizRustContainerCallFn,
    ) -> Self {
        let mut e = Self::base(KIND_CONTAINER_PROCESS, manifest, info);
        e.container_call = Some(container_call);
        e
    }

    /// Component (kind 4) の表
    pub const fn component(manifest: JuizRustManifestFn, info: JuizRustManifestFn) -> Self {
        Self::base(KIND_COMPONENT, manifest, info)
    }
}

// ---- グルー (§7) ----

/// 入口の中で起きた失敗。エラーコードと文になる
#[derive(Debug)]
enum Failure {
    Call(String),
    Frame(String),
    HostRefused,
    Null,
}

impl Failure {
    fn code_and_message(self) -> (i64, Option<String>) {
        match self {
            Failure::Call(m) => (JUIZ_RUST_CALL_ERROR, Some(m)),
            Failure::Frame(m) => (JUIZ_RUST_FRAME_ERROR, Some(m)),
            Failure::HostRefused => (JUIZ_RUST_HOST_REFUSED, None),
            Failure::Null => (JUIZ_RUST_NULL_ARGUMENT, None),
        }
    }
}

fn call_failure(e: anyhow::Error) -> Failure {
    Failure::Call(format!("{e:#}"))
}

/// panic の payload を文にする (§6.1)
fn panic_message(payload: &(dyn Any + Send)) -> String {
    let m = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    };
    format!("panicked: {m}")
}

/// 入口の中身を catch_unwind で囲み、失敗をコードにする。文があれば report に渡す
fn guarded(report: impl FnOnce(&str), body: impl FnOnce() -> Result<(), Failure>) -> i64 {
    let (code, message) = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => return JUIZ_OK,
        Ok(Err(f)) => f.code_and_message(),
        Err(payload) => {
            let m = panic_message(payload.as_ref());
            // payload の drop が panic しても外へ出さない
            let _ = catch_unwind(AssertUnwindSafe(move || drop(payload)));
            (JUIZ_RUST_PANIC, Some(m))
        }
    };
    if let Some(m) = message {
        report(&m);
    }
    code
}

/// sink に 1 回書く。sink か write が null なら false
fn write_sink(sink: *const JuizBytesSink, bytes: &[u8]) -> bool {
    if sink.is_null() {
        return false;
    }
    // SAFETY: ホストは sink を呼び出しの間有効に保つ
    let sink = unsafe { &*sink };
    match sink.write {
        Some(write) => {
            write(sink.ctx, JuizBytes::from_slice(bytes));
            true
        }
        None => false,
    }
}

fn report_to_frame_out(out: &JuizFrameOut, message: &str) {
    if let Some(error) = out.error {
        error(out.ctx, JuizBytes::from_slice(message.as_bytes()));
    }
}

/// 引数の Native を受け取る (§10.3)。owned = 1 のものはここで持ち主になる。
/// このプラグインの表 (NATIVE_VTABLE) で作ったものでなければエラー。そのとき owned = 1 のものは、
/// その表の destroy で解放する
fn take_natives(args: &JuizFrameIn) -> Result<Vec<Capsule>, Failure> {
    if args.native_count == 0 {
        return Ok(Vec::new());
    }
    if args.natives.is_null() {
        return Err(Failure::Null);
    }
    // SAFETY: ホストは natives を呼び出しの間有効に保つ
    let natives = unsafe { std::slice::from_raw_parts(args.natives, args.native_count) };
    let mut out = Vec::with_capacity(natives.len());
    let mut failure = None;
    for (i, n) in natives.iter().enumerate() {
        let ours = std::ptr::eq(n.vtable, &NATIVE_VTABLE);
        if !ours || n.handle.is_null() {
            if n.owned != 0 && !n.handle.is_null() && !n.vtable.is_null() {
                // SAFETY: owned = 1 なので持ち主はこちら。作った側の表で解放する
                if let Some(destroy) = unsafe { (*n.vtable).destroy } {
                    destroy(n.handle);
                }
            }
            failure.get_or_insert_with(|| {
                Failure::Frame(format!(
                    "native {i} was not made by this plugin (the host must encode it before passing it)"
                ))
            });
            continue;
        }
        let cell = if n.owned != 0 {
            // SAFETY: handle はこのプラグインの native_into_out が Box::into_raw した NativeCell で、
            // owned = 1 なので持ち主がこちらに移った
            *unsafe { Box::from_raw(n.handle as *mut NativeCell) }
        } else {
            // SAFETY: ホストが呼び出しの間持ち続ける。Arc を複製して使い、呼び出しの後は handle に触らない
            unsafe { &*(n.handle as *const NativeCell) }.clone()
        };
        out.push(Capsule::from(cell));
    }
    match failure {
        Some(f) => Err(f),
        None => Ok(out),
    }
}

/// 引数のフレームを CapsuleMap にする (§5.2)。画像のコピーはここで 1 回。
/// Native (§10.3) は複製も encode もせずに受け取る
fn read_args(args: *const JuizFrameIn) -> Result<CapsuleMap, Failure> {
    if args.is_null() {
        return Err(Failure::Null);
    }
    // SAFETY: ホストは args とその指す先を呼び出しの間有効に保つ
    let args = unsafe { &*args };
    // owned = 1 の Native の持ち主になるのを先にする (後で失敗しても解放されるように)
    let natives = take_natives(args)?;
    let header = unsafe { args.header.as_slice() }.ok_or(Failure::Null)?;
    let descs: &[JuizBytes] = if args.body_count == 0 {
        &[]
    } else if args.bodies.is_null() {
        return Err(Failure::Null);
    } else {
        unsafe { std::slice::from_raw_parts(args.bodies, args.body_count) }
    };
    let mut bodies = Vec::with_capacity(descs.len());
    for b in descs.iter() {
        bodies.push(unsafe { b.as_slice() }.ok_or(Failure::Null)?);
    }
    read_frame_parts(header, &bodies, &FrameLimits::default())
        .and_then(|mut f| {
            f.set_natives(natives);
            f.into_call_map()
        })
        .map_err(|e| Failure::Frame(format!("{e:#}")))
}

// ---- Native (§10) ----

/// このプラグインの Native の表。値と一緒に運ぶ (§10.1)。
/// juiz_sdk はプラグインごとに静的にリンクされるので、アドレスはプラグインごとに違う
static NATIVE_VTABLE: JuizRustNativeVtable = JuizRustNativeVtable {
    abi_version: ABI_VERSION,
    struct_size: std::mem::size_of::<JuizRustNativeVtable>() as u32,
    encode: Some(native_encode),
    destroy: Some(native_destroy),
};

/// NativeKind を表の数にする (0 Value / 1 Image)
pub fn native_kind_code(kind: NativeKind) -> u32 {
    match kind {
        NativeKind::Value => 0,
        NativeKind::Image => 1,
    }
}

/// Native の型タグの JSON (§10.1)。型タグが無ければ name (表示用の型名) だけ
fn native_type_tag_json(cell: &NativeCell) -> Vec<u8> {
    let v = match cell.value().type_tag() {
        Some(t) => serde_json::json!({"name": t.name, "fingerprint": t.fingerprint, "abi": t.abi}),
        None => serde_json::json!({"name": cell.value().type_name()}),
    };
    serde_json::to_vec(&v).unwrap_or_default()
}

/// 表の encode。共通形式にして out に書く (ヘッダは kind: response で、result がその値)
extern "C" fn native_encode(handle: *const c_void, out: *const JuizFrameOut) -> i64 {
    if out.is_null() {
        return JUIZ_RUST_NULL_ARGUMENT;
    }
    // SAFETY: ホストは out を呼び出しの間有効に保つ
    let out = unsafe { &*out };
    guarded(
        |m| report_to_frame_out(out, m),
        || {
            if handle.is_null() {
                return Err(Failure::Null);
            }
            // SAFETY: handle はこのプラグインの NativeCell で、ホストが持っている間は有効
            let cell = unsafe { &*(handle as *const NativeCell) };
            // encode 済みならその結果を使う (複製しない)。まだならキャッシュせずに encode する
            // (ホストの側が結果を覚えるので、プラグインの側でも覚えると二重になる)
            let capsule = if cell.is_encoded() {
                Capsule::from(cell.clone())
            } else {
                match cell.value().encode().map_err(|e| {
                    call_failure(anyhow::anyhow!(JuizError::NativeEncodeError {
                        native: cell.describe(),
                        message: format!("{e:#}"),
                    }))
                })? {
                    EncodedValue::Value(v) => Capsule::from(v),
                    EncodedValue::Image(i) => Capsule::from(i),
                }
            };
            write_frame(out, capsule)
        },
    )
}

/// 表の destroy。panic はログに出して握りつぶす
extern "C" fn native_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let r = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: handle は native_into_out が Box::into_raw したもので、1 回だけ渡される
        drop(unsafe { Box::from_raw(handle as *mut NativeCell) });
    }));
    if let Err(payload) = r {
        let m = panic_message(payload.as_ref());
        let _ = catch_unwind(AssertUnwindSafe(move || drop(payload)));
        let _ = catch_unwind(|| log::error!("destroying a native value {m}"));
    }
}

/// 戻り値の Native をハンドルのままホストへ渡す (§10.2)。
/// ホストが断ったら (begin / native が負)、ここで解放して HostRefused
fn native_into_out(
    out: &JuizFrameOut,
    native: extern "C" fn(ctx: *mut c_void, index: usize, native: JuizRustNative) -> i64,
    cell: NativeCell,
    options: &std::collections::HashMap<String, String>,
) -> Result<(), Failure> {
    let begin = out.begin.ok_or(Failure::Null)?;
    let mut header = crate::frame::FrameHeader::new(0, crate::frame::FrameKind::Response);
    header.result = Some(serde_json::json!({"native": 0}));
    if !options.is_empty() {
        header.option = Some(options.clone());
    }
    let header = serde_json::to_vec(&header)
        .map_err(|e| Failure::Call(format!("can not write the frame header ({e})")))?;
    if begin(out.ctx, JuizBytes::from_slice(&header)) < 0 {
        return Err(Failure::HostRefused);
    }
    let tag = native_type_tag_json(&cell);
    let kind = native_kind_code(cell.kind());
    let handle = Box::into_raw(Box::new(cell)) as *mut c_void;
    let n = JuizRustNative {
        handle,
        vtable: &NATIVE_VTABLE,
        type_tag: JuizBytes::from_slice(&tag),
        kind,
        owned: 1,
    };
    if native(out.ctx, 0, n) < 0 {
        // 受け取られなかったので、持ち主はまだこちら
        native_destroy(handle);
        return Err(Failure::HostRefused);
    }
    Ok(())
}

/// 戻り値を書き込み口に書く (§5.3)。Native は、ホストが native の口を持てばハンドルのまま渡し (§10.2)、
/// 持たなければ encode して書く
fn write_result(out: &JuizFrameOut, result: Capsule) -> Result<(), Failure> {
    if let Some(native) = out.native {
        if result.is_native() {
            let options = result.get_options().clone();
            if let Ok(cell) = result.into_native_cell() {
                return native_into_out(out, native, cell, &options);
            }
            unreachable!("is_native() was true");
        }
    }
    write_frame(out, result)
}

/// 戻り値をフレームで書く (§5.3)。画像のコピーはここで 1 回。Native は encode して書く
fn write_frame(out: &JuizFrameOut, result: Capsule) -> Result<(), Failure> {
    let begin = out.begin.ok_or(Failure::Null)?;
    let ptr: CapsulePtr = result.into();
    let frame = OutgoingFrame::response(0, &ptr).map_err(call_failure)?;
    let header = frame.header_bytes().map_err(call_failure)?;
    if begin(out.ctx, JuizBytes::from_slice(&header)) < 0 {
        return Err(Failure::HostRefused);
    }
    if frame.body_count() == 0 {
        return Ok(());
    }
    let body = out.body.ok_or(Failure::Null)?;
    for i in 0..frame.body_count() {
        let expected = frame.body_len(i).unwrap_or_default();
        let mut dst: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        if body(out.ctx, i, &mut dst, &mut len) < 0 {
            return Err(Failure::HostRefused);
        }
        if len as u64 != expected {
            return Err(Failure::Frame(format!(
                "the host gave {len} bytes for body {i}, but the body is {expected} bytes"
            )));
        }
        if len == 0 {
            continue;
        }
        if dst.is_null() {
            return Err(Failure::Null);
        }
        // SAFETY: ホストは len バイトの書ける領域を渡し、呼び出しの間有効に保つ
        let dst = unsafe { std::slice::from_raw_parts_mut(dst, len) };
        frame.copy_body_into(i, dst).map_err(call_failure)?;
    }
    Ok(())
}

/// manifest の JSON を out に書く (§5.4)。
/// manifest を作る関数の panic は JUIZ_RUST_PANIC にし、そのときは out に文を書く
pub fn write_manifest<M: Serialize>(out: *const JuizBytesSink, manifest: impl FnOnce() -> M) -> i64 {
    if out.is_null() {
        return JUIZ_RUST_NULL_ARGUMENT;
    }
    guarded(
        |m| {
            write_sink(out, m.as_bytes());
        },
        || {
            let _ = env_logger::try_init();
            let m = manifest();
            let json = serde_json::to_vec(&m)
                .map_err(|e| Failure::Call(format!("can not write the manifest as JSON ({e})")))?;
            // write が無ければ何も書いていないので、エラーの文も書けない (Null は文を持たない)
            if write_sink(out, &json) {
                Ok(())
            } else {
                Err(Failure::Null)
            }
        },
    )
}

/// コンテナの型タグ (§8.2)。
/// `rust:<rustc -V>:<juiz_sdk の版>:<ContainerImpl<T> の型名>:<size_of>:<align_of>`
pub fn container_tag<T: 'static>() -> String {
    format!(
        "rust:{}:{}:{}:{}:{}",
        env!("JUIZ_SDK_RUSTC_VERSION"),
        env!("CARGO_PKG_VERSION"),
        std::any::type_name::<ContainerImpl<T>>(),
        std::mem::size_of::<ContainerImpl<T>>(),
        std::mem::align_of::<ContainerImpl<T>>(),
    )
}

/// info の JSON (§3.3)
fn info_json(kind: u32, container_tag: Option<String>) -> serde_json::Value {
    let mut info = serde_json::Map::new();
    let build = crate::build_info::BuildInfo::current();
    info.insert(
        "build_info".to_owned(),
        format!("juiz_sdk={};rustc={}", build.juiz_sdk_str(), build.rustc_str()).into(),
    );
    if kind != KIND_COMPONENT {
        info.insert(
            "native_abi_tag".to_owned(),
            crate::types::rust_abi_tag().into(),
        );
    }
    if let Some(tag) = container_tag {
        info.insert("container_tag".to_owned(), tag.into());
    }
    serde_json::Value::Object(info)
}

fn write_info(out: *const JuizBytesSink, kind: u32, tag: impl FnOnce() -> Option<String>) -> i64 {
    write_manifest(out, || info_json(kind, tag()))
}

/// Process (kind 1) の info を書く
pub fn process_info(out: *const JuizBytesSink) -> i64 {
    write_info(out, KIND_PROCESS, || None)
}

/// Component (kind 4) の info を書く
pub fn component_info(out: *const JuizBytesSink) -> i64 {
    write_info(out, KIND_COMPONENT, || None)
}

/// Container (kind 2) の info を書く。型タグは作成関数の型から決める
pub fn container_info<T: 'static, F>(out: *const JuizBytesSink, _create: F) -> i64
where
    F: Fn(CapsuleMap) -> JuizResult<Box<T>>,
{
    write_info(out, KIND_CONTAINER, || Some(container_tag::<T>()))
}

/// ContainerProcess (kind 3) の info を書く。型タグは受け取るコンテナの型から決める
pub fn container_process_info<T: 'static, F>(out: *const JuizBytesSink, _function: F) -> i64
where
    F: Fn(&mut ContainerImpl<T>, CapsuleMap) -> JuizResult<Capsule>,
{
    write_info(out, KIND_CONTAINER_PROCESS, || Some(container_tag::<T>()))
}

/// Process を呼ぶ (§5.2・§5.3)。引数のフレームから CapsuleMap を作って function を呼び、戻り値を out に書く
pub fn call_process<F>(args: *const JuizFrameIn, out: *const JuizFrameOut, function: F) -> i64
where
    F: Fn(CapsuleMap) -> JuizResult<Capsule>,
{
    if out.is_null() {
        // owned = 1 の Native の持ち主になってから返す (§10.3)
        let _ = guarded(|_| {}, || read_args(args).map(drop));
        return JUIZ_RUST_NULL_ARGUMENT;
    }
    // SAFETY: ホストは out を呼び出しの間有効に保つ
    let out = unsafe { &*out };
    guarded(
        |m| report_to_frame_out(out, m),
        || {
            let map = read_args(args)?;
            let result = function(map).map_err(call_failure)?;
            write_result(out, result)
        },
    )
}

/// コンテナを作る (§8.1)。プラグインの中で ContainerImpl<T> を作り、そのポインタを *handle に入れる
pub fn create_container<T: 'static, F>(
    manifest_json: JuizBytes,
    args: *const JuizFrameIn,
    handle: *mut *mut c_void,
    error: *const JuizBytesSink,
    create: F,
) -> i64
where
    F: Fn(CapsuleMap) -> JuizResult<Box<T>>,
{
    guarded(
        |m| {
            write_sink(error, m.as_bytes());
        },
        || {
            // owned = 1 の Native の持ち主になるのを先にする (§10.3)
            let map = read_args(args)?;
            if handle.is_null() {
                return Err(Failure::Null);
            }
            let json = unsafe { manifest_json.as_slice() }.ok_or(Failure::Null)?;
            let manifest: ContainerManifest = serde_json::from_slice(json).map_err(|e| {
                Failure::Frame(format!("can not read the container manifest ({e})"))
            })?;
            let t = create(map).map_err(call_failure)?;
            let container = ContainerImpl::new(manifest, t).map_err(call_failure)?;
            let raw = Box::into_raw(Box::new(container)) as *mut c_void;
            // SAFETY: handle は null でないことを確かめた
            unsafe { *handle = raw };
            Ok(())
        },
    )
}

/// コンテナを破棄する (§8.1)。panic はログに出して握りつぶす
///
/// handle は同じ T の create_container が作ったもので、1 回だけ渡す
pub fn destroy_container<T: 'static, F>(handle: *mut c_void, _create: F)
where
    F: Fn(CapsuleMap) -> JuizResult<Box<T>>,
{
    if handle.is_null() {
        return;
    }
    let r = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: handle は create_container が Box::into_raw したもの
        drop(unsafe { Box::from_raw(handle as *mut ContainerImpl<T>) });
    }));
    if let Err(payload) = r {
        let m = panic_message(payload.as_ref());
        let _ = catch_unwind(AssertUnwindSafe(move || drop(payload)));
        let _ = catch_unwind(|| log::error!("destroying a container ({}) {m}", std::any::type_name::<T>()));
    }
}

/// コンテナプロセスを呼ぶ (§8.3)。handle を ContainerImpl<T> に戻して function を呼ぶ
///
/// handle の型が合っているか (型タグ) はホストが呼ぶ前に確かめる
pub fn call_container_process<T: 'static, F>(
    handle: *mut c_void,
    args: *const JuizFrameIn,
    out: *const JuizFrameOut,
    function: F,
) -> i64
where
    F: Fn(&mut ContainerImpl<T>, CapsuleMap) -> JuizResult<Capsule>,
{
    if out.is_null() {
        // owned = 1 の Native の持ち主になってから返す (§10.3)
        let _ = guarded(|_| {}, || read_args(args).map(drop));
        return JUIZ_RUST_NULL_ARGUMENT;
    }
    // SAFETY: ホストは out を呼び出しの間有効に保つ
    let out = unsafe { &*out };
    guarded(
        |m| report_to_frame_out(out, m),
        || {
            // owned = 1 の Native の持ち主になるのを先にする (§10.3)
            let map = read_args(args)?;
            if handle.is_null() {
                return Err(Failure::Null);
            }
            // SAFETY: handle は同じ型タグの create_container が作ったもので、ホストはロックを持って呼ぶ
            let container = unsafe { &mut *(handle as *mut ContainerImpl<T>) };
            let result = function(container, map).map_err(call_failure)?;
            write_result(out, result)
        },
    )
}

#[cfg(test)]
mod tests;
