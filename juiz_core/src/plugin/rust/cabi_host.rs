//! C ABI の Rust プラグインを読むホスト側 (docs/spec/rust-plugin-abi.md、issue #330)
//!
//! juiz_macro が生成した `<symbol>_cabi()` の表 (§3.2) を読み、表の関数を JZF1 フレーム (§5) で呼ぶ。
//! ホストとプラグインの間で渡すのは `#[repr(C)]` の構造体と `extern "C"` の関数だけなので、
//! プラグインはこの juiz と違う rustc・違う juiz_sdk でビルドされていてよい。
//!
//! - 引数: `CapsuleMap` からヘッダを作り、画像はカプセルをロックしたままその画素を直接指す (複製しない、§5.2)
//! - 戻り値: プラグインが渡すヘッダを読み、本体ごとに最終的な領域を確保して、そこへ直接書かせる (§5.3)
//! - エラー: 負のコードと文は `RustPluginCallError`、表の版が読めなければ `RustPluginAbiVersionError` (§6.3)
//! - コンテナ: プラグインの中の `ContainerImpl<T>` を不透明なポインタ (handle) で持ち、型タグで照合する (§8)
//! - Native: プラグインが返す Native はプラグインの中に置いたまま、ハンドルと表を [`RustCabiNative`] で持つ。
//!   同じプラグイン (`native_abi_tag` が同じ) へはハンドルのまま渡し、ほかへは encode する (§10)

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::cabi::{
    error_code_name, JuizBytes, JuizBytesSink, JuizFrameIn, JuizFrameOut, JuizRustCallFn,
    JuizRustContainerCallFn, JuizRustCreateFn, JuizRustDestroyFn, JuizRustEntry,
    JuizRustManifestFn, JuizRustNative, JuizRustNativeVtable, CABI_SYMBOL_SUFFIX,
    JUIZ_RUST_HOST_REFUSED, KIND_COMPONENT, KIND_CONTAINER, KIND_CONTAINER_PROCESS, KIND_PROCESS,
};
use juiz_sdk::frame::{FrameLimits, FrameReceiver, OutgoingFrame};
use libloading::Library;

use crate::prelude::*;

/// このホストが読める入口の表の ABI の版 (§9.2)
pub const SUPPORTED_ABI_VERSIONS: &[u32] = &[1];

fn supported_abi_str() -> String {
    SUPPORTED_ABI_VERSIONS
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn kind_name(kind: u32) -> &'static str {
    match kind {
        KIND_PROCESS => "Process",
        KIND_CONTAINER => "Container",
        KIND_CONTAINER_PROCESS => "ContainerProcess",
        KIND_COMPONENT => "Component",
        _ => "unknown",
    }
}

/// 読み込んだ入口の表。関数ポインタを写して持ち、ライブラリを外さないよう `Arc<Library>` を持つ
pub struct CabiEntry {
    lib: Arc<Library>,
    /// プラグインのパス (エラーの文に使う)
    plugin: String,
    /// `<symbol>_cabi`
    symbol: String,
    manifest: JuizRustManifestFn,
    info: JuizRustManifestFn,
    call: Option<JuizRustCallFn>,
    create: Option<JuizRustCreateFn>,
    destroy: Option<JuizRustDestroyFn>,
    container_call: Option<JuizRustContainerCallFn>,
    /// info の native_abi_tag (§3.3)。Native をハンドルのまま渡してよいかに使う (§10.3)
    native_abi_tag: Option<Arc<str>>,
}

impl std::fmt::Debug for CabiEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CabiEntry")
            .field("plugin", &self.plugin)
            .field("symbol", &self.symbol)
            .finish()
    }
}

/// `RustPluginCallError` を作る。message が無ければ `returned <コード> (<名前>)`
fn call_error(plugin: &str, symbol: &str, func: &str, code: i64, message: Option<String>) -> anyhow::Error {
    let message = message.unwrap_or_else(|| format!("returned {code} ({})", error_code_name(code)));
    let symbol = format!("{symbol}.{func}");
    log::error!("【失敗】Rust plugin ({plugin}) '{symbol}': {message}");
    anyhow!(JuizError::RustPluginCallError {
        plugin: plugin.to_owned(),
        symbol,
        message,
    })
}

