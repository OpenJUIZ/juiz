use juiz_sdk::prelude::*;

#[juiz_process]
fn _arg_proc(arg1: bool, arg2: bool) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}) called");
    return Ok(jvalue!("OK!").into());
}
