//! 大きなデータを生バイトのまま送るフレーム形式 (方式 A、docs/spec/large-data.md §3)
//!
//! 1 回の要求または応答を、JSON のヘッダと 0 個以上の本体からなる 1 つのバイト列にする。
//! 画像は PNG にせず画素をそのまま、要素の多い数の配列は JSON の数値の列にせず生バイトで送る。
//! IPC ブローカー (#233) が使い、qmp ブローカー (#234) も同じ形式を使う。
//!
//! ```text
//! "JZF1" | u32 header_len | header (UTF-8 JSON) | u64 body_len0 | body0 | u64 body_len1 | body1 | ...
//! ```
//!
//! 数はすべてリトルエンディアン。送り手は画像の画素を別のバッファに複製せずに書き、
//! 受け手は本体を最終的な `DynamicImage` / `Vec` の領域へ直接読み込む (§3.6)。

use std::collections::HashMap;
use std::io::{self, Read, Write};

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::image::ImageBuffer;
use juiz_sdk::serde_json::{self, Map, Number};
use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// フレームの先頭の 4 バイト。形式とその版を表す
pub const MAGIC: [u8; 4] = *b"JZF1";

/// ヘッダの長さの上限の既定値 (64 MiB)
pub const DEFAULT_MAX_HEADER_BYTES: u64 = 64 << 20;

/// 1 フレームの合計 (ヘッダ + 本体) の上限の既定値 (1 GiB)。manifest の max_frame_bytes で変えられる
pub const DEFAULT_MAX_FRAME_BYTES: u64 = 1 << 30;

/// 数の配列を本体に出す要素の数の下限
pub const MIN_ARRAY_BODY_COUNT: usize = 1024;

/// フレームの種類
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FrameKind {
    Request,
    Response,
    Error,
}

/// 本体の説明 (§3.4)。知らないキー (B の shm など) は読み飛ばす
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BodyDesc {
    Image {
        width: u32,
        height: u32,
        format: String,
        stride: u64,
    },
    Array {
        dtype: String,
        count: u64,
    },
}

/// エラー応答の内容。受け手は JuizError::BrokerProxyRemoteError にする
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FrameError {
    #[serde(default)]
    pub class_name: String,
    #[serde(default)]
    pub function_name: String,
    #[serde(default)]
    pub message: String,
}

/// フレームのヘッダ (§3.2)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrameHeader {
    pub id: u64,
    pub kind: FrameKind,
    /// 要求の CapsuleMap の param。method_name (小文字)・class_name・function_name を必ず含む
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<HashMap<String, String>>,
    /// 要求の CapsuleMap の map。値はカプセルの表し方 (§3.3)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<Map<String, Value>>,
    /// 応答のカプセルの表し方
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// 応答のカプセルのオプション
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyDesc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FrameError>,
}

impl FrameHeader {
    fn new(id: u64, kind: FrameKind) -> Self {
        FrameHeader {
            id,
            kind,
            param: None,
            map: None,
            result: None,
            option: None,
            bodies: Vec::new(),
            error: None,
        }
    }
}

/// 画像の画素形式 (§3.4 の表)。(名前, 1 画素のバイト数, 1 チャンネルのバイト数)
fn image_format_of(img: &DynamicImage) -> JuizResult<(&'static str, u64, u64)> {
    use DynamicImage::*;
    Ok(match img {
        ImageLuma8(_) => ("l8", 1, 1),
        ImageLumaA8(_) => ("la8", 2, 1),
        ImageRgb8(_) => ("rgb8", 3, 1),
        ImageRgba8(_) => ("rgba8", 4, 1),
        ImageLuma16(_) => ("l16", 2, 2),
        ImageLumaA16(_) => ("la16", 4, 2),
        ImageRgb16(_) => ("rgb16", 6, 2),
        ImageRgba16(_) => ("rgba16", 8, 2),
        ImageRgb32F(_) => ("rgb32f", 12, 4),
        ImageRgba32F(_) => ("rgba32f", 16, 4),
        _ => {
            return Err(anyhow!(JuizError::InvalidValueError {
                message: format!("this image format ({:?}) can not be sent in a frame", img.color())
            }))
        }
    })
}

fn bytes_per_pixel(format: &str) -> Option<u64> {
    Some(match format {
        "l8" => 1,
        "la8" => 2,
        "rgb8" => 3,
        "rgba8" => 4,
        "l16" => 2,
        "la16" => 4,
        "rgb16" => 6,
        "rgba16" => 8,
        "rgb32f" => 12,
        "rgba32f" => 16,
        _ => return None,
    })
}

