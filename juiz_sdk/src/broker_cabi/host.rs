//! ブローカーの C ABI のホスト側の道具 (docs/spec/broker-plugin-abi.md §3・§4・§6・§8)
//!
//! 入口の表を読んで確かめ ([`BrokerEntry::load`])、サーバー ([`RunningServer`]) とクライアント ([`Client`]) を作る。
//! C の構造体は Rust の `Read` / `Write` ([`PluginReader`]・[`PluginWriter`]) にして渡すので、使う側は
//! フレームの中身の処理だけを書けばよい。juiz_core の C ABI のブローカーはこれの上に作る (§6)。
//!
//! フレームの形はここでは扱わない (バイト列のまま)。[`EchoHandler`] は受け取った要求をそのまま応答に返すので、
//! juiz_core なしでプラグインのサーバーとクライアントをつないで確かめられる (ループバック、§8)。

use std::any::Any;
use std::collections::HashSet;
use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};

use super::*;
use crate::prelude::*;
use anyhow::anyhow;

/// このホストが読めるブローカーの ABI の版 (§9.2)
pub const SUPPORTED_BROKER_ABI_VERSIONS: &[u32] = &[1];

fn supported_abi_str() -> String {
    SUPPORTED_BROKER_ABI_VERSIONS
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// info の JSON (§3.3) を読んだもの
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerInfo {
    pub type_name: String,
    pub abi_version: u32,
    pub plugin_name: String,
    pub plugin_version: String,
    pub build_info: Option<String>,
    pub default_port: Option<u16>,
}

impl BrokerInfo {
    fn from_json(v: &Value) -> Result<Self, String> {
        let s = |k: &str| -> Result<String, String> {
            v.get(k)
                .and_then(|x| x.as_str())
                .map(|x| x.to_owned())
                .ok_or_else(|| format!("info has no '{k}' ({v})"))
        };
        let type_name = s("type_name")?;
        if !valid_type_name(&type_name) {
            return Err(format!(
                "info.type_name '{type_name}' must be lower-case letters, digits and '_', starting with a letter"
            ));
        }
        let abi_version = v
            .get("abi_version")
            .and_then(|x| x.as_u64())
            .and_then(|x| u32::try_from(x).ok())
            .ok_or_else(|| format!("info has no 'abi_version' ({v})"))?;
        let default_port = match v.get("default_port") {
            None | Some(Value::Null) => None,
            Some(p) => Some(
                p.as_u64()
                    .and_then(|x| u16::try_from(x).ok())
                    .ok_or_else(|| format!("info.default_port is not a port ({p})"))?,
            ),
        };
        Ok(BrokerInfo {
            type_name,
            abi_version,
            plugin_name: s("plugin_name")?,
            plugin_version: s("plugin_version")?,
            build_info: v.get("build_info").and_then(|x| x.as_str()).map(|x| x.to_owned()),
            default_port,
        })
    }
}

/// 種類名の決まり (§3.3): 英小文字・数字・`_` で、英小文字で始まる
fn valid_type_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// 読み込んで確かめた入口の表。関数ポインタを写して持つ
pub struct BrokerEntry {
    plugin: String,
    info: BrokerInfo,
    server_start: JuizBrokerServerStartFn,
    server_stop: JuizBrokerServerStopFn,
    client_connect: JuizBrokerClientConnectFn,
    client_request: JuizBrokerClientRequestFn,
    client_close: JuizBrokerClientCloseFn,
    /// ライブラリを外さないために持つもの (juiz_core の `Arc<Library>`)。組み込みなら None
    _keep: Option<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for BrokerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerEntry")
            .field("plugin", &self.plugin)
            .field("info", &self.info)
            .finish()
    }
}

fn load_error(plugin: &str, message: impl Into<String>) -> anyhow::Error {
    let message = message.into();
    log::error!("【失敗】broker plugin ({plugin}) can not be loaded. {message}");
    anyhow!(JuizError::BrokerPluginLoadError { plugin: plugin.to_owned(), message })
}

