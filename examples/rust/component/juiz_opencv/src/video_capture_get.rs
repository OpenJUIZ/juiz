use crate::convert::mat_to_image;
use crate::video_capture::*;
use juiz_sdk::prelude::*;

#[juiz_component_container_process(container_type = "video_capture")]
fn video_capture_get(container: &mut ContainerImpl<CvVideoCapture>) -> JuizResult<Capsule> {
    let img = mat_to_image(container.image.as_ref().unwrap())?;
    let value: Capsule = img.into();
    return Ok(value);
}
