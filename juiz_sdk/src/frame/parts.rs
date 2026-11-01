//! メモリ上でフレームを受け渡す道具 (Rust プラグインの C ABI のホスト側、docs/spec/rust-plugin-abi.md §5.2・§5.3)
//!
//! - [`OutgoingFrame::with_body_slices`]: 引数の本体を複製せず、カプセルの画素を直接指すバイト列で渡す
//! - [`FrameReceiver`]: 戻り値のヘッダを読んで本体ごとに最終的な領域を確保し、相手にそこへ直接書かせる

use anyhow::anyhow;
use image::ImageBuffer;
use serde_json::Number;

use super::{
    check_part_desc, BodyDesc, FrameHeader, FrameLimits, IncomingBody, IncomingFrame,
    OutBody, OutgoingFrame, PlainScalar,
};
use crate::prelude::*;

impl OutgoingFrame {
    /// 本体ごとのバイト列を f に渡す。画像はカプセルをロックしたまま、その画素を直接指す (複製しない)。
    /// ロックは f から戻るまで持つ。同じカプセルが 2 度出てくるときは、最初のロックを使い回す。
    /// 共有メモリ (B) の本体はエラー
    pub fn with_body_slices<R>(&self, f: impl FnOnce(&[&[u8]]) -> R) -> JuizResult<R> {
        let mut acc: Vec<(*const u8, usize)> = Vec::with_capacity(self.bodies.len());
        let mut f = Some(f);
        visit_bodies(&self.bodies, 0, &mut acc, &mut f)
    }
}

fn visit_bodies<R, F: FnOnce(&[&[u8]]) -> R>(
    bodies: &[OutBody],
    i: usize,
    acc: &mut Vec<(*const u8, usize)>,
    f: &mut Option<F>,
) -> JuizResult<R> {
    if i == bodies.len() {
        // SAFETY: 各組は、まだロックを持っている画像か、bodies が持つ Vec を指す
        let slices: Vec<&[u8]> = acc
            .iter()
            .map(|(p, l)| unsafe { std::slice::from_raw_parts(*p, *l) })
            .collect();
        let f = f.take().ok_or_else(|| anyhow!("with_body_slices called the function twice"))?;
        return Ok(f(&slices));
    }
    match &bodies[i] {
        OutBody::Bytes(b) => {
            acc.push((b.as_ptr(), b.len()));
            visit_bodies(bodies, i + 1, acc, f)
        }
        OutBody::Shared => Err(anyhow!(
            "body {i} is in shared memory and can not be passed in memory"
        )),
        OutBody::Image { capsule, len } => {
            let same = (0..i).find(|&j| {
                matches!(&bodies[j], OutBody::Image { capsule: c, .. } if c.ptr_eq(capsule))
            });
            if let Some(j) = same {
                let prev = acc[j];
                acc.push(prev);
                return visit_bodies(bodies, i + 1, acc, f);
            }
            capsule.lock_as_image(|img| -> JuizResult<R> {
                let bytes = img.as_bytes();
                if bytes.len() as u64 != *len {
                    return Err(anyhow!("the image was changed while it was being sent"));
                }
                acc.push((bytes.as_ptr(), bytes.len()));
                visit_bodies(bodies, i + 1, acc, f)
            })?
        }
    }
}

/// 使い回すために取っておく画像の数の上限 (スレッドごと、issue #339)
pub const SPARE_IMAGES_MAX: usize = 2;

/// カプセルの中身が、ほかから参照されていない普通の画像 (Native でない) なら、取り出して返す。
/// 使い終わった引数・戻り値の画素の領域を、次の受け取りに使い回すため (issue #339)。複製はしない
pub fn take_spare_image(c: CapsulePtr) -> Option<DynamicImage> {
    let c = c.try_into_capsule().ok()?;
    if c.is_native() || !c.is_image() {
        return None;
    }
    c.to_image()
}