/// 小さなバイト列 (info・名前・エラー文) を受け取る sink を用意して f を呼ぶ
fn collect_bytes<R>(f: impl FnOnce(*const JuizBytesSink) -> R) -> (R, Option<Vec<u8>>) {
    extern "C" fn write(ctx: *mut c_void, bytes: JuizBytes) {
        // SAFETY: ctx は collect_bytes の Option<Vec<u8>> を指す
        let slot = unsafe { &mut *(ctx as *mut Option<Vec<u8>>) };
        // SAFETY: プラグインは bytes を呼び出しの間有効に保つ
        if let Some(b) = unsafe { bytes.as_slice() } {
            *slot = Some(b.to_vec());
        }
    }
    let mut slot: Option<Vec<u8>> = None;
    let sink = JuizBytesSink {
        ctx: &mut slot as *mut Option<Vec<u8>> as *mut c_void,
        write: Some(write),
    };
    let r = f(&sink);
    (r, slot)
}

/// init を呼んだ表 (表のアドレス)。同じ表を何度読み込んでも init は 1 回だけ呼ぶ (§3.4)
static INITIALIZED: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

impl BrokerEntry {
    /// 入口の関数 `broker_factory_cabi` を呼んで表を読み、確かめる (§3.2・§9.2)。
    /// info を読み、初めての表なら init を呼ぶ。plugin はエラーの文に出す名前 (プラグインのパスなど)、
    /// keep はライブラリを外さないために持つもの
    ///
    /// # Safety
    /// entry_fn は broker-plugin-abi.md §3.1 の入口でなければならない
    pub unsafe fn load(
        entry_fn: JuizBrokerEntryFn,
        plugin: &str,
        keep: Option<Arc<dyn Any + Send + Sync>>,
    ) -> JuizResult<Arc<BrokerEntry>> {
        let p = entry_fn();
        if p.is_null() {
            return Err(load_error(plugin, format!("{ENTRY_SYMBOL}() returned null")));
        }
        // 版を最初に読み、知らない版なら残りを読まない (§3.2)
        let abi_version = std::ptr::addr_of!((*p).abi_version).read();
        if !SUPPORTED_BROKER_ABI_VERSIONS.contains(&abi_version) {
            let newest = SUPPORTED_BROKER_ABI_VERSIONS.iter().copied().max().unwrap_or(0);
            let hint = if abi_version > newest {
                "Update juiz.".to_owned()
            } else {
                "Install a newer version of the broker (juiz broker install <name> --force).".to_owned()
            };
            log::error!("【失敗】broker plugin ({plugin}) uses broker plugin ABI {abi_version}");
            return Err(anyhow!(JuizError::BrokerPluginAbiVersionError {
                plugin: plugin.to_owned(),
                plugin_abi: abi_version,
                host_abi: supported_abi_str(),
                hint,
            }));
        }
        let struct_size = std::ptr::addr_of!((*p).struct_size).read() as usize;
        let known = std::mem::size_of::<JuizBrokerEntry>();
        if struct_size < known {
            return Err(load_error(
                plugin,
                format!("the entry table is {struct_size} bytes, smaller than {known} bytes of broker plugin ABI {abi_version}"),
            ));
        }
        // SAFETY: 版と大きさを確かめた。表はプラグインを外すまで有効 (§3.1)
        let e = &*p;
        let missing = |what: &str| load_error(plugin, format!("the entry table has no '{what}' (null)"));
        let info_fn = e.info.ok_or_else(|| missing("info"))?;
        let init_fn = e.init.ok_or_else(|| missing("init"))?;
        let entry = BrokerEntry {
            plugin: plugin.to_owned(),
            info: BrokerInfo {
                type_name: String::new(),
                abi_version,
                plugin_name: String::new(),
                plugin_version: String::new(),
                build_info: None,
                default_port: None,
            },
            server_start: e.server_start.ok_or_else(|| missing("server_start"))?,
            server_stop: e.server_stop.ok_or_else(|| missing("server_stop"))?,
            client_connect: e.client_connect.ok_or_else(|| missing("client_connect"))?,
            client_request: e.client_request.ok_or_else(|| missing("client_request"))?,
            client_close: e.client_close.ok_or_else(|| missing("client_close"))?,
            _keep: keep,
        };

        // info (§3.3)
        let (code, bytes) = collect_bytes(|out| guard_call(|| info_fn(out)));
        if code < 0 {
            let m = message_of(code, bytes);
            return Err(load_error(plugin, format!("{ENTRY_SYMBOL}.info failed: {m}")));
        }
        let v: Value = bytes
            .as_deref()
            .ok_or_else(|| "wrote nothing".to_owned())
            .and_then(|b| serde_json::from_slice(b).map_err(|e| format!("wrote invalid JSON ({e})")))
            .map_err(|m| load_error(plugin, format!("{ENTRY_SYMBOL}.info {m}")))?;
        let info = BrokerInfo::from_json(&v).map_err(|m| load_error(plugin, m))?;
        if info.abi_version != abi_version {
            return Err(load_error(
                plugin,
                format!(
                    "info.abi_version ({}) does not match the entry table ({abi_version})",
                    info.abi_version
                ),
            ));
        }

        // init は表ごとに 1 回だけ (§3.4)
        let key = p as usize;
        let mut inited = INITIALIZED.lock().unwrap_or_else(|e| e.into_inner());
        let set = inited.get_or_insert_with(HashSet::new);
        if !set.contains(&key) {
            let (code, bytes) = collect_bytes(|err| guard_call(|| init_fn(host_services(), err)));
            if code < 0 {
                let m = message_of(code, bytes);
                return Err(load_error(plugin, format!("{ENTRY_SYMBOL}.init failed: {m}")));
            }
            set.insert(key);
        }
        drop(inited);

        log::debug!(
            "broker plugin ({plugin}) '{}' ({} {}) is loaded through the broker plugin ABI {abi_version} (build_info: {})",
            info.type_name,
            info.plugin_name,
            info.plugin_version,
            info.build_info.as_deref().unwrap_or("unknown")
        );
        Ok(Arc::new(BrokerEntry { info, ..entry }))
    }

