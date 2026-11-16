//! qmp ブローカーのテスト
//!
//! どのブローカーでも成り立つ確認の中身はブローカー共通のスイート juiz_broker_suite にあり (issue #522)、
//! ここではそれを qmp で走らせる分と、qmp だけの確認を置く。
//! テストの置き場所・名前の決まりは CONTRIBUTING.md の「テスト」の節を見ること。
//! tests/ 直下のファイルは 1 本ずつ別のバイナリになり、それぞれがクレートと依存を丸ごとリンクするので、
//! テストはすべてこの 1 本のバイナリにまとめている (issue #523)。新しいテストはこのディレクトリに
//! ファイルを置き、ここに `mod` を足すこと。
//!
//! ```sh
//! cargo test -p juiz_qmp_broker --test broker -- profile_json:: --test-threads=1
//! ```
//!
//! テストは固定のポートを使うので `--test-threads=1` で走らせる。

mod common;

mod bench;

mod api;
mod builtin;
mod composite;
mod container_model;
mod datatype;
mod disconnect;
mod namespace;
mod profile_json;
mod transformations;