/// 使い回す画像を spares に足す。SPARE_IMAGES_MAX を超えたら古いものから捨てる
pub fn keep_spare_image(spares: &mut Vec<DynamicImage>, img: DynamicImage) {
    if spares.len() >= SPARE_IMAGES_MAX {
        spares.remove(0);
    }
    spares.push(img);
}

/// 最終的な領域。画像の画素と数の配列の要素の型ごと。
/// 新しく確保した領域は 0 で埋めない (長さ 0、容量だけ確保する)。相手に書かせてから長さを決める (§5.3)
enum BodyBuffer {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
    I64(Vec<i64>),
    F64(Vec<f64>),
}

fn bytes_of<T: PlainScalar>(v: &mut [T]) -> &mut [u8] {
    super::as_mut_bytes(v)
}

fn swap_all<T: PlainScalar>(v: &mut [T]) {
    if cfg!(target_endian = "big") {
        for x in v.iter_mut() {
            *x = x.swap_from_le();
        }
    }
}

/// 画像の形式の、1 画素の要素の数と要素の型 (0 u8 / 1 u16 / 2 f32)
fn image_layout(format: &str) -> JuizResult<(usize, u8)> {
    Ok(match format {
        "l8" => (1, 0),
        "la8" => (2, 0),
        "rgb8" => (3, 0),
        "rgba8" => (4, 0),
        "l16" => (1, 1),
        "la16" => (2, 1),
        "rgb16" => (3, 1),
        "rgba16" => (4, 1),
        "rgb32f" => (3, 2),
        "rgba32f" => (4, 2),
        _ => return Err(anyhow!("unknown image format '{format}'")),
    })
}

/// 本体の領域の状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyState {
    /// 容量だけ確保した (中身は決まっていない)
    Reserved,
    /// 書き込み先を渡した。相手が全部書く約束 ([`FrameReceiver::body_ptr`])
    Lent,
    /// 使い回した領域。中身は前の画像のまま (まだ書き込み先を渡していない)
    Spare,
    /// 書き込み先を渡し、中身が決まっている
    Ready,
}

struct Body {
    buf: BodyBuffer,
    /// 要素の数
    elems: usize,
    state: BodyState,
}

impl BodyBuffer {
    /// 容量 elems の空の Vec
    fn reserve(kind: u8, elems: usize) -> Self {
        match kind {
            0 => BodyBuffer::U8(Vec::with_capacity(elems)),
            1 => BodyBuffer::U16(Vec::with_capacity(elems)),
            2 => BodyBuffer::F32(Vec::with_capacity(elems)),
            3 => BodyBuffer::I64(Vec::with_capacity(elems)),
            _ => BodyBuffer::F64(Vec::with_capacity(elems)),
        }
    }

    /// 使い終わった画像の画素の領域。形式と要素の数が合わなければ None
    fn from_spare(img: DynamicImage, format: &str, elems: usize) -> Option<Self> {
        use DynamicImage::*;
        let buf = match (format, img) {
            ("l8", ImageLuma8(b)) => BodyBuffer::U8(b.into_raw()),
            ("la8", ImageLumaA8(b)) => BodyBuffer::U8(b.into_raw()),
            ("rgb8", ImageRgb8(b)) => BodyBuffer::U8(b.into_raw()),
            ("rgba8", ImageRgba8(b)) => BodyBuffer::U8(b.into_raw()),
            ("l16", ImageLuma16(b)) => BodyBuffer::U16(b.into_raw()),
            ("la16", ImageLumaA16(b)) => BodyBuffer::U16(b.into_raw()),
            ("rgb16", ImageRgb16(b)) => BodyBuffer::U16(b.into_raw()),
            ("rgba16", ImageRgba16(b)) => BodyBuffer::U16(b.into_raw()),
            ("rgb32f", ImageRgb32F(b)) => BodyBuffer::F32(b.into_raw()),
            ("rgba32f", ImageRgba32F(b)) => BodyBuffer::F32(b.into_raw()),
            _ => return None,
        };
        (buf.len() == elems).then_some(buf)
    }