/// 本体の説明から、本体の長さを求める。説明が正しくなければエラーの文
fn expected_body_len(desc: &BodyDesc) -> Result<u64, String> {
    match desc {
        BodyDesc::Image {
            width,
            height,
            format,
            stride,
        } => {
            let bpp = bytes_per_pixel(format)
                .ok_or_else(|| format!("unknown image format '{format}'"))?;
            let row = *width as u64 * bpp;
            if *stride < row {
                return Err(format!(
                    "stride ({stride}) must be width x bytes per pixel ({row}) or more"
                ));
            }
            stride
                .checked_mul(*height as u64)
                .ok_or_else(|| "image is too large".to_owned())
        }
        BodyDesc::Array { dtype, count } => match dtype.as_str() {
            "i64le" | "f64le" => count
                .checked_mul(8)
                .ok_or_else(|| "array is too large".to_owned()),
            _ => Err(format!("unknown array dtype '{dtype}'")),
        },
    }
}

// ---- 書く側 ----

/// 送る本体。画像は書くときにカプセルをロックして画素を直接書く
enum OutBody {
    Image { capsule: CapsulePtr, len: u64 },
    Bytes(Vec<u8>),
}

/// 組み立てたフレーム。`write_to` で書く
pub struct OutgoingFrame {
    header: FrameHeader,
    bodies: Vec<OutBody>,
}

/// 値が要素の多い数の配列なら、(dtype, 生バイト) にする。整数と小数が混ざっていれば JSON のまま (§3.4)
fn array_body(value: &Value) -> Option<(&'static str, Vec<u8>)> {
    let arr = value.as_array()?;
    if arr.len() < MIN_ARRAY_BODY_COUNT {
        return None;
    }
    if arr.iter().all(|v| v.as_i64().is_some()) {
        let mut bytes = Vec::with_capacity(arr.len() * 8);
        for v in arr.iter() {
            bytes.extend_from_slice(&v.as_i64().unwrap_or_default().to_le_bytes());
        }
        return Some(("i64le", bytes));
    }
    if arr.iter().all(|v| v.is_f64()) {
        let mut bytes = Vec::with_capacity(arr.len() * 8);
        for v in arr.iter() {
            bytes.extend_from_slice(&v.as_f64().unwrap_or_default().to_le_bytes());
        }
        return Some(("f64le", bytes));
    }
    None
}

/// 多バイトの画素をリトルエンディアンのバイト列にする (ビッグエンディアンの機械のときだけ使う)
fn image_le_bytes(img: &DynamicImage, channel_bytes: u64) -> Vec<u8> {
    let mut bytes = img.as_bytes().to_vec();
    if channel_bytes > 1 {
        for c in bytes.chunks_exact_mut(channel_bytes as usize) {
            c.reverse();
        }
    }
    bytes
}

impl OutgoingFrame {
    /// カプセルの表し方 (§3.3) とオプションを返す。本体に出すものは bodies に足す
    fn capsule_repr(
        &mut self,
        capsule: &CapsulePtr,
    ) -> JuizResult<(Value, HashMap<String, String>)> {
        let index = self.bodies.len();
        let (repr, options, body) = capsule.lock_as_capsule(
            |c| -> JuizResult<(Value, HashMap<String, String>, Option<(BodyDesc, OutBody)>)> {
                let options = c.get_options().clone();
                if c.is_empty() {
                    return Ok((jvalue!({"empty": true}), options, None));
                }
                if let Some(img) = c.try_as_image()? {
                    let (format, bpp, channel_bytes) = image_format_of(img)?;
                    let desc = BodyDesc::Image {
                        width: img.width(),
                        height: img.height(),
                        format: format.to_owned(),
                        stride: img.width() as u64 * bpp,
                    };
                    let body = if cfg!(target_endian = "big") && channel_bytes > 1 {
                        OutBody::Bytes(image_le_bytes(img, channel_bytes))
                    } else {
                        OutBody::Image {
                            capsule: capsule.clone(),
                            len: img.as_bytes().len() as u64,
                        }
                    };
                    return Ok((jvalue!({"body": index}), options, Some((desc, body))));
                }
                if let Some(v) = c.try_as_value()? {
                    if let Some((dtype, bytes)) = array_body(v) {
                        let desc = BodyDesc::Array {
                            dtype: dtype.to_owned(),
                            count: (bytes.len() / 8) as u64,
                        };
                        return Ok((
                            jvalue!({"body": index}),
                            options,
                            Some((desc, OutBody::Bytes(bytes))),
                        ));
                    }
                    return Ok((jvalue!({"value": v}), options, None));
                }
                Err(anyhow!(JuizError::InvalidValueError {
                    message: format!("this capsule can not be sent in a frame ({c})")
                }))
            },
        )??;
        if let Some((desc, body)) = body {
            self.header.bodies.push(desc);
            self.bodies.push(body);
        }
        Ok((repr, options))
    }

