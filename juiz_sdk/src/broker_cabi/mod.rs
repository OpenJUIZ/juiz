//! ブローカーのプラグインの C ABI (docs/spec/broker-plugin-abi.md、issue #636)
//!
//! ブローカーのプラグインは、JZF1 フレーム (docs/spec/large-data.md §3) のバイト列を運ぶことだけをする。
//! フレームを作る・読む、自分宛ての ID の書き換え、`CRUDBroker` への振り分けはホスト (juiz_core) がする。
//! ホストとプラグインの間で渡すのは `#[repr(C)]` の構造体と `extern "C"` の関数だけなので、
//! プラグインは juiz と違う rustc・違う juiz_sdk の版でビルドしてよい。
//!
//! このモジュールには次のものがある。
//!
//! - 境界の構造体 ([`JuizBrokerEntry`]・[`JuizFrameReader`]・[`JuizFrameWriter`] など、§3・§4) とエラーコード (§7.1)
//! - プラグイン側のグルー ([`BrokerPlugin`]・[`ServerHandle`]・[`ClientHandle`] と [`export_broker_plugin!`]、§8)。
//!   プラグインを書く人は trait を実装し、マクロ 1 つで入口 `broker_factory_cabi` を作る
//! - ホスト側の道具 ([`host`])。表を読み、C の構造体を Rust の `Read` / `Write` にする。juiz_core はこれを使い、
//!   フレームの処理だけを自分でする。試験用のループバックもここにある
//!
//! ```ignore
//! use juiz_sdk::broker_cabi::{BrokerPlugin, ServerHandle, ClientHandle, ServerHost,
//!                             FrameSource, FrameReceiver, BrokerError};
//!
//! struct MyBroker;
//! impl BrokerPlugin for MyBroker {
//!     const TYPE_NAME: &'static str = "mytcp";
//!     const DEFAULT_PORT: Option<u16> = Some(7500);
//!     type Server = MyServer;
//!     type Client = MyClient;
//!     fn start(manifest: serde_json::Value, host: ServerHost) -> Result<(MyServer, String), BrokerError> { … }
//!     fn connect(manifest: serde_json::Value) -> Result<MyClient, BrokerError> { … }
//! }
//! juiz_sdk::export_broker_plugin!(MyBroker);
//! ```

use std::any::Any;
use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub use crate::cabi::{JuizBytes, JuizBytesSink};

pub mod host;

#[cfg(test)]
mod tests;

/// この juiz_sdk が作る入口の表の、ブローカーの ABI の版 (§9)。Rust プラグインの ABI の版とは別に数える
pub const BROKER_ABI_VERSION: u32 = 1;

/// ファイルの中の印 (§9.3)。`juiz broker install`・`juiz pack` はライブラリのバイト列からこれを探す
pub const BROKER_ABI_MARKER: &str = "juiz_broker_abi=1;";
const BROKER_ABI_MARKER_C: &[u8] = b"juiz_broker_abi=1;\0";

/// 入口のシンボル名 (§3.1)
pub const ENTRY_SYMBOL: &str = "broker_factory_cabi";

// ---- エラーコード (§7.1) ----

pub const JUIZ_OK: i64 = 0;
/// プラグインの失敗 (待ち受け・接続・送信に失敗した、manifest が合わない)
pub const JUIZ_BROKER_ERROR: i64 = -1201;
/// 入口の中で panic した
pub const JUIZ_BROKER_PANIC: i64 = -1202;
/// フレームが読めない・書けない
pub const JUIZ_BROKER_FRAME_ERROR: i64 = -1203;
/// ホストの write_to・receive・handle が負の値を返した
pub const JUIZ_BROKER_HOST_REFUSED: i64 = -1204;
/// 必要なポインタが null だった
pub const JUIZ_BROKER_NULL_ARGUMENT: i64 = -1205;
/// 待ち時間のうちに応答が来なかった
pub const JUIZ_BROKER_TIMEOUT: i64 = -1206;
/// 相手につながらない・接続が切れた
pub const JUIZ_BROKER_UNAVAILABLE: i64 = -1207;
/// プラグインの write・send・read が負の値を返した (ホストが返す)
pub const JUIZ_BROKER_SEND_FAILED: i64 = -1208;

/// エラーコードの名前。知らないコードは "unknown"
pub fn error_code_name(code: i64) -> &'static str {
    match code {
        JUIZ_OK => "JUIZ_OK",
        JUIZ_BROKER_ERROR => "JUIZ_BROKER_ERROR",
        JUIZ_BROKER_PANIC => "JUIZ_BROKER_PANIC",
        JUIZ_BROKER_FRAME_ERROR => "JUIZ_BROKER_FRAME_ERROR",
        JUIZ_BROKER_HOST_REFUSED => "JUIZ_BROKER_HOST_REFUSED",
        JUIZ_BROKER_NULL_ARGUMENT => "JUIZ_BROKER_NULL_ARGUMENT",
        JUIZ_BROKER_TIMEOUT => "JUIZ_BROKER_TIMEOUT",
        JUIZ_BROKER_UNAVAILABLE => "JUIZ_BROKER_UNAVAILABLE",
        JUIZ_BROKER_SEND_FAILED => "JUIZ_BROKER_SEND_FAILED",
        _ => "unknown",
    }
}

