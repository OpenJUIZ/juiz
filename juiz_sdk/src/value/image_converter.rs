//! C ABI のプラグイン (C++ / Haskell) 向けの、画像 (`Capsule::Image`) の関数 (issue #253、仕様 docs/spec/large-data.md §10)
//!
//! - `capsule_ptr_lock_as_image`: 引数の画像を、複製せずに callback の間だけ貸す。
//! - `capsule_set_image`: プラグインの画像を複製して戻り値に入れる。行の余白 (stride) を取り除き、
//!   `bgr8` / `bgra8` はチャンネルを並べ替えて `rgb8` / `rgba8` にする (DynamicImage に BGR の形式がないため)。
//!
//! 宣言は bindings/cjuiz/include/juiz_sdk.h と合わせること。

use std::ffi::c_void;

use image::{DynamicImage, ImageBuffer};

use super::converter_error::*;
use crate::prelude::*;

/// 画像の形式・大きさ・stride が正しくない、または扱えない形式
pub const JUIZ_IMAGE_FORMAT_ERROR: i64 = -704;

/// 画素の形式 (juiz_sdk.h の JUIZ_IMAGE_FORMAT_*)。0〜9 は large-data.md §3.4 の表と同じ並び
pub const JUIZ_IMAGE_FORMAT_L8: u32 = 0;
pub const JUIZ_IMAGE_FORMAT_LA8: u32 = 1;
pub const JUIZ_IMAGE_FORMAT_RGB8: u32 = 2;
pub const JUIZ_IMAGE_FORMAT_RGBA8: u32 = 3;
pub const JUIZ_IMAGE_FORMAT_L16: u32 = 4;
pub const JUIZ_IMAGE_FORMAT_LA16: u32 = 5;
pub const JUIZ_IMAGE_FORMAT_RGB16: u32 = 6;
pub const JUIZ_IMAGE_FORMAT_RGBA16: u32 = 7;
pub const JUIZ_IMAGE_FORMAT_RGB32F: u32 = 8;
pub const JUIZ_IMAGE_FORMAT_RGBA32F: u32 = 9;
/// 送るときだけ使える。`rgb8` に並べ替えて入れる
pub const JUIZ_IMAGE_FORMAT_BGR8: u32 = 10;
/// 送るときだけ使える。`rgba8` に並べ替えて入れる
pub const JUIZ_IMAGE_FORMAT_BGRA8: u32 = 11;

/// juiz_sdk.h の juiz_image。画素はホストのバイト順 (16 bit・f32 の形式)
#[repr(C)]
pub struct JuizImage {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub reserved: u32,
    /// 1 行のバイト数。`width × 1 画素のバイト数` 以上
    pub stride: usize,
    pub data: *const u8,
}

/// 1 画素のバイト数。知らない形式なら None
pub fn image_format_bytes_per_pixel(format: u32) -> Option<usize> {
    Some(match format {
        JUIZ_IMAGE_FORMAT_L8 => 1,
        JUIZ_IMAGE_FORMAT_LA8 | JUIZ_IMAGE_FORMAT_L16 => 2,
        JUIZ_IMAGE_FORMAT_RGB8 | JUIZ_IMAGE_FORMAT_BGR8 => 3,
        JUIZ_IMAGE_FORMAT_RGBA8 | JUIZ_IMAGE_FORMAT_BGRA8 | JUIZ_IMAGE_FORMAT_LA16 => 4,
        JUIZ_IMAGE_FORMAT_RGB16 => 6,
        JUIZ_IMAGE_FORMAT_RGBA16 => 8,
        JUIZ_IMAGE_FORMAT_RGB32F => 12,
        JUIZ_IMAGE_FORMAT_RGBA32F => 16,
        _ => return None,
    })
}

/// DynamicImage の形式。large-data.md §3.4 の表にない形式 (image クレートが足したもの) は None
fn format_of(img: &DynamicImage) -> Option<u32> {
    use DynamicImage::*;
    Some(match img {
        ImageLuma8(_) => JUIZ_IMAGE_FORMAT_L8,
        ImageLumaA8(_) => JUIZ_IMAGE_FORMAT_LA8,
        ImageRgb8(_) => JUIZ_IMAGE_FORMAT_RGB8,
        ImageRgba8(_) => JUIZ_IMAGE_FORMAT_RGBA8,
        ImageLuma16(_) => JUIZ_IMAGE_FORMAT_L16,
        ImageLumaA16(_) => JUIZ_IMAGE_FORMAT_LA16,
        ImageRgb16(_) => JUIZ_IMAGE_FORMAT_RGB16,
        ImageRgba16(_) => JUIZ_IMAGE_FORMAT_RGBA16,
        ImageRgb32F(_) => JUIZ_IMAGE_FORMAT_RGB32F,
        ImageRgba32F(_) => JUIZ_IMAGE_FORMAT_RGBA32F,
        _ => return None,
    })
}

