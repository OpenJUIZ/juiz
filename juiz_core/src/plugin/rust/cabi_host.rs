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

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::cabi::{
    error_code_name, JuizBytes, JuizBytesSink, JuizFrameIn, JuizFrameOut, JuizRustCallFn,
    JuizRustContainerCallFn, JuizRustCreateFn, JuizRustDestroyFn, JuizRustEntry,
    JuizRustManifestFn, CABI_SYMBOL_SUFFIX, JUIZ_RUST_HOST_REFUSED, KIND_COMPONENT,
    KIND_CONTAINER, KIND_CONTAINER_PROCESS, KIND_PROCESS,
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
        let entry = CabiEntry {
            lib: lib.clone(),
            plugin: plugin.to_owned(),
            symbol: name,
            manifest,
            info,
            call: e.call,
            create: e.create,
            destroy: e.destroy,
            container_call: e.container_call,
        };
        let info = entry.info_json()?;
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
        let mut out = HostOut::default();
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
        match out.receiver.take() {
            None => Ok(Capsule::empty()),
            Some(r) => r
                .finish()
                .and_then(|frame| frame.into_capsule())
                .map_err(|e| self.error(func, code, Some(format!("the result can not be read ({e:#})")))),
        }
    }

    /// Process (kind 1) を呼ぶ
    fn call_process(&self, args: &CapsuleMap) -> JuizResult<Capsule> {
        let call = self.call.ok_or_else(|| self.error("call", 0, Some("is null".to_owned())))?;
        with_frame_in(args, |frame_in| {
            self.call_with_out("call", |out| call(frame_in, out))
        })
        .map_err(|e| self.error("call", 0, Some(format!("the arguments can not be passed ({e:#})"))))?
    }
}

// ---- 引数のフレーム (§5.2) ----

/// 引数のフレームを作って f に渡す。画像はカプセルをロックしたまま、その画素を直接指す
fn with_frame_in<R>(map: &CapsuleMap, f: impl FnOnce(&JuizFrameIn) -> R) -> JuizResult<R> {
    let frame = OutgoingFrame::call_request(map)?;
    let header = frame.header_bytes()?;
    frame.with_body_slices(|bodies| {
        let descs: Vec<JuizBytes> = bodies.iter().map(|b| JuizBytes::from_slice(b)).collect();
        let frame_in = JuizFrameIn {
            header: JuizBytes::from_slice(&header),
            bodies: if descs.is_empty() {
                std::ptr::null()
            } else {
                descs.as_ptr()
            },
            body_count: descs.len(),
            natives: std::ptr::null(),
            native_count: 0,
        };
        f(&frame_in)
    })
}

// ---- 戻り値の書き込み口 (§5.3) ----

#[derive(Default)]
struct HostOut {
    receiver: Option<FrameReceiver>,
    /// プラグインが error で渡した文
    error: Option<String>,
    /// ホストが begin / body を断った理由
    refused: Option<String>,
}

impl HostOut {
    /// 書き込み口。self を動かさず、呼び出しの間生かしておくこと
    fn frame_out(&mut self) -> JuizFrameOut {
        JuizFrameOut {
            ctx: self as *mut HostOut as *mut c_void,
            begin: Some(out_begin),
            body: Some(out_body),
            native: None,
            error: Some(out_error),
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
        entry.call_process(&args)
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
        let (code, message) = with_frame_in(&args, |frame_in| {
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
        with_frame_in(&args, |frame_in| {
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
