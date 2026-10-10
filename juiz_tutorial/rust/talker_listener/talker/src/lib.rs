
use juiz_sdk::prelude::*;

use juiz_sdk::prelude::juiz_process;

#[juiz_process]
fn rust_talker() -> JuizResult<Capsule> {
    log::debug!("talker() called");
    let string_value = "Hello World";
    log::info!("talker: {:}", string_value);
    return Ok(jvalue!(string_value).into());
}