/// 表の版を確かめる (§9.2)
fn check_abi_version(plugin: &str, symbol: &str, plugin_abi: u32) -> JuizResult<()> {
    if SUPPORTED_ABI_VERSIONS.contains(&plugin_abi) {
        return Ok(());
    }
    let newest = SUPPORTED_ABI_VERSIONS.iter().copied().max().unwrap_or(0);
    let hint = if plugin_abi > newest {
        "Update juiz."
    } else {
        "Rebuild the plugin with a newer juiz_sdk."
    };
    log::error!("【失敗】Rust plugin ({plugin}) '{symbol}' uses Rust plugin ABI {plugin_abi}");
    Err(anyhow!(JuizError::RustPluginAbiVersionError {
        plugin: plugin.to_owned(),
        symbol: symbol.to_owned(),
        plugin_abi,
        host_abi: supported_abi_str(),
        hint: hint.to_owned(),
    }))
}

impl CabiEntry {
    /// `<symbol>_cabi` を探して表を読む。シンボルが無ければ None (古い Rust ABI のプラグイン、§11)。
    /// 表の版が読めない、種類が違う、要る関数が null ならエラー
    pub fn find(
        lib: &Arc<Library>,
        plugin: &str,
        symbol: &str,
        expected_kind: u32,
    ) -> JuizResult<Option<CabiEntry>> {
        let name = format!("{symbol}{CABI_SYMBOL_SUFFIX}");
        type EntryFn = unsafe extern "C" fn() -> *const JuizRustEntry;
        let f = match unsafe { lib.get::<EntryFn>(name.as_bytes()) } {
            Ok(f) => *f,
            Err(_) => return Ok(None),
        };
        let p = unsafe { f() };
        if p.is_null() {
            return Err(call_error(plugin, &name, "()", 0, Some("returned null".to_owned())));
        }
        // 版を最初に読み、知らない版なら残りを読まない (§3.2)。表全体の参照はまだ作らない
        let abi_version = unsafe { std::ptr::addr_of!((*p).abi_version).read() };
        check_abi_version(plugin, &name, abi_version)?;
        let struct_size = unsafe { std::ptr::addr_of!((*p).struct_size).read() } as usize;
        let known = std::mem::size_of::<JuizRustEntry>();
        if struct_size < known {
            return Err(call_error(
                plugin,
                &name,
                "()",
                0,
                Some(format!(
                    "the entry table is {struct_size} bytes, smaller than {known} bytes of Rust plugin ABI {abi_version}"
                )),
            ));
        }
        // SAFETY: 版と大きさを確かめた。表はプラグインを外すまで有効 (§3.1)
        let e = unsafe { &*p };
        if e.kind != expected_kind {
            return Err(call_error(
                plugin,
                &name,
                "()",
                0,
                Some(format!(
                    "the entry is a {} (kind {}), but a {} (kind {expected_kind}) is expected",
                    kind_name(e.kind),
                    e.kind,
                    kind_name(expected_kind)
                )),
            ));
        }
        let missing = |what: &str| {
            call_error(
                plugin,
                &name,
                what,
                0,
                Some(format!("the entry of a {} has no '{what}'", kind_name(expected_kind))),
            )
        };
        let manifest = e.manifest.ok_or_else(|| missing("manifest"))?;
        let info = e.info.ok_or_else(|| missing("info"))?;
        match expected_kind {
            KIND_PROCESS if e.call.is_none() => return Err(missing("call")),
            KIND_CONTAINER if e.create.is_none() => return Err(missing("create")),
            KIND_CONTAINER if e.destroy.is_none() => return Err(missing("destroy")),
            KIND_CONTAINER_PROCESS if e.container_call.is_none() => {
                return Err(missing("container_call"))
            }
            _ => {}
        }
        let mut entry = CabiEntry {
            lib: lib.clone(),
            plugin: plugin.to_owned(),
            symbol: name,
            manifest,
            info,
            call: e.call,
            create: e.create,
            destroy: e.destroy,
            container_call: e.container_call,
            native_abi_tag: None,
        };
        let info = entry.info_json()?;
        entry.native_abi_tag = info
            .get("native_abi_tag")
            .and_then(|v| v.as_str())
            .map(Arc::from);
        log::debug!(
            "Rust plugin ({plugin}) '{}' is loaded through the C ABI {abi_version} (build_info: {})",
            entry.symbol,
            info.get("build_info").and_then(|v| v.as_str()).unwrap_or("unknown")
        );
        Ok(Some(entry))
    }

    fn error(&self, func: &str, code: i64, message: Option<String>) -> anyhow::Error {
        call_error(&self.plugin, &self.symbol, func, code, message)
    }

