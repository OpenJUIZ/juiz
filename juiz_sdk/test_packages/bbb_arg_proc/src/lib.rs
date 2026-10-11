use juiz_sdk::prelude::*;

#[juiz_process]
fn fff_arg_proc(arg1: bool, arg2: bool, arg3: bool) -> JuizResult<Capsule> {
    log::trace!("process({arg1}, {arg2}, {arg3}) called");
    return Ok(jvalue!("OK!").into());
}