// ---- 構造体 (§3.4・§4) ----

/// init でホストが渡すサービス (§3.4)。プロセスが終わるまで有効
#[repr(C)]
pub struct JuizBrokerHostServices {
    pub abi_version: u32,
    pub struct_size: u32,
    pub ctx: *mut c_void,
    /// level: 1 error / 2 warn / 3 info / 4 debug / 5 trace。target・message は UTF-8
    pub log: Option<extern "C" fn(ctx: *mut c_void, level: u32, target: JuizBytes, message: JuizBytes)>,
    /// この値より大きい level は送らなくてよい
    pub max_level: u32,
}

/// 読み口。プラグインが用意し、ホストがフレームを引き出す
#[repr(C)]
pub struct JuizFrameReader {
    pub ctx: *mut c_void,
    /// フレームの長さ (バイト)。わからなければ u64::MAX
    pub len: u64,
    /// buf に最大 cap バイトを書き、書いた数を *n に入れる。*n == 0 はフレームの終わり
    pub read: Option<extern "C" fn(ctx: *mut c_void, buf: *mut u8, cap: usize, n: *mut usize) -> i64>,
}

/// 書き口。プラグインが用意し、ホストがフレームを書き込む
#[repr(C)]
pub struct JuizFrameWriter {
    pub ctx: *mut c_void,
    /// bytes を全部書く。足りなければ負の値
    pub write: Option<extern "C" fn(ctx: *mut c_void, bytes: JuizBytes) -> i64>,
}

/// 送るフレーム。ホストが用意し、プラグインが書き口を渡して書かせる
#[repr(C)]
pub struct JuizFrameSource {
    pub ctx: *mut c_void,
    /// フレームの長さ (バイト)。必ずわかる
    pub len: u64,
    /// フレームを writer に書く。書き終えるまで戻らない。1 回だけ呼んでよい
    pub write_to: Option<extern "C" fn(ctx: *mut c_void, writer: *const JuizFrameWriter) -> i64>,
}

/// 受け取ったフレームの渡し先。ホストが用意し、プラグインが読み口を渡す
#[repr(C)]
pub struct JuizFrameReceiver {
    pub ctx: *mut c_void,
    /// reader からフレームを読み終えるまで戻らない。1 回だけ呼んでよい
    pub receive: Option<extern "C" fn(ctx: *mut c_void, reader: *const JuizFrameReader) -> i64>,
}

/// サーバーの応答の送り先。プラグインが用意し、ホストが送るフレームを渡す
#[repr(C)]
pub struct JuizFrameSender {
    pub ctx: *mut c_void,
    /// frame を送る。プラグインは frame->write_to を中で呼ぶ。1 回だけ呼ぶ
    pub send: Option<extern "C" fn(ctx: *mut c_void, frame: *const JuizFrameSource) -> i64>,
}

/// サーバーにホストが渡す表 (§5.3)。server_stop が戻るまで有効
#[repr(C)]
pub struct JuizBrokerServerHost {
    pub abi_version: u32,
    pub struct_size: u32,
    pub ctx: *mut c_void,
    /// 要求を 1 つ処理する。request から要求を読み、reply に応答を送ってから戻る
    pub handle: Option<
        extern "C" fn(
            ctx: *mut c_void,
            peer_addr: JuizBytes,
            request: *const JuizFrameReader,
            reply: *const JuizFrameSender,
        ) -> i64,
    >,
}

/// info: プラグインの情報の JSON を out に書く (§3.3)
pub type JuizBrokerInfoFn = extern "C" fn(out: *const JuizBytesSink) -> i64;
/// init: 読み込んだ直後に 1 回だけ呼ぶ (§3.4)
pub type JuizBrokerInitFn =
    extern "C" fn(services: *const JuizBrokerHostServices, error: *const JuizBytesSink) -> i64;
/// server_start (§5.3)
pub type JuizBrokerServerStartFn = extern "C" fn(
    manifest_json: JuizBytes,
    host: *const JuizBrokerServerHost,
    handle: *mut *mut c_void,
    name: *const JuizBytesSink,
    error: *const JuizBytesSink,
) -> i64;
/// server_stop (§5.3)
pub type JuizBrokerServerStopFn = extern "C" fn(handle: *mut c_void);
/// client_connect (§5.4)
pub type JuizBrokerClientConnectFn = extern "C" fn(
    manifest_json: JuizBytes,
    handle: *mut *mut c_void,
    error: *const JuizBytesSink,
) -> i64;
/// client_request (§5.4)
pub type JuizBrokerClientRequestFn = extern "C" fn(
    handle: *mut c_void,
    request: *const JuizFrameSource,
    response: *const JuizFrameReceiver,
    error: *const JuizBytesSink,
) -> i64;
/// client_close (§5.4)
pub type JuizBrokerClientCloseFn = extern "C" fn(handle: *mut c_void);