    /// manifest / info の関数を呼び、書かれた JSON を読む。負のコードなら書かれたものはエラー文 (§5.4)
    fn read_json(&self, func: &str, f: JuizRustManifestFn) -> JuizResult<Value> {
        let (code, bytes) = collect_bytes(|out| f(out));
        if code < 0 {
            let message = bytes.map(|b| String::from_utf8_lossy(&b).into_owned());
            return Err(self.error(func, code, message));
        }
        let bytes = bytes.ok_or_else(|| self.error(func, code, Some("wrote nothing".to_owned())))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| self.error(func, code, Some(format!("wrote invalid JSON ({e})"))))
    }

    /// manifest の JSON
    pub fn manifest_json(&self) -> JuizResult<Value> {
        self.read_json("manifest", self.manifest)
    }

    /// info の JSON (§3.3)
    pub fn info_json(&self) -> JuizResult<Value> {
        self.read_json("info", self.info)
    }

    /// info の container_tag (§8.2)。kind 2・3 で必須
    fn container_tag(&self) -> JuizResult<String> {
        self.info_json()?
            .get("container_tag")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| self.error("info", 0, Some("has no 'container_tag'".to_owned())))
    }

    /// manifest を読んで T にする
    fn manifest_as<T: serde::de::DeserializeOwned>(&self) -> JuizResult<T> {
        let v = self.manifest_json()?;
        serde_json::from_value(v)
            .map_err(|e| self.error("manifest", 0, Some(format!("can not read the manifest ({e})"))))
    }

    /// 書き込み口を用意して f を呼び、戻り値を Capsule にする (§5.3)
    fn call_with_out(
        &self,
        func: &str,
        f: impl FnOnce(*const JuizFrameOut) -> i64,
    ) -> JuizResult<Capsule> {
        let mut out = HostOut {
            native_origin: self.native_origin(),
            ..Default::default()
        };
        let frame_out = out.frame_out();
        let code = f(&frame_out);
        if code < 0 {
            let message = out.error.take().or_else(|| {
                if code == JUIZ_RUST_HOST_REFUSED {
                    out.refused
                        .take()
                        .map(|m| format!("the host refused the result ({m})"))
                } else {
                    None
                }
            });
            return Err(self.error(func, code, message));
        }
        out.finish()
            .map_err(|e| self.error(func, code, Some(format!("the result can not be read ({e:#})"))))
    }

    /// このプラグインが返す Native に付ける出どころ (§10.2)。native_abi_tag が無ければ Native を受け取らない
    fn native_origin(&self) -> Option<NativeOrigin> {
        self.native_abi_tag.as_ref().map(|abi| NativeOrigin {
            _lib: self.lib.clone(),
            plugin: self.plugin.clone(),
            abi: abi.clone(),
        })
    }

    /// Process (kind 1) を呼ぶ
    fn call_process(&self, args: CapsuleMap) -> JuizResult<Capsule> {
        let call = self.call.ok_or_else(|| self.error("call", 0, Some("is null".to_owned())))?;
        with_frame_in(self.native_abi_tag.as_deref(), args, |frame_in| {
            self.call_with_out("call", |out| call(frame_in, out))
        })
        .map_err(|e| self.error("call", 0, Some(format!("the arguments can not be passed ({e:#})"))))?
    }
}

// ---- Native (§10) ----

/// RustCabiNative の encode (表の encode) を呼んだ回数。試験で「境界で encode しなかった」ことを確かめる
static NATIVE_ENCODES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// C ABI のプラグインの Native を encode した回数 (プロセス全体、試験用)
#[doc(hidden)]
pub fn rust_cabi_native_encode_count() -> u64 {
    NATIVE_ENCODES.load(std::sync::atomic::Ordering::SeqCst)
}

/// Native を作ったプラグイン
#[derive(Clone)]
struct NativeOrigin {
    /// destroy を呼ぶまでプラグインを外さない
    _lib: Arc<Library>,
    plugin: String,
    /// そのプラグインの native_abi_tag (§3.3)
    abi: Arc<str>,
}

/// プラグインの中に置いたままの Native (§10.2)。ホストは中身を見ず、要るときに表の encode を呼ぶ。
/// Drop で表の destroy を呼ぶ
pub struct RustCabiNative {
    handle: *mut c_void,
    encode: extern "C" fn(handle: *const c_void, out: *const JuizFrameOut) -> i64,
    destroy: extern "C" fn(handle: *mut c_void),
    vtable: *const JuizRustNativeVtable,
    /// 受け取った型タグの JSON (引数で渡すときにそのまま返す)
    type_tag_json: Vec<u8>,
    type_name: String,
    tag: Option<NativeTypeTag>,
    kind: NativeKind,
    origin: NativeOrigin,
}

