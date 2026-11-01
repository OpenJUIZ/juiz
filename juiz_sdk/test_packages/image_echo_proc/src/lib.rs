//! Rust プラグインの C ABI と Rust ABI で、呼び出しにかかる時間を比べるためのプラグイン
//! (juiz_core/tests/integration/rust_abi_bench.rs、issue #333)。画像をそのまま返す
use juiz_sdk::prelude::*;

#[juiz_process]
fn image_echo_proc(img: DynamicImage) -> JuizResult<Capsule> {
    Ok(img.into())
}
