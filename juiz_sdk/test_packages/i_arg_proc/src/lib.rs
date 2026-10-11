use juiz_sdk::prelude::*;

#[juiz_process]
fn i_arg_proc(arg1: i64) -> JuizResult<Capsule> {
    log::trace!("process({arg1}) called");
    return Ok(jvalue!("OK!").into());
}
