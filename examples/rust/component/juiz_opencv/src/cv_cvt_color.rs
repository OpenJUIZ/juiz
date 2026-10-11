use cv_convert::{TryFromCv, TryIntoCv};
use juiz_sdk::anyhow;
use juiz_sdk::{image::DynamicImage, prelude::*};
use opencv::{
    core::{AlgorithmHint, Mat, ALGO_HINT_DEFAULT},
    imgproc::{cvt_color, COLOR_BGR2RGB},
};

#[juiz_component_process]
fn cv_cvt_color(img: DynamicImage, code: String) -> JuizResult<Capsule> {
    println!("cvt_color_function called");
    let mat = Mat::try_from_cv(img)?;
    let mut out_img = Mat::default();
    match cvt_color(
        &mat,
        &mut out_img,
        COLOR_BGR2RGB,
        0,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    ) {
        Ok(()) => {
            let out_img2: DynamicImage = out_img.try_into_cv()?;
            Ok(out_img2.into())
        }
        Err(e) => Err(anyhow::Error::from(e)),
    }
}
