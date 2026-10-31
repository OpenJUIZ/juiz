//! 大きなデータを生バイトのまま送るフレーム形式 (方式 A、docs/spec/large-data.md §3)
//!
//! 実体は juiz_sdk::frame にある (Rust プラグインの C ABI のグルーと同じものを使うため、issue #329)。

pub use juiz_sdk::frame::*;
