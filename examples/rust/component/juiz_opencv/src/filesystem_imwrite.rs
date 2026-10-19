use crate::convert::image_to_mat;
use crate::filesystem::*;
use juiz_sdk::{anyhow, image::DynamicImage, prelude::*};
use opencv::{core::Vector, imgcodecs::imwrite};

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

    match write_image(&img, file_name.as_str()) {
        Ok(()) => {
            println!("ok");
            Ok(Capsule::empty())
        }
        Err(e) => {
            println!("error: {e:?}");
            Err(e)
        }
    }
}

/// 画像をファイルに書き出す。形式は拡張子から OpenCV が決める。
pub(crate) fn write_image(img: &DynamicImage, file_name: &str) -> JuizResult<()> {
    let mat = image_to_mat(img)?;
    let params: Vector<i32> = Vector::new();
    if imwrite(file_name, &mat, &params)? {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "imwrite が {file_name} を書き出せませんでした"
        ))
    }
}
