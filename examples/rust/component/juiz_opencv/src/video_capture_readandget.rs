

use juiz_sdk::{image::DynamicImage, prelude::*};
use opencv::{core::{AlgorithmHint, Mat}, imgproc::{cvt_color, COLOR_BGR2RGB}, videoio::VideoCaptureTrait};
use crate::video_capture::*;

use cv_convert::TryFromCv;

#[juiz_component_container_process(
    container_type = "video_capture"
)]
fn video_capture_readandget(container: &mut ContainerImpl<CvVideoCapture>) -> JuizResult<Capsule> {
    println!("video_capture_readandget() called");
    let mut frame : Mat = Mat::default();
    let mut dst: Mat = Mat::default();
    container.camera.read(&mut frame)?;
    cvt_color(&frame, &mut dst, COLOR_BGR2RGB, 0, AlgorithmHint::ALGO_HINT_DEFAULT)?;
    let img : DynamicImage = DynamicImage::try_from_cv(dst)?;
    let value: Capsule = img.into();
    return Ok(value);
}
