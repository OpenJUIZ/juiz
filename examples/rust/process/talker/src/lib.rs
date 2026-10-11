use juiz_sdk::prelude::*;

/// 文字列を出力します。
///
/// "Hello World" という文字列を出力します。
#[juiz_process(description = "This is talker process.")]
fn talker() -> JuizResult<Capsule> {
    log::trace!("talker() called");
    let string_value = "Hello World";
    println!("talker: {:}", string_value);
    return Ok(jvalue!(string_value).into());
}