    pub fn info(&self) -> &BrokerInfo {
        &self.info
    }

    pub fn type_name(&self) -> &str {
        &self.info.type_name
    }

    /// エラーの文に出す名前 (プラグインのパスなど)
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    fn call_error(&self, func: &str, message: String) -> anyhow::Error {
        let symbol = format!("{ENTRY_SYMBOL}.{func}");
        log::error!("【失敗】broker plugin ({}) '{symbol}': {message}", self.plugin);
        anyhow!(JuizError::BrokerPluginCallError {
            plugin: self.plugin.clone(),
            symbol,
            message,
        })
    }

    /// サーバーを始める (§5.3)。待ち受けを始めたところで戻る。handler は要求ごとにプラグインのスレッドから
    /// (同時にいくつも) 呼ばれる
    pub fn start_server(self: &Arc<Self>, manifest: &Value, handler: Arc<dyn ServerHandler>) -> JuizResult<RunningServer> {
        let json = serde_json::to_vec(manifest)?;
        let ctx: Box<Arc<dyn ServerHandler>> = Box::new(handler);
        let mut table = Box::new(JuizBrokerServerHost {
            abi_version: 1,
            struct_size: std::mem::size_of::<JuizBrokerServerHost>() as u32,
            ctx: std::ptr::null_mut(),
            handle: Some(host_handle),
        });
        table.ctx = &*ctx as *const Arc<dyn ServerHandler> as *mut c_void;
        let mut handle: *mut c_void = std::ptr::null_mut();
        let mut name: Option<Vec<u8>> = None;
        let (code, error) = collect_bytes(|err| {
            let (code, n) = collect_bytes(|name_sink| {
                guard_call(|| (self.server_start)(JuizBytes::from_slice(&json), &*table, &mut handle, name_sink, err))
            });
            name = n;
            code
        });
        if code < 0 {
            return Err(self.call_error("server_start", message_of(code, error)));
        }
        let name = name.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        if name.is_empty() {
            let r = RunningServer { entry: self.clone(), handle, _table: table, _ctx: ctx, name: String::new() };
            drop(r);
            return Err(self.call_error("server_start", "returned no name".to_owned()));
        }
        Ok(RunningServer { entry: self.clone(), handle, _table: table, _ctx: ctx, name })
    }

    /// クライアントを作る (§5.4)
    pub fn connect(self: &Arc<Self>, manifest: &Value) -> JuizResult<Client> {
        let json = serde_json::to_vec(manifest)?;
        let mut handle: *mut c_void = std::ptr::null_mut();
        let (code, error) =
            collect_bytes(|err| guard_call(|| (self.client_connect)(JuizBytes::from_slice(&json), &mut handle, err)));
        if code < 0 {
            return Err(self.call_error("client_connect", message_of(code, error)));
        }
        Ok(Client { entry: self.clone(), handle })
    }
}

