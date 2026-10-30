//! アプリ配布形式 .juizapp の `juiz pack`・`juiz run`・`juiz install`(docs/spec/app-package.md、#285、#286)
//!
//! - [`manifest`]: マニフェスト `juizapp.yaml` の読み書きと項目の検査(§4)
//! - [`names`]: zip のエントリ名の安全規則(§11)とパッケージ内の相対パス
//! - [`pack`]: `juiz pack`(§6)
//! - [`run`]: `juiz run` の検証・キャッシュへの展開・起動(§7、§9)
//! - [`install`]: `juiz install` / `uninstall` / `packages` とインストール済みの検索(§8)
//! - [`depends`]: 基礎の起動時の設定と、depends の解決・確認(§10)

pub(crate) mod depends;
pub(crate) mod install;
pub(crate) mod manifest;
pub(crate) mod names;
pub(crate) mod pack;
pub(crate) mod run;
mod yaml;

use sha2::{Digest, Sha256};
use std::io::Read;

/// マニフェストのファイル名(zip のルートに固定)
pub(crate) const MANIFEST_FILE: &str = "juizapp.yaml";
/// この juiz が扱えるパッケージ形式の版
pub(crate) const FORMAT_VERSION: i64 = 1;
/// この juiz の版
pub(crate) const JUIZ_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 利用者に見せる警告。juiz は既定では log の warn を出さないので、標準エラーに書く
pub(crate) fn warn(msg: &str) {
    eprintln!("warning: {msg}");
}

/// 読み込める全体を SHA-256 にかけ、小文字 16 進 64 桁で返す
pub(crate) fn sha256_hex_reader(mut r: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

/// ターゲットのプラグインのファイル名(lib<name>.so / lib<name>.dylib / <name>.dll)
pub(crate) fn plugin_file_name(target: &str, name: &str) -> String {
    if target.contains("windows") {
        format!("{name}.dll")
    } else if target.contains("apple") || target.contains("darwin") {
        format!("lib{name}.dylib")
    } else {
        format!("lib{name}.so")
    }
}

/// 仕様書 §5.1 の表のターゲット(表に無いものは pack が警告する)
pub(crate) const KNOWN_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty_and_abc() {
        assert_eq!(
            sha256_hex_reader(&b""[..]).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex_reader(&b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn plugin_file_names_per_target() {
        assert_eq!(
            plugin_file_name("x86_64-unknown-linux-gnu", "talker"),
            "libtalker.so"
        );
        assert_eq!(
            plugin_file_name("aarch64-apple-darwin", "talker"),
            "libtalker.dylib"
        );
        assert_eq!(
            plugin_file_name("x86_64-pc-windows-msvc", "talker"),
            "talker.dll"
        );
    }
}