// SAFETY: handle はプラグインの中の NativeCell (Send + Sync) を指し、表の関数はどのスレッドから呼んでもよい
unsafe impl Send for RustCabiNative {}
unsafe impl Sync for RustCabiNative {}

impl std::fmt::Debug for RustCabiNative {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("RustCabiNative({} from {})", self.type_name, self.origin.plugin))
    }
}

impl RustCabiNative {
    /// プラグインが渡したものを包む。表が読めなければエラーの文 (そのときは包まず、解放もしない)
    fn new(native: &JuizRustNative, origin: NativeOrigin) -> Result<Self, String> {
        if native.handle.is_null() || native.vtable.is_null() {
            return Err("the native value has a null handle or table".to_owned());
        }
        // SAFETY: 表はプラグインの中の static で、プラグインを外すまで有効。版と大きさを先に読む
        let vt = native.vtable;
        let abi_version = unsafe { std::ptr::addr_of!((*vt).abi_version).read() };
        if !SUPPORTED_ABI_VERSIONS.contains(&abi_version) {
            return Err(format!("the native table uses Rust plugin ABI {abi_version}"));
        }
        let struct_size = unsafe { std::ptr::addr_of!((*vt).struct_size).read() } as usize;
        if struct_size < std::mem::size_of::<JuizRustNativeVtable>() {
            return Err(format!("the native table is too small ({struct_size} bytes)"));
        }
        let vt = unsafe { &*vt };
        let (Some(encode), Some(destroy)) = (vt.encode, vt.destroy) else {
            return Err("the native table has no 'encode' or 'destroy'".to_owned());
        };
        let kind = match native.kind {
            0 => NativeKind::Value,
            1 => NativeKind::Image,
            k => return Err(format!("unknown native kind {k}")),
        };
        let type_tag_json = unsafe { native.type_tag.as_slice() }
            .ok_or_else(|| "the type tag is null".to_owned())?
            .to_vec();
        let tag_value: Value = if type_tag_json.is_empty() {
            jvalue!({})
        } else {
            serde_json::from_slice(&type_tag_json)
                .map_err(|e| format!("the type tag is not JSON ({e})"))?
        };
        let field = |k: &str| tag_value.get(k).and_then(|v| v.as_str()).map(|s| s.to_owned());
        let tag = match (field("name"), field("fingerprint"), field("abi")) {
            (Some(name), Some(fingerprint), Some(abi)) => Some(NativeTypeTag {
                name,
                fingerprint,
                abi,
            }),
            _ => None,
        };
        Ok(RustCabiNative {
            handle: native.handle,
            encode,
            destroy,
            vtable: native.vtable,
            type_tag_json,
            type_name: field("name").unwrap_or_else(|| "unknown".to_owned()),
            tag,
            kind,
            origin,
        })
    }

    /// 作ったプラグインの native_abi_tag (§10.3)
    pub fn native_abi_tag(&self) -> &str {
        &self.origin.abi
    }

    /// 引数として渡す形にする。owned なら持ち主を渡すので、自分は destroy を呼ばずに消える
    fn to_arg(&self, owned: bool) -> JuizRustNative {
        JuizRustNative {
            handle: self.handle,
            vtable: self.vtable,
            type_tag: JuizBytes::from_slice(&self.type_tag_json),
            kind: match self.kind {
                NativeKind::Value => 0,
                NativeKind::Image => 1,
            },
            owned: owned as u32,
        }
    }
}

impl Drop for RustCabiNative {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            (self.destroy)(self.handle);
            self.handle = std::ptr::null_mut();
        }
    }
}

