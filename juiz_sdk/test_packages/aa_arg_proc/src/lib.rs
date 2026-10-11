use juiz_sdk::prelude::*;

#[juiz_process]
fn _arg_proc(arg1: Vec<Value>, arg2: Vec<Value>) -> JuizResult<Capsule> {
    log::trace!("process({arg1:?}, {arg2:?}) called");
    return Ok(arg1.into());
}
