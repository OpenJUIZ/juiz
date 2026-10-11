use juiz_sdk::prelude::*;

#[juiz_process]
fn _arg_proc(arg1: Vec<f64>) -> JuizResult<Capsule> {
    log::trace!("process({arg1:?}) called");
    return Ok(jvalue!("OK!").into());
}
