use crate::convert::mat_to_image;
use crate::video_capture::*;
use juiz_sdk::prelude::*;
use opencv::{
    core::Mat,
    imgproc::{cvt_color_def, COLOR_BGR2RGB},
    videoio::VideoCaptureTrait,
};

#[juiz_component_container_process(container_type = "video_capture")]
fn video_capture_readandget(container: &mut ContainerImpl<CvVideoCapture>) -> JuizResult<Capsule> {
    println!("video_capture_readandget() called");
    let mut frame: Mat = Mat::default();
    let mut dst: Mat = Mat::default();
    container.camera.read(&mut frame)?;
    cvt_color_def(&frame, &mut dst, COLOR_BGR2RGB)?;
    let img = mat_to_image(&dst)?;
    let value: Capsule = img.into();
    return Ok(value);
}