    /// 要求のフレーム。param は CapsuleMap の param (method_name などを含む)
    pub fn request(id: u64, map: &CapsuleMap) -> JuizResult<Self> {
        let mut frame = OutgoingFrame {
            header: FrameHeader::new(id, FrameKind::Request),
            bodies: Vec::new(),
        };
        let mut m = Map::new();
        for (key, capsule) in map.iter() {
            let (repr, _options) = frame.capsule_repr(capsule)?;
            m.insert(key.clone(), repr);
        }
        frame.header.param = Some(map.get_params().clone());
        frame.header.map = Some(m);
        Ok(frame)
    }

    /// 応答のフレーム
    pub fn response(id: u64, result: &CapsulePtr) -> JuizResult<Self> {
        let mut frame = OutgoingFrame {
            header: FrameHeader::new(id, FrameKind::Response),
            bodies: Vec::new(),
        };
        let (repr, options) = frame.capsule_repr(result)?;
        frame.header.result = Some(repr);
        if !options.is_empty() {
            frame.header.option = Some(options);
        }
        Ok(frame)
    }

    /// エラー応答のフレーム
    pub fn error(id: u64, class_name: &str, function_name: &str, message: String) -> Self {
        let mut header = FrameHeader::new(id, FrameKind::Error);
        header.error = Some(FrameError {
            class_name: class_name.to_owned(),
            function_name: function_name.to_owned(),
            message,
        });
        OutgoingFrame {
            header,
            bodies: Vec::new(),
        }
    }

    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    /// フレームを書く。小さな部分 (先頭、ヘッダ、本体の長さ) はまとめて書き、本体は複製せずに書く
    pub fn write_to<W: Write>(&self, w: &mut W) -> JuizResult<()> {
        let header = serde_json::to_vec(&self.header)?;
        let header_len: u32 = header
            .len()
            .try_into()
            .map_err(|_| anyhow!("frame header is too large ({} bytes)", header.len()))?;
        let mut small = Vec::with_capacity(header.len() + 16);
        small.extend_from_slice(&MAGIC);
        small.extend_from_slice(&header_len.to_le_bytes());
        small.extend_from_slice(&header);
        for body in self.bodies.iter() {
            match body {
                OutBody::Bytes(bytes) => {
                    small.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                    if bytes.len() < 64 * 1024 {
                        small.extend_from_slice(bytes);
                    } else {
                        w.write_all(&small)?;
                        small.clear();
                        w.write_all(bytes)?;
                    }
                }
                OutBody::Image { capsule, len } => {
                    small.extend_from_slice(&len.to_le_bytes());
                    w.write_all(&small)?;
                    small.clear();
                    capsule.lock_as_image(|img| -> JuizResult<()> {
                        let bytes = img.as_bytes();
                        if bytes.len() as u64 != *len {
                            return Err(anyhow!(
                                "the image was changed while it was being sent"
                            ));
                        }
                        Ok(w.write_all(bytes)?)
                    })??;
                }
            }
        }
        if !small.is_empty() {
            w.write_all(&small)?;
        }
        w.flush()?;
        Ok(())
    }
}

// ---- 読む側 ----

/// 読むときの上限 (§3.5)
#[derive(Debug, Clone, Copy)]
pub struct FrameLimits {
    pub max_header_bytes: u64,
    pub max_frame_bytes: u64,
}

impl Default for FrameLimits {
    fn default() -> Self {
        FrameLimits {
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
        }
    }
}

/// フレームを読めなかったとき、接続をどうするか
#[derive(Debug)]
pub enum FrameReadError {
    /// 相手がフレームの始まりで閉じた
    Closed,
    /// 区切りがわからなくなった (先頭が違う、途中で切れた、読み取りの失敗)。応答を返さずに閉じる
    Broken(anyhow::Error),
    /// エラー応答を返してから閉じる (ヘッダが長すぎる、ヘッダが読めない)
    Fatal { id: u64, error: anyhow::Error },
    /// 区切りは保てている。エラー応答を返し、接続は保つ
    Invalid { id: u64, error: anyhow::Error },
}