/// 入口の表 (§3.2)。プラグイン側が static に置き、`broker_factory_cabi()` がそのポインタを返す。
/// 並びと型を変えるときは ABI の版を上げる (§9.1)
#[repr(C)]
#[derive(Debug)]
pub struct JuizBrokerEntry {
    /// この表の ABI の版。今は 1
    pub abi_version: u32,
    /// sizeof(JuizBrokerEntry)。後ろに項目を足すときに使う
    pub struct_size: u32,
    /// "juiz_broker_abi=1;" (§9.3)
    pub abi_marker: *const c_char,
    pub info: Option<JuizBrokerInfoFn>,
    pub init: Option<JuizBrokerInitFn>,
    pub server_start: Option<JuizBrokerServerStartFn>,
    pub server_stop: Option<JuizBrokerServerStopFn>,
    pub client_connect: Option<JuizBrokerClientConnectFn>,
    pub client_request: Option<JuizBrokerClientRequestFn>,
    pub client_close: Option<JuizBrokerClientCloseFn>,
}

// SAFETY: 表は static に置く読むだけのもので、abi_marker は静的な C 文字列を指す
unsafe impl Sync for JuizBrokerEntry {}
unsafe impl Send for JuizBrokerEntry {}

impl JuizBrokerEntry {
    /// すべての関数を持つ表。export_broker_plugin! が使う
    pub const fn new(
        info: JuizBrokerInfoFn,
        init: JuizBrokerInitFn,
        server_start: JuizBrokerServerStartFn,
        server_stop: JuizBrokerServerStopFn,
        client_connect: JuizBrokerClientConnectFn,
        client_request: JuizBrokerClientRequestFn,
        client_close: JuizBrokerClientCloseFn,
    ) -> Self {
        JuizBrokerEntry {
            abi_version: BROKER_ABI_VERSION,
            struct_size: std::mem::size_of::<JuizBrokerEntry>() as u32,
            abi_marker: BROKER_ABI_MARKER_C.as_ptr() as *const c_char,
            info: Some(info),
            init: Some(init),
            server_start: Some(server_start),
            server_stop: Some(server_stop),
            client_connect: Some(client_connect),
            client_request: Some(client_request),
            client_close: Some(client_close),
        }
    }
}

/// 入口の関数 `broker_factory_cabi` の型 (§3.1)
pub type JuizBrokerEntryFn = extern "C" fn() -> *const JuizBrokerEntry;

// ---- エラー ----

/// プラグインの失敗。エラーコード (§7.1) と文を持つ
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerError {
    code: i64,
    message: String,
}

impl BrokerError {
    /// コードと文から作る。コードが負でなければ JUIZ_BROKER_ERROR にする
    pub fn with_code(code: i64, message: impl Into<String>) -> Self {
        BrokerError {
            code: if code < 0 { code } else { JUIZ_BROKER_ERROR },
            message: message.into(),
        }
    }

    /// JUIZ_BROKER_ERROR (待ち受け・接続・送信に失敗した、manifest が合わない)
    pub fn error(message: impl Into<String>) -> Self {
        Self::with_code(JUIZ_BROKER_ERROR, message)
    }

    /// JUIZ_BROKER_TIMEOUT。文には待った時間を入れる (ホストはそのまま出す。例 `no response within 300 ms`)
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::with_code(JUIZ_BROKER_TIMEOUT, message)
    }

    /// JUIZ_BROKER_UNAVAILABLE。文には相手の住所と理由を入れる
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::with_code(JUIZ_BROKER_UNAVAILABLE, message)
    }

    /// JUIZ_BROKER_FRAME_ERROR
    pub fn frame(message: impl Into<String>) -> Self {
        Self::with_code(JUIZ_BROKER_FRAME_ERROR, message)
    }

    pub fn code(&self) -> i64 {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// 文の前に context を付ける (`<context>. <元の文>`)
    pub fn context(mut self, context: impl std::fmt::Display) -> Self {
        self.message = if self.message.is_empty() {
            context.to_string()
        } else {
            format!("{context}. {}", self.message)
        };
        self
    }
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            write!(f, "{} ({})", error_code_name(self.code), self.code)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for BrokerError {}

impl From<io::Error> for BrokerError {
    /// 接続が切れた・つながらないは JUIZ_BROKER_UNAVAILABLE、待ち時間切れは JUIZ_BROKER_TIMEOUT、ほかは JUIZ_BROKER_ERROR
    fn from(e: io::Error) -> Self {
        use io::ErrorKind::*;
        let code = match e.kind() {
            ConnectionRefused | ConnectionReset | ConnectionAborted | NotConnected | BrokenPipe
            | UnexpectedEof | AddrNotAvailable => JUIZ_BROKER_UNAVAILABLE,
            TimedOut | WouldBlock => JUIZ_BROKER_TIMEOUT,
            _ => JUIZ_BROKER_ERROR,
        };
        BrokerError::with_code(code, e.to_string())
    }
}

// ---- プラグイン側: ホストの構造体を Rust の値にしたもの (§8) ----

/// io のエラーを持ち運ぶ。C の関数をまたぐので、元のエラーをここに置いておく
#[derive(Default)]
struct IoSlot(Option<io::Error>);

/// 送るフレーム (ホストが用意したもの)。[`FrameSource::write_to`] で書き口に書かせる
pub struct FrameSource<'a> {
    raw: &'a JuizFrameSource,
}

