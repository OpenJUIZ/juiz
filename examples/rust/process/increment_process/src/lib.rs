use juiz_sdk::prelude::*;

#[juiz_process]
fn increment_process(arg1: i64) -> JuizResult<Capsule> {
    log::trace!("increment_process({:?}) called", arg1);
    return Ok(jvalue!(arg1 + 1).into());
}
