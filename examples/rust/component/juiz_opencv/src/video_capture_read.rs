use crate::video_capture::*;
use juiz_sdk::prelude::*;
use opencv::{core::Mat, videoio::VideoCaptureTrait};

#[juiz_component_container_process(container_type = "video_capture")]
fn video_capture_read(container: &mut ContainerImpl<CvVideoCapture>) -> JuizResult<Capsule> {
    let mut frame: Mat = container.image.take().unwrap();
    container.camera.read(&mut frame)?;
    container.image = Some(frame);
    return Ok(jvalue!(true).into());
}