impl<'a> FrameSource<'a> {
    /// # Safety
    /// raw はこの値を使い終わるまで有効でなければならない
    pub unsafe fn from_raw(raw: *const JuizFrameSource) -> Option<Self> {
        raw.as_ref().map(|raw| FrameSource { raw })
    }

    /// フレームの長さ (バイト)。write_to はちょうどこれだけ書く
    pub fn len(&self) -> u64 {
        self.raw.len
    }

    pub fn is_empty(&self) -> bool {
        self.raw.len == 0
    }

    /// フレームを w に書く。共有メモリへは、確保した領域を指す `&mut [u8]` を渡せばよい
    pub fn write_to<W: Write>(self, w: &mut W) -> Result<(), BrokerError> {
        let write_to = self
            .raw
            .write_to
            .ok_or_else(|| BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, "write_to is null"))?;
        let mut ctx = WriterCtx { w, error: IoSlot::default() };
        let writer = JuizFrameWriter {
            ctx: &mut ctx as *mut WriterCtx<W> as *mut c_void,
            write: Some(writer_write::<W>),
        };
        let code = write_to(self.raw.ctx, &writer);
        if let Some(e) = ctx.error.0.take() {
            return Err(BrokerError::from(e).context("writing the frame failed"));
        }
        if code < 0 {
            return Err(BrokerError::with_code(JUIZ_BROKER_HOST_REFUSED, ""));
        }
        Ok(())
    }
}

struct WriterCtx<'w, W: Write> {
    w: &'w mut W,
    error: IoSlot,
}

extern "C" fn writer_write<W: Write>(ctx: *mut c_void, bytes: JuizBytes) -> i64 {
    // SAFETY: ctx は FrameSource::write_to の WriterCtx<W> を指し、write_to の間だけ呼ばれる
    let ctx = unsafe { &mut *(ctx as *mut WriterCtx<W>) };
    let r = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: ホストは bytes を呼び出しの間有効に保つ
        let Some(b) = (unsafe { bytes.as_slice() }) else {
            return Err(io::Error::other("the host passed null bytes"));
        };
        ctx.w.write_all(b)
    }));
    match r {
        Ok(Ok(())) => JUIZ_OK,
        Ok(Err(e)) => {
            ctx.error.0 = Some(e);
            JUIZ_BROKER_SEND_FAILED
        }
        Err(p) => {
            ctx.error.0 = Some(io::Error::other(panic_message(p.as_ref())));
            JUIZ_BROKER_PANIC
        }
    }
}

/// 受け取ったフレームの渡し先 (ホストが用意したもの)。[`FrameReceiver::receive`] で読み口を渡す
pub struct FrameReceiver<'a> {
    raw: &'a JuizFrameReceiver,
}

impl<'a> FrameReceiver<'a> {
    /// # Safety
    /// raw はこの値を使い終わるまで有効でなければならない
    pub unsafe fn from_raw(raw: *const JuizFrameReceiver) -> Option<Self> {
        raw.as_ref().map(|raw| FrameReceiver { raw })
    }

    /// reader からフレームを読ませる。len はフレームの長さ (わからなければ None)。
    /// len があれば、ホストはそれより多くは読まない。reader はフレームの終わりで 0 を返さなければならない
    pub fn receive<R: Read>(self, reader: &mut R, len: Option<u64>) -> Result<(), BrokerError> {
        let receive = self
            .raw
            .receive
            .ok_or_else(|| BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, "receive is null"))?;
        with_reader(reader, len, |r| receive(self.raw.ctx, r))
    }
}

struct ReaderCtx<'r, R: Read> {
    r: &'r mut R,
    remaining: Option<u64>,
    error: IoSlot,
}

/// reader を読み口にして f に渡す。f の戻り値 (ホストのコード) を結果にする
fn with_reader<R: Read>(
    reader: &mut R,
    len: Option<u64>,
    f: impl FnOnce(*const JuizFrameReader) -> i64,
) -> Result<(), BrokerError> {
    let mut ctx = ReaderCtx { r: reader, remaining: len, error: IoSlot::default() };
    let raw = JuizFrameReader {
        ctx: &mut ctx as *mut ReaderCtx<R> as *mut c_void,
        len: len.unwrap_or(u64::MAX),
        read: Some(reader_read::<R>),
    };
    let code = f(&raw);
    if let Some(e) = ctx.error.0.take() {
        return Err(BrokerError::from(e).context("reading the frame failed"));
    }
    if code < 0 {
        return Err(BrokerError::with_code(JUIZ_BROKER_HOST_REFUSED, ""));
    }
    Ok(())
}