/// 行の余白を取り除き、1 チャンネルずつホストのバイト順で読んだ値の列にする
fn packed_channels<T: Copy, const N: usize>(
    src: &[u8],
    row_bytes: usize,
    stride: usize,
    height: usize,
    from: fn([u8; N]) -> T,
) -> Vec<T> {
    let mut out = Vec::with_capacity(row_bytes / N * height);
    for y in 0..height {
        let row = &src[y * stride..y * stride + row_bytes];
        out.extend(row.chunks_exact(N).map(|c| from(c.try_into().unwrap())));
    }
    out
}

/// juiz_image から DynamicImage を作る (複製する)。合わなければ Err (理由)
pub unsafe fn image_from_c(img: &JuizImage) -> Result<DynamicImage, String> {
    let bpp = image_format_bytes_per_pixel(img.format)
        .ok_or_else(|| format!("unknown image format {}", img.format))?;
    let (w, h) = (img.width as usize, img.height as usize);
    let row_bytes = w * bpp;
    if img.stride < row_bytes {
        return Err(format!(
            "stride ({}) must be width x bytes per pixel ({row_bytes}) or more",
            img.stride
        ));
    }
    if img.data.is_null() && row_bytes * h > 0 {
        return Err("data is NULL".to_owned());
    }
    let len = if h == 0 { 0 } else { img.stride * (h - 1) + row_bytes };
    let src: &[u8] = if len == 0 { &[] } else { std::slice::from_raw_parts(img.data, len) };
    let u8s = || packed_channels::<u8, 1>(src, row_bytes, img.stride, h, |b| b[0]);
    let u16s = || packed_channels::<u16, 2>(src, row_bytes, img.stride, h, u16::from_ne_bytes);
    let f32s = || packed_channels::<f32, 4>(src, row_bytes, img.stride, h, f32::from_ne_bytes);
    let (w, h) = (img.width, img.height);
    // 大きさは上で確かめたので from_raw は失敗しない
    let built = match img.format {
        JUIZ_IMAGE_FORMAT_L8 => ImageBuffer::from_raw(w, h, u8s()).map(DynamicImage::ImageLuma8),
        JUIZ_IMAGE_FORMAT_LA8 => ImageBuffer::from_raw(w, h, u8s()).map(DynamicImage::ImageLumaA8),
        JUIZ_IMAGE_FORMAT_RGB8 => ImageBuffer::from_raw(w, h, u8s()).map(DynamicImage::ImageRgb8),
        JUIZ_IMAGE_FORMAT_RGBA8 => ImageBuffer::from_raw(w, h, u8s()).map(DynamicImage::ImageRgba8),
        JUIZ_IMAGE_FORMAT_BGR8 => {
            let mut v = u8s();
            v.chunks_exact_mut(3).for_each(|p| p.swap(0, 2));
            ImageBuffer::from_raw(w, h, v).map(DynamicImage::ImageRgb8)
        }
        JUIZ_IMAGE_FORMAT_BGRA8 => {
            let mut v = u8s();
            v.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
            ImageBuffer::from_raw(w, h, v).map(DynamicImage::ImageRgba8)
        }
        JUIZ_IMAGE_FORMAT_L16 => ImageBuffer::from_raw(w, h, u16s()).map(DynamicImage::ImageLuma16),
        JUIZ_IMAGE_FORMAT_LA16 => ImageBuffer::from_raw(w, h, u16s()).map(DynamicImage::ImageLumaA16),
        JUIZ_IMAGE_FORMAT_RGB16 => ImageBuffer::from_raw(w, h, u16s()).map(DynamicImage::ImageRgb16),
        JUIZ_IMAGE_FORMAT_RGBA16 => ImageBuffer::from_raw(w, h, u16s()).map(DynamicImage::ImageRgba16),
        JUIZ_IMAGE_FORMAT_RGB32F => ImageBuffer::from_raw(w, h, f32s()).map(DynamicImage::ImageRgb32F),
        JUIZ_IMAGE_FORMAT_RGBA32F => ImageBuffer::from_raw(w, h, f32s()).map(DynamicImage::ImageRgba32F),
        _ => None,
    };
    built.ok_or_else(|| "image buffer size mismatch".to_owned())
}

/// capsule_ptr の値が画像か (Native の画像も含む)
#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_is_image(capsule_ptr: *mut CapsulePtr) -> bool {
    match capsule_ptr.as_ref() {
        Some(cp) => cp.is_image().unwrap_or(false),
        None => false,
    }
}

