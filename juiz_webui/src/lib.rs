//! juiz の管理画面(webui)のビルド成果物を埋め込むクレート(issue #117)。
//!
//! juiz_core の feature "webui" がこれを使い、HTTP ブローカーの /ui で配信する。
//! 埋め込み元は build.rs が決める。どれも無ければ何も埋め込まない([`embedded`] が false)。

use rust_embed::RustEmbed;

pub use rust_embed::EmbeddedFile;

#[derive(RustEmbed)]
#[folder = "$JUIZ_WEBUI_DIR"]
#[allow_missing = true]
struct Assets;

/// 画面のファイルを取り出す(path は "index.html" や "assets/index-xxxx.js" のような相対パス)
pub fn get(path: &str) -> Option<EmbeddedFile> {
    Assets::get(path)
}

/// 画面のビルド成果物が埋め込まれているか
pub fn embedded() -> bool {
    Assets::get("index.html").is_some()
}