extern "C" fn reader_read<R: Read>(ctx: *mut c_void, buf: *mut u8, cap: usize, n: *mut usize) -> i64 {
    // SAFETY: ctx は with_reader の ReaderCtx<R> を指し、その間だけ呼ばれる
    let ctx = unsafe { &mut *(ctx as *mut ReaderCtx<R>) };
    if n.is_null() || (buf.is_null() && cap > 0) {
        return JUIZ_BROKER_NULL_ARGUMENT;
    }
    let r = catch_unwind(AssertUnwindSafe(|| -> io::Result<usize> {
        let cap = match ctx.remaining {
            Some(rest) => cap.min(usize::try_from(rest).unwrap_or(usize::MAX)),
            None => cap,
        };
        if cap == 0 {
            return Ok(0);
        }
        // SAFETY: ホストは buf に cap バイトの書ける領域を渡す
        let dst = unsafe { std::slice::from_raw_parts_mut(buf, cap) };
        let got = loop {
            match ctx.r.read(dst) {
                Ok(got) => break got,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        };
        if let Some(rest) = ctx.remaining.as_mut() {
            if got == 0 && *rest > 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("the frame ended {rest} bytes early"),
                ));
            }
            *rest -= got as u64;
        }
        Ok(got)
    }));
    match r {
        Ok(Ok(got)) => {
            // SAFETY: n は null でないことを確かめた
            unsafe { *n = got };
            JUIZ_OK
        }
        Ok(Err(e)) => {
            ctx.error.0 = Some(e);
            JUIZ_BROKER_SEND_FAILED
        }
        Err(p) => {
            ctx.error.0 = Some(io::Error::other(panic_message(p.as_ref())));
            JUIZ_BROKER_PANIC
        }
    }
}

/// サーバーにホストが渡した表。`Clone + Send + Sync` で、server_stop が戻るまで使える
#[derive(Clone, Copy)]
pub struct ServerHost {
    raw: *const JuizBrokerServerHost,
}

// SAFETY: ホストは handle を同時にいくつ呼んでもよいように作る (§5.3)。表は server_stop まで有効
unsafe impl Send for ServerHost {}
unsafe impl Sync for ServerHost {}

impl ServerHost {
    /// # Safety
    /// raw は server_stop が戻るまで有効でなければならない
    pub unsafe fn from_raw(raw: *const JuizBrokerServerHost) -> Result<Self, BrokerError> {
        let Some(h) = raw.as_ref() else {
            return Err(BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, "host is null"));
        };
        if h.abi_version < 1 || (h.struct_size as usize) < std::mem::size_of::<JuizBrokerServerHost>() {
            return Err(BrokerError::error(format!(
                "the host table (ABI {}, {} bytes) is not supported",
                h.abi_version, h.struct_size
            )));
        }
        if h.handle.is_none() {
            return Err(BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, "host.handle is null"));
        }
        Ok(ServerHost { raw })
    }

    /// 要求を 1 つホストに処理させる。reader から要求を読ませ (len はフレームの長さ、わからなければ None)、
    /// 応答は sender に渡す。sender はホストが中でちょうど 1 回呼ぶ。
    /// 戻るのは Process の実行が終わってからなので、ブロックしてよいスレッドから呼ぶこと。
    ///
    /// エラーのコード: `JUIZ_BROKER_FRAME_ERROR` (要求が読めなかった。ホストはエラー応答を送ろうとした)、
    /// `JUIZ_BROKER_SEND_FAILED` (sender が失敗した。その文が入る)
    pub fn handle<R, S>(&self, peer_addr: &str, reader: &mut R, len: Option<u64>, sender: S) -> Result<(), BrokerError>
    where
        R: Read,
        S: FnOnce(FrameSource<'_>) -> Result<(), BrokerError>,
    {
        // SAFETY: from_raw で確かめた。server_stop まで有効
        let h = unsafe { &*self.raw };
        let handle = h.handle.expect("checked in from_raw");
        let mut sctx = SenderCtx { sender: Some(sender), error: None };
        let raw_sender = JuizFrameSender {
            ctx: &mut sctx as *mut SenderCtx<S> as *mut c_void,
            send: Some(sender_send::<S>),
        };
        let peer = JuizBytes::from_slice(peer_addr.as_bytes());
        let mut code = JUIZ_OK;
        let read = with_reader(reader, len, |r| {
            code = handle(h.ctx, peer, r, &raw_sender);
            code
        });
        if let Some(e) = sctx.error.take() {
            return Err(BrokerError::with_code(JUIZ_BROKER_SEND_FAILED, e.message).context("sending the response failed"));
        }
        match read {
            // 読み口の失敗 (通り道の失敗) は、そのエラーを返す
            Err(e) if e.code() != JUIZ_BROKER_HOST_REFUSED => Err(e),
            _ if code == JUIZ_BROKER_FRAME_ERROR => Err(BrokerError::frame("the host could not read the request frame")),
            _ if code < 0 => Err(BrokerError::with_code(code, format!("the host returned {code} ({})", error_code_name(code)))),
            _ => Ok(()),
        }
    }
}