    fn len(&self) -> usize {
        match self {
            BodyBuffer::U8(v) => v.len(),
            BodyBuffer::U16(v) => v.len(),
            BodyBuffer::F32(v) => v.len(),
            BodyBuffer::I64(v) => v.len(),
            BodyBuffer::F64(v) => v.len(),
        }
    }

    /// 要素 elems 個ぶんの先頭のポインタとバイト数 (容量の中、初期化されているとは限らない)
    fn raw_parts(&mut self, elems: usize) -> (*mut u8, usize) {
        fn raw<T>(v: &mut Vec<T>, elems: usize) -> (*mut u8, usize) {
            debug_assert!(v.capacity() >= elems);
            (v.as_mut_ptr() as *mut u8, elems * std::mem::size_of::<T>())
        }
        match self {
            BodyBuffer::U8(v) => raw(v, elems),
            BodyBuffer::U16(v) => raw(v, elems),
            BodyBuffer::F32(v) => raw(v, elems),
            BodyBuffer::I64(v) => raw(v, elems),
            BodyBuffer::F64(v) => raw(v, elems),
        }
    }

    /// 長さを elems にする
    ///
    /// # Safety
    /// 容量の先頭 elems 個の要素が書き込まれていること (どの要素の型も、どのビットの並びでも正しい値)
    unsafe fn assume_written(&mut self, elems: usize) {
        match self {
            BodyBuffer::U8(v) => v.set_len(elems),
            BodyBuffer::U16(v) => v.set_len(elems),
            BodyBuffer::F32(v) => v.set_len(elems),
            BodyBuffer::I64(v) => v.set_len(elems),
            BodyBuffer::F64(v) => v.set_len(elems),
        }
    }

    /// 0 で埋めて長さを elems にする
    fn fill_zero(&mut self, elems: usize) {
        match self {
            BodyBuffer::U8(v) => v.resize(elems, 0),
            BodyBuffer::U16(v) => v.resize(elems, 0),
            BodyBuffer::F32(v) => v.resize(elems, 0.0),
            BodyBuffer::I64(v) => v.resize(elems, 0),
            BodyBuffer::F64(v) => v.resize(elems, 0.0),
        }
    }

    fn as_mut_bytes(&mut self) -> &mut [u8] {
        match self {
            BodyBuffer::U8(v) => v.as_mut_slice(),
            BodyBuffer::U16(v) => bytes_of(v),
            BodyBuffer::F32(v) => bytes_of(v),
            BodyBuffer::I64(v) => bytes_of(v),
            BodyBuffer::F64(v) => bytes_of(v),
        }
    }

