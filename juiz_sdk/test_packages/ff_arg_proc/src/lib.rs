use juiz_sdk::prelude::*;

#[juiz_process]
fn ff_arg_proc(arg1: f64, arg2: f64) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}) called");
    return Ok(jvalue!("OK!").into());
}
