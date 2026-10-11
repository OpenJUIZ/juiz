use crate::filesystem::*;
use cv_convert::TryFromCv;
use juiz_sdk::{anyhow, image::DynamicImage, prelude::*};
use opencv::{
    core::{Mat, Vector},
    imgcodecs::imwrite,
};

#[juiz_component_container_process(
    container_type = "filesystem"
    arguments = {
        default = {
            file_name = "default_filename.png"
        }
    }
)]
fn filesystem_imwrite(
    _container: &mut ContainerImpl<CvFilesystem>,
    img: DynamicImage,
    file_name: String,
) -> JuizResult<Capsule> {
    println!("imwrite_function(file_name={file_name:})");

    let mat = Mat::try_from_cv(img)?;
    let params: Vector<i32> = Vector::new();
    match imwrite(file_name.as_str(), &mat, &params) {
        Ok(_) => {
            println!("ok");
            Ok(Capsule::empty())
        }
        Err(e) => {
            println!("error: {e:?}");
            Err(anyhow::Error::from(e))
        }
    }
}