/// プラグインの関数を呼ぶ。プラグインは panic を外に出さない約束だが、出てきたら止める
fn guard_call(f: impl FnOnce() -> i64) -> i64 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(_) => JUIZ_BROKER_PANIC,
    }
}

/// エラーの文。プラグインが書かなければ `returned <コード> (<名前>)`
fn message_of(code: i64, bytes: Option<Vec<u8>>) -> String {
    match bytes {
        Some(b) if !b.is_empty() => String::from_utf8_lossy(&b).into_owned(),
        _ => format!("returned {code} ({})", error_code_name(code)),
    }
}

// ---- ログの口 (§3.4) ----

struct Services(JuizBrokerHostServices);
// SAFETY: ctx は使わない (null)。log はどのスレッドから呼んでもよい
unsafe impl Send for Services {}
unsafe impl Sync for Services {}

/// init に渡すサービス。プロセスで 1 つで、終わるまで有効
fn host_services() -> *const JuizBrokerHostServices {
    static SERVICES: OnceLock<Services> = OnceLock::new();
    &SERVICES
        .get_or_init(|| {
            Services(JuizBrokerHostServices {
                abi_version: 1,
                struct_size: std::mem::size_of::<JuizBrokerHostServices>() as u32,
                ctx: std::ptr::null_mut(),
                log: Some(host_log),
                max_level: log::max_level() as u32,
            })
        })
        .0
}

thread_local! {
    /// ログを受けている最中か。プラグインを組み込んで同じ log クレートを使うときに回り続けないようにする
    static IN_HOST_LOG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

extern "C" fn host_log(_ctx: *mut c_void, level: u32, target: JuizBytes, message: JuizBytes) {
    if IN_HOST_LOG.with(|l| l.replace(true)) {
        return;
    }
    let _ = catch_unwind(|| {
        let level = match level {
            1 => log::Level::Error,
            2 => log::Level::Warn,
            3 => log::Level::Info,
            4 => log::Level::Debug,
            _ => log::Level::Trace,
        };
        // SAFETY: プラグインは target・message を呼び出しの間有効に保つ
        let target = unsafe { target.as_slice() }.map(String::from_utf8_lossy).unwrap_or_default();
        let message = unsafe { message.as_slice() }.map(String::from_utf8_lossy).unwrap_or_default();
        log::logger().log(
            &log::Record::builder()
                .level(level)
                .target(&target)
                .args(format_args!("{message}"))
                .build(),
        );
    });
    IN_HOST_LOG.with(|l| l.set(false));
}

// ---- 読み口・書き口を Rust の Read / Write にしたもの ----

/// プラグインの読み口 (§4)。`Read` で読む。len があれば、それより多くは読まない
pub struct PluginReader<'a> {
    raw: &'a JuizFrameReader,
    remaining: Option<u64>,
    /// プラグインの read が負の値を返したときのコード
    failed: Option<i64>,
}

impl<'a> PluginReader<'a> {
    fn new(raw: &'a JuizFrameReader) -> Self {
        let remaining = if raw.len == u64::MAX { None } else { Some(raw.len) };
        PluginReader { raw, remaining, failed: None }
    }

    /// フレームの長さ。わからなければ None
    pub fn len(&self) -> Option<u64> {
        if self.raw.len == u64::MAX {
            None
        } else {
            Some(self.raw.len)
        }
    }

    /// プラグインの read が失敗したか (通り道の失敗)。そのときのコード
    pub fn plugin_failed(&self) -> Option<i64> {
        self.failed
    }
}

impl Read for PluginReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let cap = match self.remaining {
            Some(0) => return Ok(0),
            Some(rest) => buf.len().min(usize::try_from(rest).unwrap_or(usize::MAX)),
            None => buf.len(),
        };
        if cap == 0 {
            return Ok(0);
        }
        let read = self
            .raw
            .read
            .ok_or_else(|| io::Error::other("the broker plugin's reader has no read (null)"))?;
        let mut n: usize = 0;
        let code = guard_call(|| read(self.raw.ctx, buf.as_mut_ptr(), cap, &mut n));
        if code < 0 {
            self.failed = Some(code);
            return Err(io::Error::other(format!(
                "the broker plugin's read returned {code} ({})",
                error_code_name(code)
            )));
        }
        if n > cap {
            self.failed = Some(JUIZ_BROKER_ERROR);
            return Err(io::Error::other(format!("the broker plugin's read returned {n} bytes, more than {cap}")));
        }
        if let Some(rest) = self.remaining.as_mut() {
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("the frame ended {rest} bytes before its length"),
                ));
            }
            *rest -= n as u64;
        }
        Ok(n)
    }
}

