use crate::window::*;
use cv_convert::TryFromCv;
use juiz_sdk::{anyhow, image::DynamicImage, prelude::*};
use opencv::{
    core::Mat,
    highgui::{imshow, wait_key},
};

#[juiz_component_container_process(container_type = "window")]
fn window_show(container: &mut ContainerImpl<CvWindow>, img: DynamicImage) -> JuizResult<Capsule> {
    let window_name = container.name.as_str();
    println!("imshow_function(name={window_name:})");
    let mat = Mat::try_from_cv(img)?;
    match imshow(window_name, &mat) {
        Ok(()) => {
            println!("ok");
            wait_key(0)?;
            Ok(Capsule::empty())
        }
        Err(e) => {
            println!("error: {e:?}");
            Err(anyhow::Error::from(e))
        }
    }
}
