//! 管理画面(webui)の埋め込み元を決める(issue #117)。
//!
//! src/lib.rs が環境変数 JUIZ_WEBUI_DIR のディレクトリをバイナリに埋め込む。探す順は次のとおり。
//!
//! 1. 環境変数 JUIZ_WEBUI_DIR(ビルドする人が明示したとき)
//! 2. juiz_webui/dist(crates.io に公開したクレート。公開ワークフローがビルド成果物を置く)
//! 3. ../webui/dist(リポジトリの中で `cd webui && npm run build` したとき)
//!
//! どれも無ければ埋め込まず、/ui は画面のビルド方法を案内するページになる。

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=JUIZ_WEBUI_DIR");
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let packaged = manifest_dir.join("dist");
    let in_repo = manifest_dir.join("..").join("webui").join("dist");
    let dir = match std::env::var_os("JUIZ_WEBUI_DIR") {
        Some(d) => PathBuf::from(d),
        None if packaged.join("index.html").exists() => packaged.clone(),
        None if in_repo.join("index.html").exists() => in_repo.clone(),
        None => packaged,
    };
    // 画面をビルドし直したら埋め込み直す(ディレクトリを渡すと中のファイルの変更も見る)。
    // 無いパスを渡すと毎回ビルドし直しになるので、あるものだけ渡す。
    if dir.exists() {
        println!("cargo:rerun-if-changed={}", dir.display());
    } else if let Some(webui) = in_repo.parent().filter(|w| w.exists()) {
        // リポジトリの中でまだ画面をビルドしていないときは、webui/dist ができたら気付けるよう webui/ を見る
        println!("cargo:rerun-if-changed={}", webui.display());
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=JUIZ_WEBUI_DIR={}", dir.display());
}
