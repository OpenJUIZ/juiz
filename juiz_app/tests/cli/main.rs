//! juiz コマンドを子プロセスで動かすテスト
//!
//! テストの置き場所・名前の決まりは CONTRIBUTING.md の「テスト」の節を見ること。
//! tests/ 直下のファイルは 1 本ずつ別のバイナリになり、それぞれが juiz_app と依存を丸ごとリンクする
//! (1 本 100MB を超える)ので、テストはすべてこの 1 本のバイナリにまとめている (issue #523)。
//! 新しいテストはこのディレクトリにファイルを置き、ここに `mod` を足すこと。
//!
//! ```sh
//! scripts/test.sh -p juiz_app --test cli -- package::
//! cargo test -p juiz_app --features zenoh-broker --test cli -- zenoh::
//! cargo test -p juiz_app --features naming-mdns --test cli -- mdns::
//! ```
//!
//! 例外: feature python-entry のテストは埋め込み Python を使うので、別のバイナリ
//! (tests/python_entry_test.rs) のまま。
//!
//! テストは固定のポートを使うので `--test-threads=1` で走らせる(scripts/test.sh が付ける)。

mod common;

mod app;
mod broker_plugin_features;
mod connection;
mod datatype;
mod model;
mod mdns;
mod module;
mod name;
mod name_target;
mod namespace;
mod package;
mod peers;
mod pkg;
mod plugin_path;
mod remote;
mod sdk;
mod setup;
mod transformations;
mod transformations_cross_system;
mod zenoh;