    fn into_body(self, desc: &BodyDesc) -> JuizResult<IncomingBody> {
        match (desc, self) {
            (
                BodyDesc::Image {
                    width,
                    height,
                    format,
                    ..
                },
                buf,
            ) => {
                let (w, h) = (*width, *height);
                let size_err = || anyhow!("image size does not match ({w}x{h} {format})");
                macro_rules! image {
                    ($v:expr, $variant:ident) => {
                        DynamicImage::$variant(ImageBuffer::from_raw(w, h, $v).ok_or_else(size_err)?)
                    };
                }
                let img = match (format.as_str(), buf) {
                    ("l8", BodyBuffer::U8(v)) => image!(v, ImageLuma8),
                    ("la8", BodyBuffer::U8(v)) => image!(v, ImageLumaA8),
                    ("rgb8", BodyBuffer::U8(v)) => image!(v, ImageRgb8),
                    ("rgba8", BodyBuffer::U8(v)) => image!(v, ImageRgba8),
                    ("l16", BodyBuffer::U16(mut v)) => {
                        swap_all(&mut v);
                        image!(v, ImageLuma16)
                    }
                    ("la16", BodyBuffer::U16(mut v)) => {
                        swap_all(&mut v);
                        image!(v, ImageLumaA16)
                    }
                    ("rgb16", BodyBuffer::U16(mut v)) => {
                        swap_all(&mut v);
                        image!(v, ImageRgb16)
                    }
                    ("rgba16", BodyBuffer::U16(mut v)) => {
                        swap_all(&mut v);
                        image!(v, ImageRgba16)
                    }
                    ("rgb32f", BodyBuffer::F32(mut v)) => {
                        swap_all(&mut v);
                        image!(v, ImageRgb32F)
                    }
                    ("rgba32f", BodyBuffer::F32(mut v)) => {
                        swap_all(&mut v);
                        image!(v, ImageRgba32F)
                    }
                    _ => return Err(anyhow!("unknown image format '{format}'")),
                };
                Ok(IncomingBody::Image(img))
            }
            (BodyDesc::Array { .. }, BodyBuffer::I64(mut v)) => {
                swap_all(&mut v);
                Ok(IncomingBody::Array(Value::Array(
                    v.into_iter().map(|x| Value::Number(x.into())).collect(),
                )))
            }
            (BodyDesc::Array { .. }, BodyBuffer::F64(mut v)) => {
                swap_all(&mut v);
                Ok(IncomingBody::Array(Value::Array(
                    v.into_iter()
                        .map(|x| Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null))
                        .collect(),
                )))
            }
            (BodyDesc::Array { dtype, .. }, _) => Err(anyhow!("unknown array dtype '{dtype}'")),
        }
    }
}

/// 戻り値を受け取る側 (§5.3)。ヘッダを読んで本体ごとに最終的な `DynamicImage` / `Vec` の領域を確保し、
/// 相手は [`FrameReceiver::body_ptr`] (または [`FrameReceiver::body_mut`]) の領域へ直接書く。
/// 書き終わったら [`FrameReceiver::finish`] でフレームにする。画像のコピーは相手が書く 1 回だけ。
///
/// 新しく確保した領域は 0 で埋めない。同じ大きさ・形式の使い終わった画像を渡せば、その領域を使い回す
/// ([`FrameReceiver::begin_reusing`]、新しい領域のページフォールトを避ける、issue #339)
pub struct FrameReceiver {
    header: FrameHeader,
    bodies: Vec<Body>,
}

impl FrameReceiver {
    /// ヘッダ (先頭の "JZF1" と長さの無い JSON) を読み、本体の領域を確保する。
    /// 共有メモリの本体、余白のある画像、上限を超える大きさはエラー
    pub fn begin(header: &[u8], limits: &FrameLimits) -> JuizResult<Self> {
        Self::begin_reusing(header, limits, &mut Vec::new())
    }

    /// begin と同じ。画像の本体は、spares の中に同じ大きさ・形式の画像があれば、その領域を使う
    /// (使ったものは spares から取り除く。合わないものには触らない)
    pub fn begin_reusing(
        header: &[u8],
        limits: &FrameLimits,
        spares: &mut Vec<DynamicImage>,
    ) -> JuizResult<Self> {
        if header.len() as u64 > limits.max_header_bytes {
            return Err(anyhow!(
                "frame header is too large ({} bytes > {} bytes)",
                header.len(),
                limits.max_header_bytes
            ));
        }
        let header: FrameHeader = serde_json::from_slice(header)
            .map_err(|e| anyhow!("can not read the frame header ({e})"))?;
        let mut total = header.bodies.len() as u64;
        for (i, desc) in header.bodies.iter().enumerate() {
            total = total.saturating_add(check_part_desc(i, desc)?);
            if total > limits.max_frame_bytes {
                return Err(anyhow!(
                    "frame is too large (more than {} bytes)",
                    limits.max_frame_bytes
                ));
            }
        }
        let bodies = header
            .bodies
            .iter()
            .map(|d| Self::alloc(d, spares))
            .collect::<JuizResult<Vec<_>>>()?;
        Ok(FrameReceiver { header, bodies })
    }