struct SenderCtx<S> {
    sender: Option<S>,
    error: Option<BrokerError>,
}

extern "C" fn sender_send<S>(ctx: *mut c_void, frame: *const JuizFrameSource) -> i64
where
    S: FnOnce(FrameSource<'_>) -> Result<(), BrokerError>,
{
    // SAFETY: ctx は ServerHost::handle の SenderCtx<S> を指し、handle の間だけ呼ばれる
    let ctx = unsafe { &mut *(ctx as *mut SenderCtx<S>) };
    let Some(sender) = ctx.sender.take() else {
        ctx.error = Some(BrokerError::error("send was called twice"));
        return JUIZ_BROKER_ERROR;
    };
    // SAFETY: ホストは frame を呼び出しの間有効に保つ
    let Some(source) = (unsafe { FrameSource::from_raw(frame) }) else {
        return JUIZ_BROKER_NULL_ARGUMENT;
    };
    match catch_unwind(AssertUnwindSafe(|| sender(source))) {
        Ok(Ok(())) => JUIZ_OK,
        Ok(Err(e)) => {
            let code = e.code();
            ctx.error = Some(e);
            code
        }
        Err(p) => {
            ctx.error = Some(BrokerError::with_code(JUIZ_BROKER_PANIC, panic_message(p.as_ref())));
            JUIZ_BROKER_PANIC
        }
    }
}

// ---- プラグインを書く人が実装する trait (§8) ----

/// ブローカーの種類。[`export_broker_plugin!`] に渡す
pub trait BrokerPlugin: 'static {
    /// ブローカーの種類名。conf の `type_name` と ID の頭 (`<TYPE_NAME>://…`) になる (§3.3)
    const TYPE_NAME: &'static str;
    /// conf にポートを書かないときのポート
    const DEFAULT_PORT: Option<u16> = None;
    type Server: ServerHandle;
    type Client: ClientHandle;

    /// 待ち受けを始めて、実際の住所 (`<host>:<port>`、IPv6 は `[<addr>]:<port>`、0.0.0.0 は 127.0.0.1) と一緒に返す。
    /// 待ち受けに失敗したらエラーを返す (log に出して終わりにしない、§5.3)
    fn start(manifest: serde_json::Value, host: ServerHost) -> Result<(Self::Server, String), BrokerError>;

    /// クライアントを作る。接続を開くのは最初の request まで遅らせてよい (§5.4)
    fn connect(manifest: serde_json::Value) -> Result<Self::Client, BrokerError>;
}

/// 待ち受け中のサーバー
pub trait ServerHandle: Send + Sync + 'static {
    /// 受け取りをやめ、処理中の `host.handle(…)` がすべて戻るまで待ってから戻る
    fn stop(self);
}

/// クライアント。同じ値で同時にいくつ request を呼んでもよいように作る (§5.4)
pub trait ClientHandle: Send + Sync + 'static {
    /// request を送り、応答が来たら response.receive を 1 回呼んでから戻る。
    /// 待ち時間切れは [`BrokerError::timeout`]、つながらないは [`BrokerError::unavailable`]
    fn request(&self, request: FrameSource<'_>, response: FrameReceiver<'_>) -> Result<(), BrokerError>;
}

// ---- グルー (§8)。export_broker_plugin! が生成する入口から呼ぶ ----

/// panic の payload を文にする (rust-plugin-abi.md §6.1 と同じ形)
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

/// sink に 1 回書く。sink か write が null なら何もしない
fn write_sink(sink: *const JuizBytesSink, bytes: &[u8]) {
    // SAFETY: ホストは sink を呼び出しの間有効に保つ
    let Some(sink) = (unsafe { sink.as_ref() }) else {
        return;
    };
    if let Some(write) = sink.write {
        write(sink.ctx, JuizBytes::from_slice(bytes));
    }
}

/// 入口の中身を catch_unwind で囲み、失敗をコードにして文を error に書く (§7.1・§7.2)
fn guarded(error: *const JuizBytesSink, body: impl FnOnce() -> Result<(), BrokerError>) -> i64 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => JUIZ_OK,
        Ok(Err(e)) => {
            if e.code() != JUIZ_BROKER_HOST_REFUSED && !e.message().is_empty() {
                write_sink(error, e.message().as_bytes());
            }
            e.code()
        }
        Err(payload) => {
            let m = panic_message(payload.as_ref());
            // payload の drop が panic しても外へ出さない
            let _ = catch_unwind(AssertUnwindSafe(move || drop(payload)));
            write_sink(error, m.as_bytes());
            JUIZ_BROKER_PANIC
        }
    }
}

