use crate::convert::{image_to_mat, mat_to_image};
use juiz_sdk::{image::DynamicImage, prelude::*};
use opencv::{
    core::Mat,
    imgproc::{cvt_color_def, COLOR_BGR2RGB},
};

#[juiz_component_process]
pub(crate) fn cv_cvt_color(img: DynamicImage, code: String) -> JuizResult<Capsule> {
    println!("cvt_color_function called");
    Ok(bgr_to_rgb(&img)?.into())
}

/// 画像の B と R のチャンネルを入れ替える(COLOR_BGR2RGB)
pub(crate) fn bgr_to_rgb(img: &DynamicImage) -> JuizResult<DynamicImage> {
    let mat = image_to_mat(img)?;
    let mut out_img = Mat::default();
    cvt_color_def(&mat, &mut out_img, COLOR_BGR2RGB)?;
    mat_to_image(&out_img)
}
