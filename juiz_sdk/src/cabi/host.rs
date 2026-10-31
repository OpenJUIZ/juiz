//! ホスト側から C ABI の入口を呼ぶための簡単な道具 (試験用)
//!
//! 引数のフレームを作り、戻り値の書き込み口を用意する。引数の本体は複製して持つので、
//! 仕様 (docs/spec/rust-plugin-abi.md §5.2) の「引数の画像を複製しない」は満たさない。
//! juiz_core の読み込み (Step 2、issue #330) はカプセルを直接指す形で作る。ここは試験と確認のためのもの。

use std::ffi::c_void;

use super::{JuizBytes, JuizBytesSink, JuizFrameIn, JuizFrameOut, JuizRustNative};
use crate::frame::{read_frame_parts, FrameHeader, FrameLimits, OutgoingFrame};
use crate::prelude::*;

/// 引数のフレーム。[`FrameArgs::frame_in`] が返すものは、これを落とすまで有効
pub struct FrameArgs {
    header: Vec<u8>,
    _bodies: Vec<Vec<u8>>,
    descs: Vec<JuizBytes>,
    natives: Vec<JuizRustNative>,
}

impl FrameArgs {
    pub fn new(map: &CapsuleMap) -> JuizResult<Self> {
        let frame = OutgoingFrame::call_request(map)?;
        let mut bodies = Vec::with_capacity(frame.body_count());
        for i in 0..frame.body_count() {
            let mut b = vec![0u8; frame.body_len(i).unwrap_or_default() as usize];
            frame.copy_body_into(i, &mut b)?;
            bodies.push(b);
        }
        let descs = bodies.iter().map(|b| JuizBytes::from_slice(b)).collect();
        Ok(FrameArgs {
            header: frame.header_bytes()?,
            _bodies: bodies,
            descs,
            natives: Vec::new(),
        })
    }

    /// map と、ハンドルのまま渡す Native (§10.3) で引数を作る。natives の名前はヘッダの
    /// `{"native": <番号>}` になる。owned などは呼ぶ側が決める
    pub fn with_natives(map: &CapsuleMap, natives: Vec<(&str, JuizRustNative)>) -> JuizResult<Self> {
        let mut frame = OutgoingFrame::call_request(map)?;
        if !natives.is_empty() {
            let header_map = frame.header_mut().map.get_or_insert_with(Default::default);
            for (i, (k, _)) in natives.iter().enumerate() {
                header_map.insert((*k).to_owned(), jvalue!({ "native": i }));
            }
        }
        let mut args = Self::new(&CapsuleMap::new())?;
        let mut bodies = Vec::with_capacity(frame.body_count());
        for i in 0..frame.body_count() {
            let mut b = vec![0u8; frame.body_len(i).unwrap_or_default() as usize];
            frame.copy_body_into(i, &mut b)?;
            bodies.push(b);
        }
        args.descs = bodies.iter().map(|b| JuizBytes::from_slice(b)).collect();
        args._bodies = bodies;
        args.header = frame.header_bytes()?;
        args.natives = natives.into_iter().map(|(_, n)| n).collect();
        Ok(args)
    }

    pub fn frame_in(&self) -> JuizFrameIn {
        JuizFrameIn {
            header: JuizBytes::from_slice(&self.header),
            bodies: if self.descs.is_empty() {
                std::ptr::null()
            } else {
                self.descs.as_ptr()
            },
            body_count: self.descs.len(),
            natives: if self.natives.is_empty() {
                std::ptr::null()
            } else {
                self.natives.as_ptr()
            },
            native_count: self.natives.len(),
        }
    }
}

/// 戻り値の書き込み口。ヘッダを読んで本体ごとに領域を確保する
#[derive(Default)]
pub struct FrameResult {
    header: Option<Vec<u8>>,
    bodies: Vec<Vec<u8>>,
    error: Option<String>,
    /// Native を受け取るか (§10.2)。受け取らなければ native の口を渡さない (プラグインは encode して書く)
    accept_natives: bool,
    /// 受け取った Native。type_tag は呼び出しの間だけ有効なので空にし、複製を native_type_tags に置く
    natives: Vec<JuizRustNative>,
    native_type_tags: Vec<Vec<u8>>,
}

extern "C" fn result_begin(ctx: *mut c_void, header: JuizBytes) -> i64 {
    let r = unsafe { &mut *(ctx as *mut FrameResult) };
    let Some(bytes) = (unsafe { header.as_slice() }) else {
        return -1;
    };
    let Ok(h) = serde_json::from_slice::<FrameHeader>(bytes) else {
        return -1;
    };
    let mut bodies = Vec::with_capacity(h.bodies.len());
    for d in h.bodies.iter() {
        match d.body_len() {
            Ok(len) if len <= crate::frame::DEFAULT_MAX_FRAME_BYTES => bodies.push(vec![0u8; len as usize]),
            _ => return -1,
        }
    }
    r.bodies = bodies;
    r.header = Some(bytes.to_vec());
    0
}

