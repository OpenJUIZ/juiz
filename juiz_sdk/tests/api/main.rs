//! juiz_sdk の公開 API のテスト
//!
//! テストの置き場所・名前の決まりは CONTRIBUTING.md の「テスト」の節を見ること。
//! tests/ 直下のファイルは 1 本ずつ別のバイナリになり、それぞれがクレートと依存を丸ごとリンクするので、
//! テストはすべてこの 1 本のバイナリにまとめている (issue #523)。新しいテストはこのディレクトリに
//! ファイルを置き、ここに `mod` を足すこと。
//!
//! ```sh
//! cargo test -p juiz_sdk --test api -- identifier::
//! ```
//!
//! - `#[juiz_process]` / `#[juiz_container_process]` が生成するファクトリ関数は `#[no_mangle]` なので、
//!   このバイナリに置けるのはそれぞれ 1 つまで (process_macro)。
//! - value_object_foreach_leak は `#[global_allocator]` を置く。数えるのは自分のスレッドの分だけ。

mod capsule;
mod composite_manifest;
mod container;
mod converter;
mod datatype;
mod geometry;
mod identifier;
mod manifest;
mod native;
mod process_macro;
mod profile_serde;
mod transformations;
mod typed_process;
mod value;
mod value_object_foreach_leak;
