//! プラグインのビルド情報(コンパイラと juiz_sdk の版)
//!
//! Rust のプラグインは `extern "Rust"` の関数で読み込まれるので、ホスト(juiz)と
//! コンパイラや juiz_sdk の版が食い違うと、読み込みに失敗したり異常終了したりする。
//! juiz_macro は各ファクトリ関数 `<シンボル名>` の隣に `<シンボル名>_build_info` という
//! `extern "C"` の関数を生成し、プラグインをビルドしたときのビルド情報を返す。
//! juiz_core はファクトリを呼ぶ前にこの関数を呼び、ホストのビルド情報と比べる。

use std::os::raw::c_char;

/// ビルド情報を返す関数のシンボル名に付ける接尾辞
pub const BUILD_INFO_SYMBOL_SUFFIX: &str = "_build_info";

/// この juiz_sdk のビルド情報。`key=value` を `;` でつなぎ、NUL で終わる。
const BUILD_INFO: &str = concat!(
    "juiz_sdk=",
    env!("CARGO_PKG_VERSION"),
    ";rustc=",
    env!("JUIZ_SDK_RUSTC_VERSION"),
    "\0"
);

/// ビルド情報を C 文字列で返す。juiz_macro が生成する `*_build_info` 関数から呼ばれる。
pub fn build_info_ptr() -> *const c_char {
    BUILD_INFO.as_ptr() as *const c_char
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub juiz_sdk: Option<String>,
    pub rustc: Option<String>,
}

impl BuildInfo {
    /// この juiz_sdk(を使うホストまたはプラグイン)自身のビルド情報
    pub fn current() -> BuildInfo {
        BuildInfo::parse(BUILD_INFO.trim_end_matches('\0'))
    }

    /// `key=value;key=value` 形式の文字列を読む。知らないキーは無視する。
    pub fn parse(s: &str) -> BuildInfo {
        let mut info = BuildInfo {
            juiz_sdk: None,
            rustc: None,
        };
        for item in s.split(';') {
            match item.split_once('=') {
                Some(("juiz_sdk", v)) => info.juiz_sdk = Some(v.to_owned()),
                Some(("rustc", v)) => info.rustc = Some(v.to_owned()),
                _ => {}
            }
        }
        info
    }

    /// juiz_sdk の版
    pub fn juiz_sdk_str(&self) -> &str {
        self.juiz_sdk.as_deref().unwrap_or("unknown")
    }

    /// コンパイラの版
    pub fn rustc_str(&self) -> &str {
        self.rustc.as_deref().unwrap_or("unknown")
    }
}

impl std::fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "juiz_sdk {}, {}", self.juiz_sdk_str(), self.rustc_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn current_build_info_has_versions() {
        let info = BuildInfo::current();
        assert_eq!(info.juiz_sdk.as_deref(), Some(env!("CARGO_PKG_VERSION")));
        assert!(info.rustc_str().starts_with("rustc "), "{info}");
    }

    #[test]
    fn build_info_ptr_is_c_string() {
        let s = unsafe { CStr::from_ptr(build_info_ptr()) }
            .to_str()
            .unwrap();
        assert_eq!(BuildInfo::parse(s), BuildInfo::current());
    }

    #[test]
    fn parse_ignores_unknown_keys() {
        let info = BuildInfo::parse("juiz_sdk=1.2.3;target=x86_64;rustc=rustc 1.0.0");
        assert_eq!(info.juiz_sdk.as_deref(), Some("1.2.3"));
        assert_eq!(info.rustc.as_deref(), Some("rustc 1.0.0"));
        assert_eq!(BuildInfo::parse("").juiz_sdk, None);
    }
}
