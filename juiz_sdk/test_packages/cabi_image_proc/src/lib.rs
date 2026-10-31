//! Rust プラグインの C ABI で画像の引数と戻り値が運べることを確かめるためのプラグイン
//! (docs/spec/rust-plugin-abi.md §5.2・§5.3、issue #330)。画像の色を反転して返す
use juiz_sdk::prelude::*;

#[juiz_process]
fn cabi_image_proc(img: DynamicImage) -> JuizResult<Capsule> {
    let mut img = img;
    img.invert();
    Ok(img.into())
}
