use juiz_sdk::prelude::*;
use opencv::highgui::named_window;

#[allow(dead_code)]
#[repr(Rust)]
pub struct CvWindow {
    pub name: String,
}

#[juiz_component_container]
fn window(name: String) -> JuizResult<Box<CvWindow>> {
    named_window(name.as_str(), 1)?;
    Ok(Box::new(CvWindow { name }))
}
