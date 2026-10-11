use juiz_sdk::prelude::*;

#[juiz_process]
fn _arg_proc(arg1: String, arg2: String, arg3: String) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}, {arg3}) called");
    return Ok(jvalue!("OK!").into());
}
