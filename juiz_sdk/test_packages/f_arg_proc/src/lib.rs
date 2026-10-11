use juiz_sdk::prelude::*;

#[juiz_process]
fn f_arg_proc(arg1: f64) -> JuizResult<Capsule> {
    log::trace!("process({arg1}) called");
    return Ok(jvalue!("OK!").into());
}