/// プラグインの書き口 (§4)。`Write` で書く
pub struct PluginWriter<'a> {
    raw: &'a JuizFrameWriter,
    written: u64,
    failed: Option<i64>,
}

impl PluginWriter<'_> {
    /// これまでに書いたバイト数
    pub fn written(&self) -> u64 {
        self.written
    }
}

impl Write for PluginWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        let write = self
            .raw
            .write
            .ok_or_else(|| io::Error::other("the broker plugin's writer has no write (null)"))?;
        let code = guard_call(|| write(self.raw.ctx, JuizBytes::from_slice(buf)));
        if code < 0 {
            self.failed = Some(code);
            return Err(io::Error::other(format!(
                "the broker plugin's write returned {code} ({})",
                error_code_name(code)
            )));
        }
        self.written += buf.len() as u64;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 送るフレームを書く関数。書けなければ文を返す
type WriteFn<'f> = dyn FnMut(&mut PluginWriter<'_>) -> Result<(), String> + 'f;

struct SourceCtx<'f> {
    len: u64,
    write: &'f mut WriteFn<'f>,
    called: bool,
    /// ホストが断った理由 (書き方の失敗)
    refused: Option<String>,
    /// プラグインの write が失敗した
    plugin_failed: bool,
}

extern "C" fn source_write_to(ctx: *mut c_void, writer: *const JuizFrameWriter) -> i64 {
    // SAFETY: ctx は with_source の SourceCtx を指し、その間だけ呼ばれる
    let ctx = unsafe { &mut *(ctx as *mut SourceCtx<'_>) };
    // SAFETY: プラグインは writer を呼び出しの間有効に保つ
    let Some(raw) = (unsafe { writer.as_ref() }) else {
        return JUIZ_BROKER_NULL_ARGUMENT;
    };
    if ctx.called {
        ctx.refused = Some("write_to was called twice".to_owned());
        return JUIZ_BROKER_FRAME_ERROR;
    }
    ctx.called = true;
    let mut w = PluginWriter { raw, written: 0, failed: None };
    let r = catch_unwind(AssertUnwindSafe(|| (ctx.write)(&mut w)));
    let r = match r {
        Ok(r) => r,
        Err(p) => Err(super::panic_message(p.as_ref())),
    };
    match r {
        Ok(()) if w.written == ctx.len => JUIZ_OK,
        Ok(()) => {
            ctx.refused = Some(format!("the host wrote {} bytes, not {} bytes", w.written, ctx.len));
            JUIZ_BROKER_FRAME_ERROR
        }
        Err(_) if w.failed.is_some() => {
            ctx.plugin_failed = true;
            JUIZ_BROKER_SEND_FAILED
        }
        Err(m) => {
            ctx.refused = Some(m);
            JUIZ_BROKER_FRAME_ERROR
        }
    }
}

/// 送るフレームの構造体を作って f に渡す。戻り値は f の戻り値と、ホストが断った理由
fn with_source<'f, R>(len: u64, write: &'f mut WriteFn<'f>, f: impl FnOnce(&JuizFrameSource) -> R) -> (R, Option<String>, bool) {
    let mut ctx = SourceCtx { len, write, called: false, refused: None, plugin_failed: false };
    let source = JuizFrameSource {
        ctx: &mut ctx as *mut SourceCtx<'_> as *mut c_void,
        len,
        write_to: Some(source_write_to),
    };
    let r = f(&source);
    let refused = if ctx.called { ctx.refused } else { Some("the plugin did not write the frame".to_owned()) };
    (r, refused, ctx.plugin_failed)
}

/// 受け取ったフレームを読む関数。読めなければ文を返す
type ReceiveFn<'f> = dyn FnMut(&mut PluginReader<'_>) -> Result<(), String> + 'f;

struct ReceiverCtx<'f> {
    receive: &'f mut ReceiveFn<'f>,
    called: bool,
    refused: Option<String>,
}

