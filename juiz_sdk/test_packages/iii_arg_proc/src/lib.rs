use juiz_sdk::prelude::*;

#[juiz_process]
fn iii_arg_proc(arg1: i64, arg2: i64, arg3: i64) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}, {arg3}) called");
    return Ok(jvalue!("OK!").into());
}