    fn alloc(desc: &BodyDesc, spares: &mut Vec<DynamicImage>) -> JuizResult<Body> {
        match desc {
            BodyDesc::Image {
                width,
                height,
                format,
                ..
            } => {
                let (channels, kind) = image_layout(format)?;
                let elems = *width as usize * *height as usize * channels;
                let found = spares.iter().position(|img| {
                    img.width() == *width
                        && img.height() == *height
                        && super::image_format_of(img).is_ok_and(|f| f.0 == format)
                });
                if let Some(i) = found {
                    if let Some(buf) = BodyBuffer::from_spare(spares.swap_remove(i), format, elems) {
                        return Ok(Body {
                            buf,
                            elems,
                            state: BodyState::Spare,
                        });
                    }
                }
                Ok(Body {
                    buf: BodyBuffer::reserve(kind, elems),
                    elems,
                    state: BodyState::Reserved,
                })
            }
            BodyDesc::Array { dtype, count, .. } => {
                let kind = match dtype.as_str() {
                    "i64le" => 3,
                    "f64le" => 4,
                    _ => return Err(anyhow!("unknown array dtype '{dtype}'")),
                };
                let elems = *count as usize;
                Ok(Body {
                    buf: BodyBuffer::reserve(kind, elems),
                    elems,
                    state: BodyState::Reserved,
                })
            }
        }
    }

    /// 読んだヘッダ
    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    /// 本体の数
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// 本体 index の書き込み先の先頭とバイト数 (長さはヘッダの説明どおり)。中身は 0 で埋めていない
    ///
    /// # Safety
    /// 受け取った側は、[`FrameReceiver::finish`] を呼ぶまでに、その領域の全バイトを書かなければならない。
    /// 領域は self を動かしても変わらないが、self を落とすと無効になる
    pub unsafe fn body_ptr(&mut self, index: usize) -> Option<(*mut u8, usize)> {
        let body = self.bodies.get_mut(index)?;
        body.state = match body.state {
            BodyState::Reserved => BodyState::Lent,
            BodyState::Spare => BodyState::Ready,
            s => s,
        };
        Some(body.buf.raw_parts(body.elems))
    }

