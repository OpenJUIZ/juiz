//! 型 (Factory) の既定のモデル (ContainerModel Step 8、issue #387) のテスト用 Container
//! (juiz_core/tests/integration/container_model_default_test.rs)
//!
//! manifest に `model` を書く。パスはプラグインのファイル (libmodel_container.so など) のある
//! ディレクトリからの相対。負の数 (`z = -0.02`) も書ける。

use juiz_sdk::prelude::*;

pub struct ModelContainer {
    pub value: i64,
}

#[juiz_container(
    description = "Container with a default model",
    model = {
        path = "models/model_container.stl",
        scale = 0.001,
        color = "#336699",
        origin = { translation = { z = -0.02 } }
    }
)]
fn model_container(initial_value: i64) -> JuizResult<Box<ModelContainer>> {
    Ok(Box::new(ModelContainer {
        value: initial_value,
    }))
}