impl std::fmt::Display for FrameReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameReadError::Closed => f.write_str("connection closed"),
            FrameReadError::Broken(e) => write!(f, "broken frame: {e:#}"),
            FrameReadError::Fatal { error, .. } => write!(f, "invalid frame: {error:#}"),
            FrameReadError::Invalid { error, .. } => write!(f, "invalid frame: {error:#}"),
        }
    }
}

impl From<FrameReadError> for anyhow::Error {
    fn from(e: FrameReadError) -> Self {
        match e {
            FrameReadError::Broken(e) => e,
            FrameReadError::Fatal { error, .. } | FrameReadError::Invalid { error, .. } => error,
            FrameReadError::Closed => anyhow!("connection closed"),
        }
    }
}

/// 受け取った本体
#[derive(Debug)]
pub enum IncomingBody {
    Image(DynamicImage),
    Array(Value),
}

/// 読んだフレーム
#[derive(Debug)]
pub struct IncomingFrame {
    pub header: FrameHeader,
    bodies: Vec<Option<IncomingBody>>,
}

/// u8 / u16 / f32 / i64 / f64 の Vec。どのビット列も正しい値になる型だけに使う
trait PlainScalar: Copy + Default {
    fn swap_from_le(self) -> Self;
}
macro_rules! plain_scalar {
    ($($t:ty => $bits:ty),*) => {$(
        impl PlainScalar for $t {
            fn swap_from_le(self) -> Self {
                <$t>::from_bits(<$bits>::from_le(self.to_bits()))
            }
        }
    )*};
}
impl PlainScalar for u8 {
    fn swap_from_le(self) -> Self {
        self
    }
}
impl PlainScalar for u16 {
    fn swap_from_le(self) -> Self {
        u16::from_le(self)
    }
}
impl PlainScalar for i64 {
    fn swap_from_le(self) -> Self {
        i64::from_le(self)
    }
}
plain_scalar!(f32 => u32, f64 => u64);

fn as_mut_bytes<T: PlainScalar>(v: &mut [T]) -> &mut [u8] {
    // SAFETY: T は u8/u16/f32/i64/f64 だけで、パディングが無く、どのビット列も正しい値になる
    unsafe {
        std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, std::mem::size_of_val(v))
    }
}

/// 長さ count の Vec<T> に、行ごとに row_bytes を読み、stride との差を読み飛ばす
fn read_rows<T: PlainScalar, R: Read>(
    r: &mut R,
    count: usize,
    row_bytes: usize,
    stride: usize,
    rows: usize,
) -> io::Result<Vec<T>> {
    let mut v: Vec<T> = vec![T::default(); count];
    {
        let bytes = as_mut_bytes(&mut v);
        if row_bytes == 0 {
            skip(r, (stride * rows) as u64)?;
        } else if stride == row_bytes {
            r.read_exact(bytes)?;
        } else {
            let mut pad = vec![0u8; stride - row_bytes];
            for row in bytes.chunks_exact_mut(row_bytes).take(rows) {
                r.read_exact(row)?;
                r.read_exact(&mut pad)?;
            }
        }
    }
    if cfg!(target_endian = "big") {
        for x in v.iter_mut() {
            *x = x.swap_from_le();
        }
    }
    Ok(v)
}

fn read_image<R: Read>(
    r: &mut R,
    width: u32,
    height: u32,
    format: &str,
    stride: u64,
) -> JuizResult<DynamicImage> {
    let bpp = bytes_per_pixel(format).ok_or_else(|| anyhow!("unknown image format '{format}'"))?;
    let row_bytes = (width as u64 * bpp) as usize;
    let stride = stride as usize;
    let rows = height as usize;
    let pixels = width as usize * rows;
    let size_err = || anyhow!("image size does not match ({width}x{height} {format})");
    macro_rules! read_as {
        ($t:ty, $channels:expr, $variant:ident) => {{
            let v = read_rows::<$t, R>(r, pixels * $channels, row_bytes, stride, rows)?;
            DynamicImage::$variant(ImageBuffer::from_raw(width, height, v).ok_or_else(size_err)?)
        }};
    }
    Ok(match format {
        "l8" => read_as!(u8, 1, ImageLuma8),
        "la8" => read_as!(u8, 2, ImageLumaA8),
        "rgb8" => read_as!(u8, 3, ImageRgb8),
        "rgba8" => read_as!(u8, 4, ImageRgba8),
        "l16" => read_as!(u16, 1, ImageLuma16),
        "la16" => read_as!(u16, 2, ImageLumaA16),
        "rgb16" => read_as!(u16, 3, ImageRgb16),
        "rgba16" => read_as!(u16, 4, ImageRgba16),
        "rgb32f" => read_as!(f32, 3, ImageRgb32F),
        "rgba32f" => read_as!(f32, 4, ImageRgba32F),
        _ => return Err(anyhow!("unknown image format '{format}'")),
    })
}