impl NativeValue for RustCabiNative {
    fn language(&self) -> &str {
        "rust"
    }
    fn kind(&self) -> NativeKind {
        self.kind
    }
    fn type_name(&self) -> String {
        self.type_name.clone()
    }
    fn encode(&self) -> JuizResult<EncodedValue> {
        NATIVE_ENCODES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // encode の結果に Native は来ない (native の口を渡さない)
        let mut out = HostOut::default();
        let frame_out = out.frame_out();
        let code = (self.encode)(self.handle, &frame_out);
        let error = |message: Option<String>| {
            call_error(&self.origin.plugin, "native", "encode", code, message)
        };
        if code < 0 {
            let message = out.error.take().or_else(|| {
                out.refused
                    .take()
                    .map(|m| format!("the host refused the result ({m})"))
            });
            return Err(error(message));
        }
        let c = out
            .finish()
            .map_err(|e| error(Some(format!("the result can not be read ({e:#})"))))?;
        if c.is_image() {
            if let Some(img) = c.try_to_image()? {
                return Ok(EncodedValue::Image(img));
            }
        } else if let Some(v) = c.try_to_value()? {
            return Ok(EncodedValue::Value(v));
        }
        Err(error(Some("encode wrote neither a value nor an image".to_owned())))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn type_tag(&self) -> Option<&NativeTypeTag> {
        self.tag.as_ref()
    }
    fn into_any(self: Box<Self>) -> Option<Box<dyn std::any::Any>> {
        Some(self)
    }
}

/// 引数の Native のうち、渡す先のプラグインが作ったものを探す (§10.3)
fn is_native_of(capsule: &CapsulePtr, target_abi: &str) -> JuizResult<bool> {
    if !capsule.is_native()? {
        return Ok(false);
    }
    capsule.lock_as_native(|n| {
        n.as_any()
            .downcast_ref::<RustCabiNative>()
            .is_some_and(|r| r.native_abi_tag() == target_abi)
    })
}

/// ハンドルのまま渡す引数の Native。呼び出しの間生かしておく
enum ArgNative {
    /// 持ち主を渡す (owned = 1)。渡せずに終わったら Drop で destroy する
    Owned(Box<RustCabiNative>),
    /// ホストが持ち続ける (owned = 0)。NativeCell を複製して持ち、呼び出しの間ハンドルを有効に保つ
    Shared(NativeCell),
}

impl ArgNative {
    /// カプセルから取り出す。ほかに参照が無ければ移し、あれば共有する
    fn take(capsule: CapsulePtr) -> JuizResult<ArgNative> {
        let shared = |cell: NativeCell| -> JuizResult<ArgNative> { Ok(ArgNative::Shared(cell)) };
        let cell = match capsule.try_into_capsule() {
            Err(ptr) => {
                let cell = ptr
                    .native_cell()?
                    .ok_or_else(|| anyhow!("the argument is no longer a native value"))?;
                return shared(cell);
            }
            Ok(c) => c
                .into_native_cell()
                .map_err(|_| anyhow!("the argument is no longer a native value"))?,
        };
        match cell.try_into_value() {
            Err(cell) => shared(cell),
            Ok(boxed) => match boxed.into_any().map(|a| a.downcast::<RustCabiNative>()) {
                Some(Ok(n)) => Ok(ArgNative::Owned(n)),
                _ => Err(anyhow!("the argument is not a native value of a Rust plugin")),
            },
        }
    }

    fn native(&self) -> &RustCabiNative {
        match self {
            ArgNative::Owned(n) => n,
            ArgNative::Shared(cell) => cell
                .value()
                .as_any()
                .downcast_ref::<RustCabiNative>()
                .expect("checked by is_native_of"),
        }
    }

    fn to_arg(&self) -> JuizRustNative {
        self.native().to_arg(matches!(self, ArgNative::Owned(_)))
    }

