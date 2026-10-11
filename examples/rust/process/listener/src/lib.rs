use juiz_sdk::prelude::*;

#[juiz_process(
    description = "This is listener process."
    arguments = {
        default = {
            arg1 = "Hello, Juiz!"
        }
        description = {
            arg1 = "This message is printed."
        }
    }
)]
fn listener(arg1: String) -> JuizResult<Capsule> {
    log::trace!("listener({:}) called", arg1);
    println!("listener: {:}", arg1);
    return Ok(jvalue!("Hello World").into());
}