extern "C" fn result_body(ctx: *mut c_void, index: usize, ptr: *mut *mut u8, len: *mut usize) -> i64 {
    let r = unsafe { &mut *(ctx as *mut FrameResult) };
    let Some(b) = r.bodies.get_mut(index) else {
        return -1;
    };
    unsafe {
        *ptr = b.as_mut_ptr();
        *len = b.len();
    }
    0
}

extern "C" fn result_native(ctx: *mut c_void, index: usize, native: JuizRustNative) -> i64 {
    let r = unsafe { &mut *(ctx as *mut FrameResult) };
    if index != r.natives.len() {
        return -1;
    }
    r.native_type_tags
        .push(unsafe { native.type_tag.as_slice() }.unwrap_or_default().to_vec());
    r.natives.push(JuizRustNative {
        type_tag: JuizBytes::empty(),
        ..native
    });
    0
}

extern "C" fn result_error(ctx: *mut c_void, message: JuizBytes) {
    let r = unsafe { &mut *(ctx as *mut FrameResult) };
    let bytes = unsafe { message.as_slice() }.unwrap_or_default();
    r.error = Some(String::from_utf8_lossy(bytes).into_owned());
}

impl FrameResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Native をハンドルのまま受け取る書き込み口 (§10.2)。受け取った Native は [`FrameResult::natives`] で見る。
    /// 解放 (表の destroy) は呼ぶ側がする
    pub fn accepting_natives() -> Self {
        FrameResult {
            accept_natives: true,
            ..Self::default()
        }
    }

    /// 受け取った Native (type_tag は空。型タグは [`FrameResult::native_type_tags`])
    pub fn natives(&self) -> &[JuizRustNative] {
        &self.natives
    }

    /// 受け取った Native の型タグの JSON (複製)
    pub fn native_type_tags(&self) -> &[Vec<u8>] {
        &self.native_type_tags
    }

    /// 書かれたヘッダの JSON
    pub fn header_json(&self) -> Option<Value> {
        self.header.as_ref().and_then(|h| serde_json::from_slice(h).ok())
    }

    /// 書き込み口。self を動かさず、呼び出しの間生かしておくこと
    pub fn frame_out(&mut self) -> JuizFrameOut {
        JuizFrameOut {
            ctx: self as *mut FrameResult as *mut c_void,
            begin: Some(result_begin),
            body: Some(result_body),
            native: if self.accept_natives {
                Some(result_native)
            } else {
                None
            },
            error: Some(result_error),
        }
    }

    /// プラグインが error で渡した文
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 書かれた戻り値をカプセルにする。begin が呼ばれていなければ空
    pub fn into_capsule_ptr(self) -> JuizResult<CapsulePtr> {
        let Some(header) = self.header else {
            return Ok(CapsulePtr::new());
        };
        let bodies: Vec<&[u8]> = self.bodies.iter().map(|b| b.as_slice()).collect();
        read_frame_parts(&header, &bodies, &FrameLimits::default())?.into_capsule_ptr()
    }
}

extern "C" fn collect_write(ctx: *mut c_void, bytes: JuizBytes) {
    let dst = unsafe { &mut *(ctx as *mut Option<Vec<u8>>) };
    *dst = Some(unsafe { bytes.as_slice() }.unwrap_or_default().to_vec());
}

/// sink を用意して f を呼び、書かれたバイト列を返す (manifest・info・エラー文)
pub fn collect_bytes(f: impl FnOnce(*const JuizBytesSink) -> i64) -> (i64, Option<Vec<u8>>) {
    let mut got: Option<Vec<u8>> = None;
    let sink = JuizBytesSink {
        ctx: &mut got as *mut Option<Vec<u8>> as *mut c_void,
        write: Some(collect_write),
    };
    let code = f(&sink);
    (code, got)
}

/// Native の表の encode を呼び、共通形式のカプセルにする (§10.1)
pub fn encode_native(native: &JuizRustNative) -> JuizResult<CapsulePtr> {
    if native.vtable.is_null() {
        anyhow::bail!("the native value has no table");
    }
    let encode = unsafe { (*native.vtable).encode }.ok_or_else(|| anyhow::anyhow!("no encode"))?;
    let mut r = FrameResult::new();
    let out = r.frame_out();
    let code = encode(native.handle, &out);
    if code < 0 {
        anyhow::bail!("encode returned {code} ({})", r.error().unwrap_or(""));
    }
    r.into_capsule_ptr()
}

/// Native の表の destroy を呼ぶ (§10.1)。1 回だけ呼ぶこと
pub fn destroy_native(native: &JuizRustNative) {
    if native.vtable.is_null() {
        return;
    }
    if let Some(destroy) = unsafe { (*native.vtable).destroy } {
        destroy(native.handle);
    }
}
