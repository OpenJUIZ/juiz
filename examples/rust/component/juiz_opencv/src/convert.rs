//! DynamicImage と OpenCV の Mat の相互変換
//!
//! 画素のバイト列をそのままコピーし、チャンネルの並び(RGB / BGR)は入れ替えない。
//! 並びを変えたい場合は cvt_color を使う。
use juiz_sdk::{anyhow::anyhow, image::DynamicImage, prelude::*};
use opencv::{
    core::{Mat, Scalar, CV_8U, CV_8UC1, CV_8UC3, CV_8UC4},
    prelude::*,
};

/// DynamicImage を Mat に変換する。
///
/// グレースケール(8bit)は CV_8UC1、RGBA(8bit)は CV_8UC4、それ以外は RGB(8bit)にしてから CV_8UC3 にする。
pub fn image_to_mat(img: &DynamicImage) -> JuizResult<Mat> {
    let (typ, width, height, bytes): (i32, u32, u32, std::borrow::Cow<[u8]>) = match img {
        DynamicImage::ImageLuma8(buf) => (CV_8UC1, buf.width(), buf.height(), buf.as_raw().into()),
        DynamicImage::ImageRgb8(buf) => (CV_8UC3, buf.width(), buf.height(), buf.as_raw().into()),
        DynamicImage::ImageRgba8(buf) => (CV_8UC4, buf.width(), buf.height(), buf.as_raw().into()),
        _ => {
            let buf = img.to_rgb8();
            (CV_8UC3, buf.width(), buf.height(), buf.into_raw().into())
        }
    };
    let mut mat =
        Mat::new_rows_cols_with_default(height as i32, width as i32, typ, Scalar::all(0.0))?;
    mat.data_bytes_mut()?.copy_from_slice(&bytes);
    Ok(mat)
}

/// Mat を DynamicImage に変換する。
///
/// 8bit で 1 / 3 / 4 チャンネルの Mat に対応し、それぞれ Luma8 / Rgb8 / Rgba8 になる。
pub fn mat_to_image(mat: &impl MatTraitConst) -> JuizResult<DynamicImage> {
    if mat.depth() != CV_8U {
        return Err(anyhow!(
            "mat_to_image: 8bit 以外の Mat (depth={}) には対応していません",
            mat.depth()
        ));
    }
    let width = mat.cols() as u32;
    let height = mat.rows() as u32;
    // data_bytes() は連続したメモリを要求するので、ROI などで連続していなければコピーする
    let bytes = if mat.is_continuous() {
        mat.data_bytes()?.to_vec()
    } else {
        mat.try_clone()?.data_bytes()?.to_vec()
    };
    let not_match = || anyhow!("mat_to_image: 画素数とデータの大きさが一致しません");
    let img = match mat.channels() {
        1 => DynamicImage::ImageLuma8(
            juiz_sdk::image::GrayImage::from_raw(width, height, bytes).ok_or_else(not_match)?,
        ),
        3 => DynamicImage::ImageRgb8(
            juiz_sdk::image::RgbImage::from_raw(width, height, bytes).ok_or_else(not_match)?,
        ),
        4 => DynamicImage::ImageRgba8(
            juiz_sdk::image::RgbaImage::from_raw(width, height, bytes).ok_or_else(not_match)?,
        ),
        c => {
            return Err(anyhow!(
                "mat_to_image: {c} チャンネルの Mat には対応していません"
            ))
        }
    };
    Ok(img)
}
