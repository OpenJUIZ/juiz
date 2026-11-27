//! 試験用の TCP ブローカー (C ABI) のテスト (docs/spec/broker-plugin-abi.md、issue #636)
//!
//! どのブローカーでも成り立つ確認はブローカー共通のスイート juiz_broker_suite にあり (issue #522)、
//! ここではそれを C ABI のブローカーで走らせる。C ABI のブローカーはフレームの処理を juiz_core の
//! 共通の処理 (juiz_core::cabi) に任せるので、そこもこれで通る。C ABI だけの確認は cabi.rs に置く。
//! テストの置き場所・名前の決まりは CONTRIBUTING.md の「テスト」の節を見ること。
//!
//! ```sh
//! cargo test -p juiz_tcp_test_broker --test broker -- cabi:: --test-threads=1
//! ```
//!
//! テストは固定のポートを使うので `--test-threads=1` で走らせる。プラグイン (tcp_test_broker、Rust の例) と
//! juiz コマンドは cargo build で ../../target/debug にビルドされている前提 (CI の test-*.yml と同じ)。

mod common;

mod api;
mod builtin;
mod cabi;
mod composite;
mod container_model;
mod datatype;
mod namespace;
mod transformations;
