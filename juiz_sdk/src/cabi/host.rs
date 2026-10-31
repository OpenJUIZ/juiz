//! ホスト側から C ABI の入口を呼ぶための簡単な道具 (試験用)
//!
//! 引数のフレームを作り、戻り値の書き込み口を用意する。引数の本体は複製して持つので、
//! 仕様 (docs/spec/rust-plugin-abi.md §5.2) の「引数の画像を複製しない」は満たさない。
//! juiz_core の読み込み (Step 2、issue #330) はカプセルを直接指す形で作る。ここは試験と確認のためのもの。

use std::ffi::c_void;

use super::{JuizBytes, JuizBytesSink, JuizFrameIn, JuizFrameOut};
use crate::frame::{read_frame_parts, FrameHeader, FrameLimits, OutgoingFrame};
use crate::prelude::*;

/// 引数のフレーム。[`FrameArgs::frame_in`] が返すものは、これを落とすまで有効
pub struct FrameArgs {
    header: Vec<u8>,
    _bodies: Vec<Vec<u8>>,
    descs: Vec<JuizBytes>,
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
        })
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
            natives: std::ptr::null(),
            native_count: 0,
        }
    }
}

/// 戻り値の書き込み口。ヘッダを読んで本体ごとに領域を確保する
#[derive(Default)]
pub struct FrameResult {
    header: Option<Vec<u8>>,
    bodies: Vec<Vec<u8>>,
    error: Option<String>,
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

extern "C" fn result_error(ctx: *mut c_void, message: JuizBytes) {
    let r = unsafe { &mut *(ctx as *mut FrameResult) };
    let bytes = unsafe { message.as_slice() }.unwrap_or_default();
    r.error = Some(String::from_utf8_lossy(bytes).into_owned());
}

impl FrameResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// 書き込み口。self を動かさず、呼び出しの間生かしておくこと
    pub fn frame_out(&mut self) -> JuizFrameOut {
        JuizFrameOut {
            ctx: self as *mut FrameResult as *mut c_void,
            begin: Some(result_begin),
            body: Some(result_body),
            native: None,
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
