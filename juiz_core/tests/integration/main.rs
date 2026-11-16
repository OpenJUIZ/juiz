//! juiz_core の統合テスト
//!
//! テストの置き場所・名前・カテゴリ・絞り込み方の決まりは CONTRIBUTING.md の「テスト」の節を見ること
//! (機能ごとのディレクトリに分け、言語はその下。ファイル名に `_test`、関数名に `test_` / `_test` は付けない)。
//!
//! テストファイルごとに別のバイナリにすると、それぞれが juiz_core と依存クレートを丸ごとリンクして
//! (1 本 ~110MB)、lib を少し変えただけでも再リンクに時間がかかる。そこで tests/integration/ の下の
//! テストをすべてこの 1 本のバイナリにまとめている。新しいテストは機能のディレクトリにファイルを置き、
//! その `mod.rs` に `mod` を足すこと(tests/ 直下に置くと別のバイナリになる)。
//!
//! 一部だけ実行するときはモジュールパスで絞り込む。
//!
//! ```sh
//! scripts/test.sh -p juiz_core --test integration -- core::system::
//! scripts/test.sh -p juiz_core --test integration -- transformations::
//! ```
//!
//! 1 つのプロセスで全テストが走るので、logger の設定は `common::logger::init()` に揃えること。
//! Python のプラグインなどは tests/python と tests/confs から読む(パスは変わらない)。

mod common;

mod bench;
mod brokers;
mod composition;
mod container_model;
mod core;
mod datatype;
mod plugins;
mod pyjuiz;
mod transformations;
