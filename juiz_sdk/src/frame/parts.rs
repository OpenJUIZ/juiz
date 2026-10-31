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

/// 最終的な領域。画像の画素と数の配列の要素の型ごと
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

impl BodyBuffer {
    fn alloc(desc: &BodyDesc) -> JuizResult<Self> {
        Ok(match desc {
            BodyDesc::Image {
                width,
                height,
                format,
                ..
            } => {
                let pixels = *width as usize * *height as usize;
                let (channels, kind) = match format.as_str() {
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
                };
                let n = pixels * channels;
                match kind {
                    0 => BodyBuffer::U8(vec![0; n]),
                    1 => BodyBuffer::U16(vec![0; n]),
                    _ => BodyBuffer::F32(vec![0.0; n]),
                }
            }
            BodyDesc::Array { dtype, count, .. } => match dtype.as_str() {
                "i64le" => BodyBuffer::I64(vec![0; *count as usize]),
                "f64le" => BodyBuffer::F64(vec![0.0; *count as usize]),
                _ => return Err(anyhow!("unknown array dtype '{dtype}'")),
            },
        })
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
/// 相手は [`FrameReceiver::body_mut`] の領域へ直接書く。書き終わったら [`FrameReceiver::finish`] でフレームにする。
/// 画像のコピーは相手が書く 1 回だけ
pub struct FrameReceiver {
    header: FrameHeader,
    bodies: Vec<BodyBuffer>,
}

impl FrameReceiver {
    /// ヘッダ (先頭の "JZF1" と長さの無い JSON) を読み、本体の領域を確保する。
    /// 共有メモリの本体、余白のある画像、上限を超える大きさはエラー
    pub fn begin(header: &[u8], limits: &FrameLimits) -> JuizResult<Self> {
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
            .map(BodyBuffer::alloc)
            .collect::<JuizResult<Vec<_>>>()?;
        Ok(FrameReceiver { header, bodies })
    }

    /// 読んだヘッダ
    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    /// 本体の数
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// 本体 index の書き込み先 (長さはヘッダの説明どおり)
    pub fn body_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        self.bodies.get_mut(index).map(|b| b.as_mut_bytes())
    }

    /// 書き終わった本体とヘッダからフレームを作る
    pub fn finish(self) -> JuizResult<IncomingFrame> {
        let FrameReceiver { header, bodies } = self;
        let incoming = bodies
            .into_iter()
            .zip(header.bodies.iter())
            .map(|(b, desc)| b.into_body(desc).map(Some))
            .collect::<JuizResult<Vec<_>>>()?;
        Ok(IncomingFrame {
            header,
            bodies: incoming,
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
}