fn read_array<R: Read>(r: &mut R, dtype: &str, count: u64) -> JuizResult<Value> {
    let count = count as usize;
    let values: Vec<Value> = match dtype {
        "i64le" => read_rows::<i64, R>(r, count, count * 8, count * 8, 1)?
            .into_iter()
            .map(|x| Value::Number(x.into()))
            .collect(),
        "f64le" => read_rows::<f64, R>(r, count, count * 8, count * 8, 1)?
            .into_iter()
            .map(|x| Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null))
            .collect(),
        _ => return Err(anyhow!("unknown array dtype '{dtype}'")),
    };
    Ok(Value::Array(values))
}

/// 先頭の 4 バイトを読む。何も読まずに閉じられたら None
fn read_magic<R: Read>(r: &mut R) -> io::Result<Option<[u8; 4]>> {
    let mut buf = [0u8; 4];
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(Some(buf))
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn skip<R: Read>(r: &mut R, len: u64) -> io::Result<()> {
    let skipped = io::copy(&mut r.take(len), &mut io::sink())?;
    if skipped != len {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

/// ヘッダの JSON から id だけでも取り出す (エラー応答に使う)
fn id_of(header: &[u8]) -> u64 {
    serde_json::from_slice::<Value>(header)
        .ok()
        .and_then(|v| v.get("id").and_then(|id| id.as_u64()))
        .unwrap_or(0)
}

/// フレームを 1 つ読む
pub fn read_frame<R: Read>(r: &mut R, limits: &FrameLimits) -> Result<IncomingFrame, FrameReadError> {
    let broken = |e: io::Error| FrameReadError::Broken(anyhow!(e));
    let magic = match read_magic(r).map_err(broken)? {
        None => return Err(FrameReadError::Closed),
        Some(m) => m,
    };
    if magic != MAGIC {
        return Err(FrameReadError::Broken(anyhow!(
            "this is not a juiz frame (the first 4 bytes are {magic:?}, expected \"JZF1\")"
        )));
    }
    let header_len = read_u32(r).map_err(broken)? as u64;
    if header_len > limits.max_header_bytes {
        return Err(FrameReadError::Fatal {
            id: 0,
            error: anyhow!(
                "frame header is too large ({header_len} bytes > {} bytes)",
                limits.max_header_bytes
            ),
        });
    }
    let mut header_bytes = vec![0u8; header_len as usize];
    r.read_exact(&mut header_bytes).map_err(broken)?;
    let header: FrameHeader = serde_json::from_slice(&header_bytes).map_err(|e| {
        FrameReadError::Fatal {
            id: id_of(&header_bytes),
            error: anyhow!("can not read the frame header ({e})"),
        }
    })?;
    let id = header.id;

    // 本体は説明と長さを確かめてから読む。合わないものは長さだけ読み飛ばして区切りを保つ
    let mut total = 8 + header_len;
    let mut bodies = Vec::with_capacity(header.bodies.len());
    let mut first_error: Option<anyhow::Error> = None;
    for (i, desc) in header.bodies.iter().enumerate() {
        let body_len = read_u64(r).map_err(broken)?;
        total = total.saturating_add(8).saturating_add(body_len);
        let problem = if first_error.is_some() {
            Some(None)
        } else if total > limits.max_frame_bytes {
            Some(Some(anyhow!(
                "frame is too large (more than {} bytes)",
                limits.max_frame_bytes
            )))
        } else {
            match expected_body_len(desc) {
                Err(msg) => Some(Some(anyhow!("body {i}: {msg}"))),
                Ok(expected) if expected != body_len => Some(Some(anyhow!(
                    "body {i}: length {body_len} does not match its description ({expected} bytes)"
                ))),
                Ok(_) => None,
            }
        };
        match problem {
            Some(err) => {
                if let Some(err) = err {
                    first_error = Some(err);
                }
                skip(r, body_len).map_err(broken)?;
                bodies.push(None);
            }
            None => {
                let body = match desc {
                    BodyDesc::Image {
                        width,
                        height,
                        format,
                        stride,
                    } => read_image(r, *width, *height, format, *stride).map(IncomingBody::Image),
                    BodyDesc::Array { dtype, count } => {
                        read_array(r, dtype, *count).map(IncomingBody::Array)
                    }
                };
                // 長さは確かめてあるので、ここで失敗するのは読み取りの失敗だけ
                bodies.push(Some(body.map_err(FrameReadError::Broken)?));
            }
        }
    }
    if let Some(error) = first_error {
        return Err(FrameReadError::Invalid { id, error });
    }
    Ok(IncomingFrame { header, bodies })
}

impl IncomingFrame {
    pub fn id(&self) -> u64 {
        self.header.id
    }

    pub fn kind(&self) -> FrameKind {
        self.header.kind
    }

    /// カプセルの表し方からカプセルを作る。本体は取り出す (同じ本体を 2 度は使えない)
    fn capsule_from_repr(&mut self, repr: Value) -> JuizResult<Capsule> {
        let invalid = |repr: &Value| {
            anyhow!(JuizError::InvalidValueError {
                message: format!("invalid capsule in frame ({repr})")
            })
        };
        let Value::Object(mut obj) = repr else {
            return Err(invalid(&repr));
        };
        if let Some(v) = obj.remove("value") {
            return Ok(Capsule::from(v));
        }
        if obj.get("empty").and_then(|v| v.as_bool()) == Some(true) {
            return Ok(Capsule::empty());
        }
        if let Some(index) = obj.get("body").and_then(|v| v.as_u64()) {
            let body = self
                .bodies
                .get_mut(index as usize)
                .and_then(|b| b.take())
                .ok_or_else(|| anyhow!("body {index} is not in the frame"))?;
            return Ok(match body {
                IncomingBody::Image(img) => Capsule::from(img),
                IncomingBody::Array(v) => Capsule::from(v),
            });
        }
        Err(invalid(&Value::Object(obj)))
    }

    /// 要求のフレームを CapsuleMap にする。param が無ければエラー
    pub fn into_capsule_map(mut self) -> JuizResult<CapsuleMap> {
        if self.header.kind != FrameKind::Request {
            return Err(anyhow!("frame is not a request ({:?})", self.header.kind));
        }
        let param = self
            .header
            .param
            .take()
            .ok_or_else(|| anyhow!("request frame has no 'param'"))?;
        let mut cm = CapsuleMap::new();
        for (k, v) in param.iter() {
            cm.set_param(k, v);
        }
        if let Some(map) = self.header.map.take() {
            for (k, repr) in map {
                let c = self.capsule_from_repr(repr)?;
                cm.insert(k, c.into());
            }
        }
        Ok(cm)
    }

    /// 応答のフレームをカプセルにする。エラー応答は BrokerProxyRemoteError にする
    pub fn into_capsule_ptr(mut self) -> JuizResult<CapsulePtr> {
        match self.header.kind {
            FrameKind::Response => {
                let repr = self
                    .header
                    .result
                    .take()
                    .ok_or_else(|| anyhow!("response frame has no 'result'"))?;
                let mut c = self.capsule_from_repr(repr)?;
                if let Some(option) = self.header.option.take() {
                    for (k, v) in option.iter() {
                        c.set_option(k, v);
                    }
                }
                Ok(c.into())
            }
            FrameKind::Error => {
                let e = self.header.error.take().unwrap_or_default();
                Err(anyhow!(JuizError::BrokerProxyRemoteError {
                    class_name: e.class_name,
                    function_name: e.function_name,
                    message: e.message,
                }))
            }
            FrameKind::Request => Err(anyhow!("expected a response, but got a request frame")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_sdk::image::{Luma, Rgb, Rgba};

    fn roundtrip_request(map: CapsuleMap) -> CapsuleMap {
        let mut buf = Vec::new();
        OutgoingFrame::request(3, &map).unwrap().write_to(&mut buf).unwrap();
        let frame = read_frame(&mut buf.as_slice(), &FrameLimits::default()).unwrap();
        assert_eq!(frame.id(), 3);
        frame.into_capsule_map().unwrap()
    }

    fn roundtrip_response(c: CapsulePtr) -> CapsulePtr {
        let mut buf = Vec::new();
        OutgoingFrame::response(9, &c).unwrap().write_to(&mut buf).unwrap();
        let frame = read_frame(&mut buf.as_slice(), &FrameLimits::default()).unwrap();
        assert_eq!(frame.id(), 9);
        frame.into_capsule_ptr().unwrap()
    }

    fn images() -> Vec<DynamicImage> {
        let (w, h) = (7u32, 5u32);
        vec![
            DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| Luma([(x * 10 + y) as u8]))),
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
                Rgb([x as u8, y as u8, (x * y) as u8])
            })),
            DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, y| {
                Rgba([x as u8, y as u8, 3, 255])
            })),
            DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| {
                Luma([(x * 1000 + y * 7) as u16])
            })),
            DynamicImage::ImageRgba32F(ImageBuffer::from_fn(w, h, |x, y| {
                Rgba([x as f32 * 0.5, y as f32 * -1.25, 3.0, 1.0])
            })),
        ]
    }

    #[test]
    fn request_keeps_values_images_and_params() {
        let mut map = CapsuleMap::new();
        map.set_param("method_name", "update");
        map.set_param("class_name", "process");
        map.insert("gain".to_owned(), jvalue!(1.5).into());
        map.insert("count".to_owned(), jvalue!(3).into());
        map.insert("obj".to_owned(), jvalue!({"a": [1, 2.5, "x"]}).into());
        for (i, img) in images().into_iter().enumerate() {
            map.insert(format!("img{i}"), img.into());
        }
        let got = roundtrip_request(map);
        assert_eq!(got.get_param("method_name").unwrap(), "update");
        assert_eq!(got.get_param("class_name").unwrap(), "process");
        assert_eq!(got.get("gain").unwrap().extract_value().unwrap(), jvalue!(1.5));
        let count = got.get("count").unwrap().extract_value().unwrap();
        assert!(count.is_i64(), "integers stay integers ({count})");
        assert_eq!(
            got.get("obj").unwrap().extract_value().unwrap(),
            jvalue!({"a": [1, 2.5, "x"]})
        );
        for (i, img) in images().into_iter().enumerate() {
            let received = got.get(&format!("img{i}")).unwrap().extract_image().unwrap();
            assert_eq!(received, img, "image {i} ({:?})", img.color());
        }
    }

    #[test]
    fn response_keeps_image_and_options() {
        let mut c: CapsulePtr = images().remove(1).into();
        c.set_function_name("call").unwrap();
        c.set_class_name("process").unwrap();
        let got = roundtrip_response(c);
        assert_eq!(got.get_function_name().unwrap(), "call");
        assert_eq!(got.get_class_name().unwrap(), "process");
        assert_eq!(got.extract_image().unwrap(), images().remove(1));
    }

    #[test]
    fn empty_capsule_is_sent() {
        assert!(roundtrip_response(CapsulePtr::new()).is_empty().unwrap());
    }

    #[test]
    fn large_numeric_arrays_are_sent_as_bodies() {
        let ints: Vec<i64> = (0..3000).map(|i| i * 3 - 1000).collect();
        let floats: Vec<f64> = (0..2000).map(|i| i as f64 * 0.25).collect();
        let mut mixed: Vec<Value> = (0..2000).map(|i| jvalue!(i)).collect();
        mixed.push(jvalue!(0.5));

        let mut map = CapsuleMap::new();
        map.set_param("method_name", "update");
        map.insert("ints".to_owned(), jvalue!(ints).into());
        map.insert("floats".to_owned(), jvalue!(floats).into());
        map.insert("mixed".to_owned(), jvalue!(mixed).into());
        let frame = OutgoingFrame::request(1, &map).unwrap();
        // 整数だけと小数だけの配列は本体になり、混ざった配列は JSON のまま
        let mut dtypes: Vec<String> = frame
            .header()
            .bodies
            .iter()
            .map(|b| match b {
                BodyDesc::Array { dtype, .. } => dtype.clone(),
                _ => panic!(),
            })
            .collect();
        dtypes.sort();
        assert_eq!(dtypes, vec!["f64le", "i64le"]);

        let got = roundtrip_request(map);
        let ints_v = got.get("ints").unwrap().extract_value().unwrap();
        assert!(ints_v.as_array().unwrap().iter().all(|v| v.is_i64()));
        assert_eq!(ints_v, jvalue!(ints));
        let floats_v = got.get("floats").unwrap().extract_value().unwrap();
        assert!(floats_v.as_array().unwrap().iter().all(|v| v.is_f64()));
        assert_eq!(floats_v, jvalue!(floats));
        assert_eq!(got.get("mixed").unwrap().extract_value().unwrap(), jvalue!(mixed));
    }

    #[test]
    fn padded_stride_is_removed() {
        // 行の終わりに 2 バイトの余白がある 3x2 の rgb8
        let header = jvalue!({
            "id": 5, "kind": "response", "result": {"body": 0},
            "bodies": [{"kind": "image", "width": 3, "height": 2, "format": "rgb8", "stride": 11}]
        });
        let header = serde_json::to_vec(&header).unwrap();
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&22u64.to_le_bytes());
        for row in 0..2u8 {
            buf.extend((0..9u8).map(|i| row * 100 + i));
            buf.extend_from_slice(&[0xEE, 0xEE]);
        }
        let img = read_frame(&mut buf.as_slice(), &FrameLimits::default())
            .unwrap()
            .into_capsule_ptr()
            .unwrap()
            .extract_image()
            .unwrap();
        let mut expected: Vec<u8> = (0..9).collect();
        expected.extend((0..9).map(|i| 100 + i));
        assert_eq!(img.as_bytes(), expected.as_slice());
    }

    #[test]
    fn error_frame_becomes_remote_error() {
        let mut buf = Vec::new();
        OutgoingFrame::error(4, "process", "call", "boom".to_owned())
            .write_to(&mut buf)
            .unwrap();
        let e = read_frame(&mut buf.as_slice(), &FrameLimits::default())
            .unwrap()
            .into_capsule_ptr()
            .unwrap_err();
        match e.downcast_ref::<JuizError>() {
            Some(JuizError::BrokerProxyRemoteError {
                class_name,
                function_name,
                message,
            }) => {
                assert_eq!((class_name.as_str(), function_name.as_str()), ("process", "call"));
                assert_eq!(message, "boom");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bad_frames() {
        let limits = FrameLimits::default();
        // 何も無ければ閉じられた
        assert!(matches!(read_frame(&mut &b""[..], &limits), Err(FrameReadError::Closed)));
        // 先頭が違う
        assert!(matches!(
            read_frame(&mut &b"HTTP/1.1"[..], &limits),
            Err(FrameReadError::Broken(_))
        ));
        // ヘッダが長すぎる
        let mut buf = MAGIC.to_vec();
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read_frame(&mut buf.as_slice(), &limits),
            Err(FrameReadError::Fatal { .. })
        ));
        // ヘッダが JSON でない
        let mut buf = MAGIC.to_vec();
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(b"{{{");
        assert!(matches!(
            read_frame(&mut buf.as_slice(), &limits),
            Err(FrameReadError::Fatal { .. })
        ));
    }

    #[test]
    fn invalid_body_is_skipped_and_next_frame_is_readable() {
        // 説明と長さが合わない本体の後ろに、正しいフレームが続く
        let header = serde_json::to_vec(&jvalue!({
            "id": 8, "kind": "request", "param": {}, "map": {"img": {"body": 0}},
            "bodies": [{"kind": "image", "width": 4, "height": 4, "format": "rgb8", "stride": 12}]
        }))
        .unwrap();
        let mut buf = MAGIC.to_vec();
        buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&5u64.to_le_bytes());
        buf.extend_from_slice(&[1, 2, 3, 4, 5]);
        let mut map = CapsuleMap::new();
        map.set_param("method_name", "read");
        OutgoingFrame::request(9, &map).unwrap().write_to(&mut buf).unwrap();

        let mut r = buf.as_slice();
        match read_frame(&mut r, &FrameLimits::default()) {
            Err(FrameReadError::Invalid { id, .. }) => assert_eq!(id, 8),
            other => panic!("{other:?}"),
        }
        assert_eq!(read_frame(&mut r, &FrameLimits::default()).unwrap().id(), 9);

        // 合計の上限を超える本体も読み飛ばす
        let mut big = CapsuleMap::new();
        big.set_param("method_name", "update");
        big.insert("img".to_owned(), images().remove(1).into());
        let mut buf = Vec::new();
        OutgoingFrame::request(10, &big).unwrap().write_to(&mut buf).unwrap();
        OutgoingFrame::request(11, &map).unwrap().write_to(&mut buf).unwrap();
        let small = FrameLimits {
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            max_frame_bytes: 200,
        };
        let mut r = buf.as_slice();
        assert!(matches!(
            read_frame(&mut r, &small),
            Err(FrameReadError::Invalid { id: 10, .. })
        ));
        assert_eq!(read_frame(&mut r, &small).unwrap().id(), 11);
    }
}