extern "C" fn receiver_receive(ctx: *mut c_void, reader: *const JuizFrameReader) -> i64 {
    // SAFETY: ctx は Client::request の ReceiverCtx を指し、その間だけ呼ばれる
    let ctx = unsafe { &mut *(ctx as *mut ReceiverCtx<'_>) };
    // SAFETY: プラグインは reader を呼び出しの間有効に保つ
    let Some(raw) = (unsafe { reader.as_ref() }) else {
        return JUIZ_BROKER_NULL_ARGUMENT;
    };
    if ctx.called {
        ctx.refused = Some("receive was called twice".to_owned());
        return JUIZ_BROKER_FRAME_ERROR;
    }
    ctx.called = true;
    let mut r = PluginReader::new(raw);
    let result = catch_unwind(AssertUnwindSafe(|| (ctx.receive)(&mut r)));
    let result = match result {
        Ok(x) => x,
        Err(p) => Err(super::panic_message(p.as_ref())),
    };
    match result {
        Ok(()) => JUIZ_OK,
        Err(m) => {
            ctx.refused = Some(m);
            if r.failed.is_some() {
                JUIZ_BROKER_SEND_FAILED
            } else {
                JUIZ_BROKER_FRAME_ERROR
            }
        }
    }
}

// ---- サーバー (§5.3) ----

/// サーバーの要求を処理するもの。プラグインのスレッドから同時にいくつも呼ばれる
pub trait ServerHandler: Send + Sync + 'static {
    /// request から要求を読み、reply で応答をちょうど 1 回送る。戻り値は §5.3 の handle の戻り値
    /// (`JUIZ_OK`・`JUIZ_BROKER_FRAME_ERROR`・`JUIZ_BROKER_SEND_FAILED`)
    fn handle(&self, peer_addr: &str, request: &mut PluginReader<'_>, reply: PluginSender<'_>) -> i64;
}

/// サーバーの応答の送り先 (§4 の juiz_frame_sender)
pub struct PluginSender<'a> {
    raw: &'a JuizFrameSender,
}

impl PluginSender<'_> {
    /// 長さ len のフレームを送る。write はプラグインの書き口へちょうど len バイト書く。
    /// 失敗したら、プラグインの send のコードと、ホストが断った理由 (あれば)
    pub fn send(
        self,
        len: u64,
        mut write: impl FnMut(&mut PluginWriter<'_>) -> Result<(), String>,
    ) -> Result<(), (i64, Option<String>)> {
        let send = self.raw.send.ok_or((JUIZ_BROKER_NULL_ARGUMENT, None))?;
        let ctx = self.raw.ctx;
        let (code, refused, _) = with_source(len, &mut write, |source| guard_call(|| send(ctx, source)));
        if code < 0 {
            return Err((code, refused));
        }
        Ok(())
    }
}

extern "C" fn host_handle(
    ctx: *mut c_void,
    peer_addr: JuizBytes,
    request: *const JuizFrameReader,
    reply: *const JuizFrameSender,
) -> i64 {
    // SAFETY: ctx は RunningServer の Box<Arc<dyn ServerHandler>> を指し、server_stop が戻るまで有効
    let handler = unsafe { &*(ctx as *const Arc<dyn ServerHandler>) };
    // SAFETY: プラグインは request・reply・peer_addr を呼び出しの間有効に保つ
    let (Some(request), Some(reply)) = (unsafe { request.as_ref() }, unsafe { reply.as_ref() }) else {
        return JUIZ_BROKER_NULL_ARGUMENT;
    };
    let peer = unsafe { peer_addr.as_slice() }.map(String::from_utf8_lossy).unwrap_or_default();
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut reader = PluginReader::new(request);
        handler.handle(&peer, &mut reader, PluginSender { raw: reply })
    }));
    match r {
        Ok(code) => code,
        Err(p) => {
            let m = super::panic_message(p.as_ref());
            let _ = catch_unwind(|| log::error!("broker plugin host: handling a request {m}"));
            JUIZ_BROKER_ERROR
        }
    }
}

/// 待ち受け中のサーバー。落とすと server_stop を呼ぶ
pub struct RunningServer {
    entry: Arc<BrokerEntry>,
    handle: *mut c_void,
    /// プラグインに渡した表とその ctx。server_stop が戻るまで持つ
    _table: Box<JuizBrokerServerHost>,
    _ctx: Box<Arc<dyn ServerHandler>>,
    name: String,
}

// SAFETY: handle はプラグインのサーバーで、server_stop はどのスレッドから呼んでもよい (§5.3)
unsafe impl Send for RunningServer {}
unsafe impl Sync for RunningServer {}