fn read_manifest(manifest_json: JuizBytes) -> Result<serde_json::Value, BrokerError> {
    // SAFETY: ホストは manifest_json を呼び出しの間有効に保つ
    let bytes = unsafe { manifest_json.as_slice() }
        .ok_or_else(|| BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, ""))?;
    serde_json::from_slice(bytes).map_err(|e| BrokerError::error(format!("the manifest is not valid JSON ({e})")))
}

/// export_broker_plugin! の中身。プラグインを書く人は直接呼ばない
#[doc(hidden)]
pub mod glue {
    use super::*;

    /// info の JSON (§3.3)
    pub fn info<T: BrokerPlugin>(out: *const JuizBytesSink, plugin_name: &str, plugin_version: &str) -> i64 {
        guarded(out, || {
            let mut v = serde_json::json!({
                "type_name": T::TYPE_NAME,
                "abi_version": BROKER_ABI_VERSION,
                "plugin_name": plugin_name,
                "plugin_version": plugin_version,
                "build_info": crate::build_info::build_info_str(),
            });
            if let Some(port) = T::DEFAULT_PORT {
                v["default_port"] = serde_json::json!(port);
            }
            let bytes = serde_json::to_vec(&v).map_err(|e| BrokerError::error(e.to_string()))?;
            write_sink(out, &bytes);
            Ok(())
        })
    }

    /// ホストのログの口を log クレートのロガーとして据える (§3.4)。すでに据えてあれば何もしない
    pub extern "C" fn init(services: *const JuizBrokerHostServices, error: *const JuizBytesSink) -> i64 {
        guarded(error, || {
            // SAFETY: ホストは services をプロセスが終わるまで有効に保つ
            let Some(s) = (unsafe { services.as_ref() }) else {
                return Err(BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, ""));
            };
            if s.abi_version < 1 || (s.struct_size as usize) < std::mem::size_of::<JuizBrokerHostServices>() {
                return Err(BrokerError::error(format!(
                    "the host services (ABI {}, {} bytes) are not supported",
                    s.abi_version, s.struct_size
                )));
            }
            if s.log.is_some() {
                let max = level_filter(s.max_level);
                if log::set_boxed_logger(Box::new(HostLogger { services: ServicesPtr(services), max })).is_ok() {
                    log::set_max_level(max);
                }
            }
            Ok(())
        })
    }

    pub extern "C" fn server_start<T: BrokerPlugin>(
        manifest_json: JuizBytes,
        host: *const JuizBrokerServerHost,
        handle: *mut *mut c_void,
        name: *const JuizBytesSink,
        error: *const JuizBytesSink,
    ) -> i64 {
        guarded(error, || {
            if handle.is_null() {
                return Err(BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, ""));
            }
            let manifest = read_manifest(manifest_json)?;
            // SAFETY: ホストは host を server_stop が戻るまで有効に保つ (§4)
            let host = unsafe { ServerHost::from_raw(host)? };
            let (server, actual_name) = T::start(manifest, host)?;
            write_sink(name, actual_name.as_bytes());
            // SAFETY: handle は null でないことを確かめた
            unsafe { *handle = Box::into_raw(Box::new(server)) as *mut c_void };
            Ok(())
        })
    }

    pub extern "C" fn server_stop<T: BrokerPlugin>(handle: *mut c_void) {
        if handle.is_null() {
            return;
        }
        let r = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: handle は server_start が Box::into_raw で作ったもので、ホストは 1 回だけ渡す
            let server = unsafe { Box::from_raw(handle as *mut T::Server) };
            server.stop();
        }));
        if let Err(p) = r {
            let m = panic_message(p.as_ref());
            let _ = catch_unwind(AssertUnwindSafe(move || drop(p)));
            let _ = catch_unwind(|| log::error!("{}: stopping the server {m}", T::TYPE_NAME));
        }
    }

    pub extern "C" fn client_connect<T: BrokerPlugin>(
        manifest_json: JuizBytes,
        handle: *mut *mut c_void,
        error: *const JuizBytesSink,
    ) -> i64 {
        guarded(error, || {
            if handle.is_null() {
                return Err(BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, ""));
            }
            let manifest = read_manifest(manifest_json)?;
            let client = T::connect(manifest)?;
            // SAFETY: handle は null でないことを確かめた
            unsafe { *handle = Box::into_raw(Box::new(client)) as *mut c_void };
            Ok(())
        })
    }

    pub extern "C" fn client_request<T: BrokerPlugin>(
        handle: *mut c_void,
        request: *const JuizFrameSource,
        response: *const JuizFrameReceiver,
        error: *const JuizBytesSink,
    ) -> i64 {
        guarded(error, || {
            let null = || BrokerError::with_code(JUIZ_BROKER_NULL_ARGUMENT, "");
            // SAFETY: handle は client_connect が作ったもので、client_close まで有効。& で使うので並行に呼べる
            let client = unsafe { (handle as *const T::Client).as_ref() }.ok_or_else(null)?;
            // SAFETY: ホストは request と response を呼び出しの間有効に保つ
            let request = unsafe { FrameSource::from_raw(request) }.ok_or_else(null)?;
            let response = unsafe { FrameReceiver::from_raw(response) }.ok_or_else(null)?;
            client.request(request, response)
        })
    }

    pub extern "C" fn client_close<T: BrokerPlugin>(handle: *mut c_void) {
        if handle.is_null() {
            return;
        }
        let r = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: handle は client_connect が作ったもので、ホストは 1 回だけ渡す
            drop(unsafe { Box::from_raw(handle as *mut T::Client) });
        }));
        if let Err(p) = r {
            let m = panic_message(p.as_ref());
            let _ = catch_unwind(AssertUnwindSafe(move || drop(p)));
            let _ = catch_unwind(|| log::error!("{}: closing the client {m}", T::TYPE_NAME));
        }
    }
}