    /// 本体 index の書き込み先 (長さはヘッダの説明どおり)。まだ中身が決まっていなければ 0 で埋める
    pub fn body_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        let body = self.bodies.get_mut(index)?;
        match body.state {
            BodyState::Reserved => body.buf.fill_zero(body.elems),
            // SAFETY: body_ptr を受け取った側が全部書く約束をしている
            BodyState::Lent => unsafe { body.buf.assume_written(body.elems) },
            BodyState::Spare | BodyState::Ready => {}
        }
        body.state = BodyState::Ready;
        Some(body.buf.as_mut_bytes())
    }

    /// 書き終わった本体とヘッダからフレームを作る。書き込み先を渡していない本体があればエラー
    /// (使い回した領域の古い中身や、決まっていない中身を戻り値にしない)
    pub fn finish(self) -> JuizResult<IncomingFrame> {
        let FrameReceiver { header, bodies } = self;
        let incoming = bodies
            .into_iter()
            .zip(header.bodies.iter())
            .enumerate()
            .map(|(i, (mut b, desc))| {
                match b.state {
                    BodyState::Reserved | BodyState::Spare => {
                        return Err(anyhow!("body {i} was not written"));
                    }
                    // SAFETY: body_ptr を受け取った側が全部書く約束をしている
                    BodyState::Lent => unsafe { b.buf.assume_written(b.elems) },
                    BodyState::Ready => {}
                }
                debug_assert_eq!(b.buf.len(), b.elems);
                b.buf.into_body(desc).map(Some)
            })
            .collect::<JuizResult<Vec<_>>>()?;
        Ok(IncomingFrame {
            header,
            bodies: incoming,
            natives: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgb, Rgba};

    /// 送り手のフレームを with_body_slices で渡し、FrameReceiver で受けて元に戻す
    fn roundtrip(c: CapsulePtr) -> CapsulePtr {
        let frame = OutgoingFrame::response(0, &c).unwrap();
        let header = frame.header_bytes().unwrap();
        frame
            .with_body_slices(|bodies| {
                let mut r = FrameReceiver::begin(&header, &FrameLimits::default()).unwrap();
                assert_eq!(r.body_count(), bodies.len());
                for (i, b) in bodies.iter().enumerate() {
                    r.body_mut(i).unwrap().copy_from_slice(b);
                }
                r.finish().unwrap().into_capsule_ptr().unwrap()
            })
            .unwrap()
    }

    #[test]
    fn images_and_arrays_roundtrip() {
        let (w, h) = (5u32, 3u32);
        let imgs = vec![
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| Rgb([x as u8, y as u8, 7]))),
            DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| Luma([(x * 1000 + y) as u16]))),
            DynamicImage::ImageRgba32F(ImageBuffer::from_fn(w, h, |x, y| {
                Rgba([x as f32 * 0.5, y as f32, 1.0, -2.0])
            })),
        ];
        for img in imgs {
            let back = roundtrip(Capsule::from(img.clone()).into());
            back.lock_as_image(|b| assert_eq!(b, &img)).unwrap();
        }
        let ints: Vec<Value> = (0..2000).map(|i| jvalue!(i - 1000)).collect();
        let back = roundtrip(jvalue!(ints.clone()).into());
        assert_eq!(back.extract_value().unwrap(), Value::Array(ints));
        let floats: Vec<Value> = (0..1500).map(|i| jvalue!(i as f64 * 0.25)).collect();
        let back = roundtrip(jvalue!(floats.clone()).into());
        assert_eq!(back.extract_value().unwrap(), Value::Array(floats));
    }

    #[test]
    fn slices_point_into_the_capsule_and_same_capsule_is_locked_once() {
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(4, 2, |x, _| Rgb([x as u8, 0, 0])));
        let c: CapsulePtr = Capsule::from(img).into();
        let mut map = CapsuleMap::new();
        map.insert("a".to_owned(), c.clone());
        map.insert("b".to_owned(), c.clone());
        let frame = OutgoingFrame::call_request(&map).unwrap();
        assert_eq!(frame.body_count(), 2);
        let ptr = c.lock_as_image(|i| i.as_bytes().as_ptr() as usize).unwrap();
        // 同じカプセルを 2 度ロックすると止まるので、止まらずに戻ることも確かめている
        let ptrs = frame
            .with_body_slices(|b| b.iter().map(|s| s.as_ptr() as usize).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(ptrs, vec![ptr, ptr]);
    }

    #[test]
    fn receiver_rejects_padding_and_shm() {
        let pad = br#"{"id":0,"kind":"response","result":{"body":0},"bodies":[{"kind":"image","width":2,"height":2,"format":"rgb8","stride":8}]}"#;
        assert!(FrameReceiver::begin(pad, &FrameLimits::default()).is_err());
        let shm = br#"{"id":0,"kind":"response","result":{"body":0},"bodies":[{"kind":"array","dtype":"i64le","count":1,"shm":{"region":"r","slot":0,"seq":2,"len":8}}]}"#;
        assert!(FrameReceiver::begin(shm, &FrameLimits::default()).is_err());
        let big = br#"{"id":0,"kind":"response","result":{"body":0},"bodies":[{"kind":"array","dtype":"i64le","count":1000}]}"#;
        let limits = FrameLimits {
            max_header_bytes: 1 << 20,
            max_frame_bytes: 100,
        };
        assert!(FrameReceiver::begin(big, &limits).is_err());
    }

    fn rgb_response(w: u32, h: u32) -> (CapsulePtr, Vec<u8>) {
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| Rgb([x as u8, y as u8, 9])));
        let c: CapsulePtr = Capsule::from(img).into();
        let header = OutgoingFrame::response(0, &c).unwrap().header_bytes().unwrap();
        (c, header)
    }

    /// 書き込み先を渡していない本体は、0 で埋めた中身にも使い回した中身にもせず、エラーにする (#339)
    #[test]
    fn unwritten_bodies_are_errors() {
        let (_, header) = rgb_response(4, 2);
        let r = FrameReceiver::begin(&header, &FrameLimits::default()).unwrap();
        assert!(format!("{:#}", r.finish().unwrap_err()).contains("body 0 was not written"));
        let spare = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 2, Rgb([1, 2, 3])));
        let mut spares = vec![spare];
        let r = FrameReceiver::begin_reusing(&header, &FrameLimits::default(), &mut spares).unwrap();
        assert!(spares.is_empty());
        assert!(r.finish().is_err());
    }

    /// body_ptr で渡した領域に書けば、0 で埋めずにそのまま戻り値になる
    #[test]
    fn body_ptr_is_written_without_zero_filling() {
        let (c, header) = rgb_response(5, 3);
        let src = c.lock_as_image(|i| i.as_bytes().to_vec()).unwrap();
        let mut r = FrameReceiver::begin(&header, &FrameLimits::default()).unwrap();
        let (p, n) = unsafe { r.body_ptr(0) }.unwrap();
        assert_eq!(n, src.len());
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), p, n) };
        let got = r.finish().unwrap().into_capsule_ptr().unwrap();
        assert_eq!(got.extract_image().unwrap().as_bytes(), &src[..]);
    }

    /// 同じ大きさ・形式の画像があればその領域を使い、合わないものには触らない
    #[test]
    fn spare_images_are_reused_when_they_match() {
        let (c, header) = rgb_response(6, 4);
        let src = c.lock_as_image(|i| i.as_bytes().to_vec()).unwrap();
        let same = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(6, 4, Rgb([7, 7, 7])));
        let same_ptr = same.as_bytes().as_ptr() as usize;
        let other_size = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 6, Rgb([7, 7, 7])));
        let other_format = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(6, 4, Rgba([7, 7, 7, 7])));
        let mut spares = vec![other_size.clone(), same, other_format.clone()];
        let mut r = FrameReceiver::begin_reusing(&header, &FrameLimits::default(), &mut spares).unwrap();
        assert_eq!(spares.len(), 2);
        assert!(spares.contains(&other_size) && spares.contains(&other_format));
        let dst = r.body_mut(0).unwrap();
        assert_eq!(dst.as_ptr() as usize, same_ptr);
        dst.copy_from_slice(&src);
        let got = r.finish().unwrap().into_capsule_ptr().unwrap().extract_image().unwrap();
        assert_eq!(got.as_bytes().as_ptr() as usize, same_ptr);
        assert_eq!(got.as_bytes(), &src[..]);
    }

    #[test]
    fn take_spare_image_only_takes_unshared_plain_images() {
        let (c, _) = rgb_response(2, 2);
        let other = c.clone();
        assert!(take_spare_image(c).is_none(), "still referenced");
        assert!(take_spare_image(other).is_some());
        assert!(take_spare_image(jvalue!(1).into()).is_none());
        let mut spares = Vec::new();
        for i in 0..SPARE_IMAGES_MAX + 1 {
            keep_spare_image(&mut spares, DynamicImage::new_rgb8(i as u32 + 1, 1));
        }
        assert_eq!(spares.len(), SPARE_IMAGES_MAX);
        assert_eq!(spares[0].width(), 2, "the oldest is dropped");
    }
}
