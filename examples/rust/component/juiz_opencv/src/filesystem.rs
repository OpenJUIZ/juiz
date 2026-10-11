use juiz_sdk::prelude::*;

#[repr(Rust)]
pub struct CvFilesystem {}

#[juiz_component_container]
fn filesystem() -> JuizResult<Box<CvFilesystem>> {
    Ok(Box::new(CvFilesystem {}))
}