    /// 持ち主をプラグインに渡し終えた。destroy を呼ばずに消える
    fn forget_handle(&mut self) {
        if let ArgNative::Owned(n) = self {
            n.handle = std::ptr::null_mut();
        }
    }
}

// ---- 引数のフレーム (§5.2) ----

/// 引数のフレームを作って f に渡す。画像はカプセルをロックしたまま、その画素を直接指す。
/// 渡す先のプラグイン (native_abi_tag が target_abi) が作った Native はハンドルのまま渡し (§10.3)、
/// ほかの Native は encode する
fn with_frame_in<R>(
    target_abi: Option<&str>,
    mut map: CapsuleMap,
    f: impl FnOnce(&JuizFrameIn) -> R,
) -> JuizResult<R> {
    let mut natives: Vec<(String, ArgNative)> = Vec::new();
    if let Some(abi) = target_abi {
        let keys = map
            .iter()
            .filter_map(|(k, c)| match is_native_of(c, abi) {
                Ok(true) => Some(Ok(k.clone())),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<JuizResult<Vec<_>>>()?;
        for k in keys {
            let c = map.take(&k)?;
            natives.push((k, ArgNative::take(c)?));
        }
    }
    let mut frame = OutgoingFrame::call_request(&map)?;
    if !natives.is_empty() {
        let header_map = frame.header_mut().map.get_or_insert_with(Default::default);
        for (i, (k, _)) in natives.iter().enumerate() {
            header_map.insert(k.clone(), jvalue!({ "native": i }));
        }
    }
    let header = frame.header_bytes()?;
    let args: Vec<JuizRustNative> = natives.iter().map(|(_, n)| n.to_arg()).collect();
    let r = frame.with_body_slices(|bodies| {
        let descs: Vec<JuizBytes> = bodies.iter().map(|b| JuizBytes::from_slice(b)).collect();
        let frame_in = JuizFrameIn {
            header: JuizBytes::from_slice(&header),
            bodies: if descs.is_empty() {
                std::ptr::null()
            } else {
                descs.as_ptr()
            },
            body_count: descs.len(),
            natives: if args.is_empty() {
                std::ptr::null()
            } else {
                args.as_ptr()
            },
            native_count: args.len(),
        };
        let r = f(&frame_in);
        // 入口を呼んだら、owned = 1 のものの持ち主はプラグイン (どの値を返しても、§10.3)
        for (_, n) in natives.iter_mut() {
            n.forget_handle();
        }
        r
    });
    // 呼べなかったとき、owned = 1 のものは natives の Drop で destroy される
    drop(natives);
    r
}

// ---- 戻り値の書き込み口 (§5.3) ----

#[derive(Default)]
struct HostOut {
    receiver: Option<FrameReceiver>,
    /// プラグインが native で渡した Native (§10.2)
    natives: Vec<Capsule>,
    /// Native を受け取るときの出どころ。None なら native の口を渡さない (プラグインは encode して書く)
    native_origin: Option<NativeOrigin>,
    /// プラグインが error で渡した文
    error: Option<String>,
    /// ホストが begin / body / native を断った理由
    refused: Option<String>,
}

impl HostOut {
    /// 書き込み口。self を動かさず、呼び出しの間生かしておくこと
    fn frame_out(&mut self) -> JuizFrameOut {
        JuizFrameOut {
            ctx: self as *mut HostOut as *mut c_void,
            begin: Some(out_begin),
            body: Some(out_body),
            native: self.native_origin.as_ref().map(|_| out_native as _),
            error: Some(out_error),
        }
    }

    /// 書かれた戻り値を Capsule にする。begin が呼ばれていなければ Empty
    fn finish(&mut self) -> JuizResult<Capsule> {
        let natives = std::mem::take(&mut self.natives);
        match self.receiver.take() {
            None => Ok(Capsule::empty()),
            Some(r) => {
                let mut frame = r.finish()?;
                frame.set_natives(natives);
                frame.into_capsule()
            }
        }
    }
}

/// ホストの用意する関数も panic を外へ出さない (§4)
fn no_unwind<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

extern "C" fn out_begin(ctx: *mut c_void, header: JuizBytes) -> i64 {
    no_unwind(-1, || {
        // SAFETY: ctx は frame_out が渡した HostOut で、呼び出しの間有効
        let out = unsafe { &mut *(ctx as *mut HostOut) };
        let Some(bytes) = (unsafe { header.as_slice() }) else {
            out.refused = Some("the header is null".to_owned());
            return -1;
        };
        match FrameReceiver::begin(bytes, &FrameLimits::default()) {
            Ok(r) => {
                out.receiver = Some(r);
                0
            }
            Err(e) => {
                out.refused = Some(format!("{e:#}"));
                -1
            }
        }
    })
}

extern "C" fn out_body(ctx: *mut c_void, index: usize, ptr: *mut *mut u8, len: *mut usize) -> i64 {
    no_unwind(-1, || {
        // SAFETY: ctx は frame_out が渡した HostOut で、呼び出しの間有効
        let out = unsafe { &mut *(ctx as *mut HostOut) };
        if ptr.is_null() || len.is_null() {
            out.refused = Some("body() was called with null".to_owned());
            return -1;
        }
        let Some(receiver) = out.receiver.as_mut() else {
            out.refused = Some("body() was called before begin()".to_owned());
            return -1;
        };
        let Some(dst) = receiver.body_mut(index) else {
            out.refused = Some(format!("body {index} is not in the header"));
            return -1;
        };
        // SAFETY: ptr と len は null でないことを確かめた。領域は receiver が持ち、呼び出しの間動かない
        unsafe {
            *ptr = dst.as_mut_ptr();
            *len = dst.len();
        }
        0
    })
}

extern "C" fn out_native(ctx: *mut c_void, index: usize, native: JuizRustNative) -> i64 {
    no_unwind(-1, || {
        // SAFETY: ctx は frame_out が渡した HostOut で、呼び出しの間有効
        let out = unsafe { &mut *(ctx as *mut HostOut) };
        if out.receiver.is_none() {
            out.refused = Some("native() was called before begin()".to_owned());
            return -1;
        }
        let Some(origin) = out.native_origin.clone() else {
            out.refused = Some("native() is not offered".to_owned());
            return -1;
        };
        // 番号は 0 から順に渡す
        if index != out.natives.len() {
            out.refused = Some(format!(
                "native {index} was passed, but native {} is expected next",
                out.natives.len()
            ));
            return -1;
        }
        match RustCabiNative::new(&native, origin) {
            Ok(n) => {
                out.natives.push(Capsule::from_native(n));
                0
            }
            Err(m) => {
                // 受け取らない。持ち主はプラグインのまま (プラグインが解放する)
                out.refused = Some(m);
                -1
            }
        }
    })
}

extern "C" fn out_error(ctx: *mut c_void, message: JuizBytes) {
    no_unwind((), || {
        // SAFETY: ctx は frame_out が渡した HostOut で、呼び出しの間有効
        let out = unsafe { &mut *(ctx as *mut HostOut) };
        let bytes = unsafe { message.as_slice() }.unwrap_or_default();
        out.error = Some(String::from_utf8_lossy(bytes).into_owned());
    })
}

// ---- 小さなバイト列の受け取り口 ----

extern "C" fn sink_write(ctx: *mut c_void, bytes: JuizBytes) {
    no_unwind((), || {
        // SAFETY: ctx は collect_bytes が渡した Option<Vec<u8>> で、呼び出しの間有効
        let dst = unsafe { &mut *(ctx as *mut Option<Vec<u8>>) };
        *dst = Some(unsafe { bytes.as_slice() }.unwrap_or_default().to_vec());
    })
}

/// sink を用意して f を呼び、書かれたバイト列を返す (manifest・info・エラー文)
fn collect_bytes(f: impl FnOnce(*const JuizBytesSink) -> i64) -> (i64, Option<Vec<u8>>) {
    let mut got: Option<Vec<u8>> = None;
    let sink = JuizBytesSink {
        ctx: &mut got as *mut Option<Vec<u8>> as *mut c_void,
        write: Some(sink_write),
    };
    let code = f(&sink);
    (code, got)
}

// ---- ファクトリ ----

/// Process (kind 1) のファクトリを作る
pub fn process_factory(entry: CabiEntry) -> JuizResult<ProcessFactoryPtr> {
    let manifest: ProcessManifest = entry.manifest_as()?;
    let entry = Arc::new(entry);
    crate::processes::process_factory_create_from_trait(manifest, move |args: CapsuleMap| {
        entry.call_process(args)
    })
}

/// Component (kind 4) の manifest を読む
pub fn component_manifest(entry: &CabiEntry) -> JuizResult<ComponentManifest> {
    entry.manifest_as()
}

/// ホストの側のコンテナの入れ物 (§8.1)。中身はプラグインの中の `ContainerImpl<T>` で、ホストは見ない
pub struct RustCabiContainer {
    handle: *mut c_void,
    destroy: JuizRustDestroyFn,
    /// 作ったコンテナの型タグ (§8.2)
    container_tag: String,
    /// destroy を呼ぶまでプラグインを外さない
    _lib: Arc<Library>,
}

impl RustCabiContainer {
    /// 型タグ (§8.2)
    pub fn container_tag(&self) -> &str {
        &self.container_tag
    }
}

impl Drop for RustCabiContainer {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            log::trace!("RustCabiContainer::drop() calls destroy");
            (self.destroy)(self.handle);
            self.handle = std::ptr::null_mut();
        }
    }
}

/// Container (kind 2) のファクトリを作る
pub fn container_factory(entry: CabiEntry) -> JuizResult<ContainerFactoryPtr> {
    let manifest: ContainerManifest = entry.manifest_as()?;
    let container_tag = entry.container_tag()?;
    let entry = Arc::new(entry);
    let constructor = move |cm: ContainerManifest, args: CapsuleMap| -> JuizResult<ContainerPtr> {
        let create = entry
            .create
            .ok_or_else(|| entry.error("create", 0, Some("is null".to_owned())))?;
        let destroy = entry
            .destroy
            .ok_or_else(|| entry.error("destroy", 0, Some("is null".to_owned())))?;
        let manifest_json = serde_json::to_vec(&cm)?;
        let mut handle: *mut c_void = std::ptr::null_mut();
        let (code, message) = with_frame_in(entry.native_abi_tag.as_deref(), args, |frame_in| {
            collect_bytes(|error| {
                create(
                    JuizBytes::from_slice(&manifest_json),
                    frame_in,
                    &mut handle,
                    error,
                )
            })
        })
        .map_err(|e| entry.error("create", 0, Some(format!("the arguments can not be passed ({e:#})"))))?;
        if code < 0 {
            let message = message.map(|b| String::from_utf8_lossy(&b).into_owned());
            return Err(entry.error("create", code, message));
        }
        if handle.is_null() {
            return Err(entry.error("create", code, Some("returned a null handle".to_owned())));
        }
        let holder = RustCabiContainer {
            handle,
            destroy,
            container_tag: container_tag.clone(),
            _lib: entry.lib.clone(),
        };
        Ok(ContainerPtr::new(ContainerImpl::new(cm, Box::new(holder))?))
    };
    crate::containers::container_factory_create(manifest, Arc::new(constructor))
}

/// ContainerProcess (kind 3) のファクトリを作る
pub fn container_process_factory(entry: CabiEntry) -> JuizResult<ContainerProcessFactoryPtr> {
    let manifest: ProcessManifest = entry.manifest_as()?;
    let container_tag = entry.container_tag()?;
    let entry = Arc::new(entry);
    let function = move |container: ContainerPtr, args: CapsuleMap| -> JuizResult<Capsule> {
        let container_call = entry
            .container_call
            .ok_or_else(|| entry.error("container_call", 0, Some("is null".to_owned())))?;
        // ロックは呼び出しが終わるまで持つ。同じコンテナのコンテナプロセスは同時に呼ばれない (§8.3)
        let mut guard = container.lock_mut()?;
        let Some(c) = guard.downcast_mut::<ContainerImpl<RustCabiContainer>>() else {
            log::error!(
                "【失敗】{} expects a container loaded through the Rust plugin C ABI, but the container is not",
                entry.symbol
            );
            return Err(anyhow!(JuizError::ContainerDowncastingError {
                identifier: format!(
                    "{} (the container process is loaded through the Rust plugin C ABI, but the container is not. \
                     Rebuild the container and its container processes with the same juiz_sdk)",
                    container.identifier()
                ),
            }));
        };
        if c.t.container_tag != container_tag {
            return Err(anyhow!(JuizError::ContainerTypeMismatchError {
                expected: container_tag.clone(),
                actual: c.t.container_tag.clone(),
            }));
        }
        let handle = c.t.handle;
        with_frame_in(entry.native_abi_tag.as_deref(), args, |frame_in| {
            entry.call_with_out("container_call", |out| container_call(handle, frame_in, out))
        })
        .map_err(|e| {
            entry.error(
                "container_call",
                0,
                Some(format!("the arguments can not be passed ({e:#})")),
            )
        })?
    };
    crate::containers::container_process_factory_create(manifest, Arc::new(function))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_abi_version_is_accepted() {
        assert!(check_abi_version("libp.so", "process_factory_cabi", 1).is_ok());
    }

    #[test]
    fn unknown_abi_version_is_rejected_with_hint() {
        for (abi, hint) in [(2, "Update juiz."), (0, "Rebuild the plugin with a newer juiz_sdk.")] {
            let err = check_abi_version("libp.so", "process_factory_cabi", abi).unwrap_err();
            match err.downcast_ref::<JuizError>() {
                Some(JuizError::RustPluginAbiVersionError {
                    plugin,
                    symbol,
                    plugin_abi,
                    host_abi,
                    hint: h,
                }) => {
                    assert_eq!(plugin, "libp.so");
                    assert_eq!(symbol, "process_factory_cabi");
                    assert_eq!(*plugin_abi, abi);
                    assert_eq!(host_abi, "1");
                    assert_eq!(h, hint);
                }
                other => panic!("unexpected error {other:?}"),
            }
            assert!(err.to_string().contains(hint), "{err}");
        }
    }

    #[test]
    fn call_error_names_the_code_when_the_plugin_gives_no_message() {
        let err = call_error("libp.so", "process_factory_cabi", "call", -805, None);
        match err.downcast_ref::<JuizError>() {
            Some(JuizError::RustPluginCallError { symbol, message, .. }) => {
                assert_eq!(symbol, "process_factory_cabi.call");
                assert_eq!(message, "returned -805 (JUIZ_RUST_NULL_ARGUMENT)");
            }
            other => panic!("unexpected error {other:?}"),
        }
    }
}
