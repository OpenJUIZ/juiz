use juiz_sdk::prelude::*;

#[juiz_process]
fn _arg_proc(arg1: String, arg2: String) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}) called");
    return Ok(jvalue!("OK!").into());
}