// ---- ホストのログの口 (§3.4) ----

#[derive(Clone, Copy)]
struct ServicesPtr(*const JuizBrokerHostServices);
// SAFETY: ホストは services をプロセスが終わるまで有効に保ち、log をどのスレッドから呼んでもよいように作る
unsafe impl Send for ServicesPtr {}
unsafe impl Sync for ServicesPtr {}

struct HostLogger {
    services: ServicesPtr,
    max: log::LevelFilter,
}

fn level_filter(max_level: u32) -> log::LevelFilter {
    match max_level {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    }
}

thread_local! {
    /// ログを送っている最中か。ホストとプラグインが同じ log クレートを使う (プラグインを組み込んだ) とき、
    /// ホストのロガーがまたこのロガーを呼んで回り続けないようにする
    static LOGGING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl log::Log for HostLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.max
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) || LOGGING.with(|l| l.replace(true)) {
            return;
        }
        // SAFETY: ServicesPtr を参照
        let s = unsafe { &*self.services.0 };
        if let Some(log) = s.log {
            let message = record.args().to_string();
            log(
                s.ctx,
                record.level() as u32,
                JuizBytes::from_slice(record.target().as_bytes()),
                JuizBytes::from_slice(message.as_bytes()),
            );
        }
        LOGGING.with(|l| l.set(false));
    }

    fn flush(&self) {}
}

/// 入口の表を返す関数を作る。引数は [`BrokerPlugin`] を実装した型と、作る関数の名前 (`pub fn <名前>` のように書く)。
/// [`export_broker_plugin!`] と違って `#[no_mangle]` の `broker_factory_cabi` は置かないので、
/// 組み込み (rlib として juiz にリンクし、`register_builtin_cabi_broker(<名前>)` で登録する) だけで使うとき、
/// ほかのブローカーの入口とリンクでぶつからない
#[macro_export]
macro_rules! define_broker_entry {
    ($t:ty, $vis:vis fn $name:ident) => {
        /// ブローカーの C ABI の入口の表を返す (docs/spec/broker-plugin-abi.md §3.1)
        $vis extern "C" fn $name() -> *const $crate::broker_cabi::JuizBrokerEntry {
            extern "C" fn __juiz_broker_info(out: *const $crate::cabi::JuizBytesSink) -> i64 {
                $crate::broker_cabi::glue::info::<$t>(out, env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
            }

            static __JUIZ_BROKER_ENTRY: $crate::broker_cabi::JuizBrokerEntry = $crate::broker_cabi::JuizBrokerEntry::new(
                __juiz_broker_info,
                $crate::broker_cabi::glue::init,
                $crate::broker_cabi::glue::server_start::<$t>,
                $crate::broker_cabi::glue::server_stop::<$t>,
                $crate::broker_cabi::glue::client_connect::<$t>,
                $crate::broker_cabi::glue::client_request::<$t>,
                $crate::broker_cabi::glue::client_close::<$t>,
            );
            &__JUIZ_BROKER_ENTRY
        }
    };
}

/// ブローカーのプラグインの入口 `broker_factory_cabi` (§3.1) と、入口の表・info・init を作る。
/// 引数は [`BrokerPlugin`] を実装した型。1 つの cdylib に 1 回だけ書く
#[macro_export]
macro_rules! export_broker_plugin {
    ($t:ty) => {
        $crate::define_broker_entry!($t, fn __juiz_broker_entry);

        /// ブローカーの C ABI の入口 (docs/spec/broker-plugin-abi.md §3.1)
        #[unsafe(no_mangle)]
        pub extern "C" fn broker_factory_cabi() -> *const $crate::broker_cabi::JuizBrokerEntry {
            __juiz_broker_entry()
        }
    };
}
