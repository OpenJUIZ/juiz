use juiz_sdk::prelude::*;

#[juiz_process]
fn fff_arg_proc(arg1: f64, arg2: f64, arg3: f64) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}, {arg3}) called");
    return Ok(jvalue!("OK!").into());
}
