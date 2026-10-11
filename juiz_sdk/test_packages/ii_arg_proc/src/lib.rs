use juiz_sdk::prelude::*;

#[juiz_process]
fn ii_arg_proc(arg1: i64, arg2: i64) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}) called");
    return Ok(jvalue!("OK!").into());
}