impl RunningServer {
    /// server_start が返した実際の住所
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 止める。処理中の要求が終わるまで待つ
    pub fn stop(self) {
        drop(self)
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let h = std::mem::replace(&mut self.handle, std::ptr::null_mut());
        if !h.is_null() {
            let stop = self.entry.server_stop;
            let _ = catch_unwind(|| stop(h));
        }
    }
}

impl std::fmt::Debug for RunningServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningServer")
            .field("type_name", &self.entry.type_name())
            .field("name", &self.name)
            .finish()
    }
}

// ---- クライアント (§5.4) ----

/// client_request の失敗
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFailure {
    /// プラグインが返したコード
    pub code: i64,
    /// プラグインの文。HOST_REFUSED ならホストが断った理由
    pub message: String,
}

impl std::fmt::Display for CallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// プラグインのクライアント。落とすと client_close を呼ぶ
pub struct Client {
    entry: Arc<BrokerEntry>,
    handle: *mut c_void,
}

// SAFETY: プラグインは handle をスレッドをまたいで、同時に使えるように作る (§5.4)
unsafe impl Send for Client {}
unsafe impl Sync for Client {}

impl Client {
    pub fn entry(&self) -> &Arc<BrokerEntry> {
        &self.entry
    }

    /// 長さ len の要求を送り、応答を受け取る。write は要求をちょうど len バイト書き、
    /// receive は応答を読む (読めなければ文)。応答が来るまでブロックする
    pub fn request(
        &self,
        len: u64,
        mut write: impl FnMut(&mut PluginWriter<'_>) -> Result<(), String>,
        mut receive: impl FnMut(&mut PluginReader<'_>) -> Result<(), String>,
    ) -> Result<(), CallFailure> {
        let request_fn = self.entry.client_request;
        let handle = self.handle;
        let mut rctx = ReceiverCtx { receive: &mut receive, called: false, refused: None };
        let receiver = JuizFrameReceiver {
            ctx: &mut rctx as *mut ReceiverCtx<'_> as *mut c_void,
            receive: Some(receiver_receive),
        };
        let ((code, error), write_refused, _) = with_source(len, &mut write, |source| {
            collect_bytes(|err| guard_call(|| request_fn(handle, source, &receiver, err)))
        });
        if code < 0 {
            // HOST_REFUSED はホストが断った理由 (応答が読めない、要求を書けない) を文にする (§7.1)
            let refused = rctx.refused.take().or(write_refused);
            let message = match (code, error) {
                (JUIZ_BROKER_HOST_REFUSED, _) => refused.unwrap_or_else(|| message_of(code, None)),
                (_, Some(b)) if !b.is_empty() => String::from_utf8_lossy(&b).into_owned(),
                _ => message_of(code, None),
            };
            return Err(CallFailure { code, message });
        }
        if !rctx.called {
            return Err(CallFailure {
                code: JUIZ_BROKER_ERROR,
                message: "the broker plugin returned without a response".to_owned(),
            });
        }
        if let Some(m) = rctx.refused.take() {
            return Err(CallFailure { code: JUIZ_BROKER_HOST_REFUSED, message: m });
        }
        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let h = std::mem::replace(&mut self.handle, std::ptr::null_mut());
        if !h.is_null() {
            let close = self.entry.client_close;
            let _ = catch_unwind(|| close(h));
        }
    }
}

// ---- ループバックの確かめ (§8) ----

/// 受け取った要求をそのまま応答として返すサーバーの処理。juiz_core なしでプラグインを確かめるのに使う。
/// 長さのわからない要求は、上限 max_bytes まで読む
pub struct EchoHandler {
    pub max_bytes: u64,
}

impl Default for EchoHandler {
    fn default() -> Self {
        EchoHandler { max_bytes: 64 << 20 }
    }
}

impl ServerHandler for EchoHandler {
    fn handle(&self, _peer_addr: &str, request: &mut PluginReader<'_>, reply: PluginSender<'_>) -> i64 {
        let mut buf = Vec::new();
        if let Err(_e) = request.by_ref().take(self.max_bytes).read_to_end(&mut buf) {
            return JUIZ_BROKER_FRAME_ERROR;
        }
        match reply.send(buf.len() as u64, |w| w.write_all(&buf).map_err(|e| e.to_string())) {
            Ok(()) => JUIZ_OK,
            Err(_) => JUIZ_BROKER_SEND_FAILED,
        }
    }
}
