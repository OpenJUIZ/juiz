use juiz_sdk::prelude::*;

#[juiz_process]
fn rust_listener(arg1: String) -> JuizResult<Capsule> {
    log::trace!("listener({:?}) called", arg1);
    println!("listener: {:}", arg1);
    return Ok(jvalue!("Hello World").into());
}
