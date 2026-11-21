//! 言語ごとのプラグインの読み込みそのもの(ABI・例外・値の受け渡し)のテスト

mod cpp;
mod haskell;
#[cfg(feature = "ipc-broker")]
mod image;
mod python;
mod rust;
pub(crate) mod zig;