/// capsule_ptr の画像を、複製せずに callback の間だけ juiz_image として貸す (callback の間はロックされる)。
/// 画像でなければ JUIZ_CAPSULE_TYPE_ERROR、§3.4 の表にない形式なら JUIZ_IMAGE_FORMAT_ERROR。
/// そうでなければ callback の戻り値が返る
#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_lock_as_image(
    capsule_ptr: *mut CapsulePtr,
    callback: extern "C" fn(*mut c_void, *const JuizImage) -> i64,
    arg: *mut c_void,
) -> i64 {
    let Some(cp) = capsule_ptr.as_ref() else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let r = cp.lock_as_image(|img| {
        let Some(format) = format_of(img) else {
            log::error!("capsule_ptr_lock_as_image() failed. unsupported image format {:?}", img.color());
            return JUIZ_IMAGE_FORMAT_ERROR;
        };
        let bytes = img.as_bytes();
        let c = JuizImage {
            width: img.width(),
            height: img.height(),
            format,
            reserved: 0,
            stride: img.width() as usize * image_format_bytes_per_pixel(format).unwrap(),
            data: bytes.as_ptr(),
        };
        callback(arg, &c)
    });
    r.unwrap_or(JUIZ_CAPSULE_TYPE_ERROR)
}

/// juiz_image を複製して capsule (Process の戻り値) に入れる。
/// 合わなければ JUIZ_IMAGE_FORMAT_ERROR (理由はログに出す)
#[no_mangle]
pub unsafe extern "C" fn capsule_set_image(capsule: *mut Capsule, image: *const JuizImage) -> i64 {
    let (Some(cap), Some(img)) = (capsule.as_mut(), image.as_ref()) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    match image_from_c(img) {
        Ok(d) => {
            *cap = Capsule::from(d);
            JUIZ_OK
        }
        Err(e) => {
            log::error!("capsule_set_image() failed. {e}");
            JUIZ_IMAGE_FORMAT_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    extern "C" fn copy_out(arg: *mut c_void, img: *const JuizImage) -> i64 {
        let (out, img) = unsafe { (&mut *(arg as *mut Vec<u8>), &*img) };
        let len = img.stride * img.height as usize;
        out.extend_from_slice(unsafe { std::slice::from_raw_parts(img.data, len) });
        img.format as i64
    }

    #[test]
    fn bgr_with_padding_becomes_rgb() {
        // 2x2 の bgr8、1 行 8 バイト (余白 2 バイト)
        let data: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 0xEE, 0xEE, 7, 8, 9, 10, 11, 12, 0xEE, 0xEE];
        let c = JuizImage { width: 2, height: 2, format: JUIZ_IMAGE_FORMAT_BGR8, reserved: 0, stride: 8, data: data.as_ptr() };
        let mut cap = Capsule::empty();
        assert_eq!(unsafe { capsule_set_image(&mut cap, &c) }, JUIZ_OK);
        let img = cap.as_image().unwrap().clone();
        assert_eq!(img.get_pixel(0, 0).0[..3], [3, 2, 1]);
        assert_eq!(img.get_pixel(1, 1).0[..3], [12, 11, 10]);

        let mut cp = CapsulePtr::from(img);
        let mut out: Vec<u8> = Vec::new();
        let r = unsafe { capsule_ptr_lock_as_image(&mut cp, copy_out, &mut out as *mut _ as *mut c_void) };
        assert_eq!(r, JUIZ_IMAGE_FORMAT_RGB8 as i64);
        assert_eq!(out, vec![3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
    }

    #[test]
    fn sixteen_bit_is_host_order() {
        let px: Vec<u16> = vec![1, 0x1234, 0xFFFF, 7];
        let bytes: Vec<u8> = px.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let c = JuizImage { width: 2, height: 2, format: JUIZ_IMAGE_FORMAT_L16, reserved: 0, stride: 4, data: bytes.as_ptr() };
        let img = unsafe { image_from_c(&c) }.unwrap();
        assert_eq!(img.as_luma16().unwrap().as_raw(), &px);
    }

    #[test]
    fn bad_images_are_rejected() {
        let data = vec![0u8; 16];
        let mut cap = Capsule::empty();
        let short = JuizImage { width: 2, height: 2, format: JUIZ_IMAGE_FORMAT_RGB8, reserved: 0, stride: 5, data: data.as_ptr() };
        assert_eq!(unsafe { capsule_set_image(&mut cap, &short) }, JUIZ_IMAGE_FORMAT_ERROR);
        let unknown = JuizImage { width: 2, height: 2, format: 99, reserved: 0, stride: 8, data: data.as_ptr() };
        assert_eq!(unsafe { capsule_set_image(&mut cap, &unknown) }, JUIZ_IMAGE_FORMAT_ERROR);
        let mut not_image = CapsulePtr::from(jvalue!(1));
        let mut out: Vec<u8> = Vec::new();
        let r = unsafe { capsule_ptr_lock_as_image(&mut not_image, copy_out, &mut out as *mut _ as *mut c_void) };
        assert_eq!(r, JUIZ_CAPSULE_TYPE_ERROR);
    }
}
