use crate::video_capture::*;
use juiz_sdk::{image::DynamicImage, prelude::*};

use cv_convert::TryFromCv;

#[juiz_component_container_process(container_type = "video_capture")]
fn video_capture_get(container: &mut ContainerImpl<CvVideoCapture>) -> JuizResult<Capsule> {
    let img: DynamicImage = DynamicImage::try_from_cv(container.image.clone().unwrap())?;
    let value: Capsule = img.into();
    return Ok(value);
}
