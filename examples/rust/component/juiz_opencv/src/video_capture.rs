use juiz_sdk::prelude::*;
use opencv::{
    core::Mat,
    videoio::{VideoCapture, CAP_ANY},
};

#[repr(Rust)]
pub struct CvVideoCapture {
    pub camera: VideoCapture,
    pub image: Option<Mat>,
}

#[juiz_component_container]
fn video_capture(camera_id: i64) -> JuizResult<Box<CvVideoCapture>> {
    log::trace!("create_cv_capture_container({:?}) called", camera_id);
    let cam = VideoCapture::new(camera_id as i32, CAP_ANY)?; // 0 is the default camera
    Ok(Box::new(CvVideoCapture {
        camera: cam,
        image: Some(Mat::default()),
    }))
}
