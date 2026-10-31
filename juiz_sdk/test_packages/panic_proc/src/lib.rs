//! Rust プラグインの C ABI で、利用者の関数の panic がホストまで届かないことを確かめるためのプラグイン
//! (docs/spec/rust-plugin-abi.md §6.2、issue #330)。arg1 が負なら panic し、0 なら Err を返す
use juiz_sdk::prelude::*;

#[juiz_process]
fn panic_proc(arg1: i64) -> JuizResult<Capsule> {
    if arg1 < 0 {
        panic!("arg1 is negative ({arg1})");
    }
    if arg1 == 0 {
        return Err(juiz_sdk::anyhow::anyhow!(JuizError::InvalidValueError {
            message: "arg1 must not be 0".to_owned()
        }));
    }
    Ok(jvalue!(arg1 * 2).into())
}
