//! IPC ブローカーの名前と、ソケット (名前付きパイプ) の場所 (docs/spec/ipc-broker.md §2)
//!
//! サーバーとクライアントは、名前だけから同じ場所を求める。

use juiz_sdk::anyhow::anyhow;

use crate::prelude::*;

/// 名前を省略したときの名前
pub const DEFAULT_IPC_NAME: &str = "juiz";

/// ソケットを置くディレクトリを指定する環境変数
pub const IPC_DIR_ENV: &str = crate::naming::IPC_DIR_ENV;

/// 名前は `[A-Za-z0-9_-]` と `.` からなる 1〜64 文字で、`.` で始まらない
pub fn validate_ipc_name(name: &str) -> JuizResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
    if ok {
        Ok(())
    } else {
        Err(anyhow!(JuizError::InvalidValueError {
            message: format!(
                "invalid ipc broker name '{name}'. Use 1 to 64 characters of [A-Za-z0-9_-.], not starting with '.'"
            )
        }))
    }
}

#[cfg(unix)]
mod unix {
    use std::path::PathBuf;

    use juiz_sdk::anyhow::anyhow;

    use super::{validate_ipc_name, IPC_DIR_ENV};
    use crate::prelude::*;

    /// ソケットのパスの長さの上限 (sun_path の大きさから終端の 1 バイトを引いたもの)
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    const MAX_SOCKET_PATH: usize = 103;
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
    const MAX_SOCKET_PATH: usize = 107;

    /// ソケットを置くディレクトリ。無ければモード 0700 で作る。
    /// 置き場所の規則はネーミングサービスの登録ファイルと共通 ([`crate::naming::runtime_dir`])
    pub fn ipc_socket_dir() -> JuizResult<PathBuf> {
        crate::naming::runtime_dir()
    }

    /// 名前のソケットのパス `<dir>/<name>.sock`
    pub fn ipc_socket_path(name: &str) -> JuizResult<PathBuf> {
        validate_ipc_name(name)?;
        let path = ipc_socket_dir()?.join(format!("{name}.sock"));
        let len = path.as_os_str().len();
        if len > MAX_SOCKET_PATH {
            return Err(anyhow!(JuizError::InvalidValueError {
                message: format!(
                    "ipc socket path {} is too long ({len} bytes > {MAX_SOCKET_PATH}). Set {IPC_DIR_ENV} to a shorter directory",
                    path.display()
                )
            }));
        }
        Ok(path)
    }
}

#[cfg(unix)]
pub use unix::{ipc_socket_dir, ipc_socket_path};

/// 名前付きパイプの名前 `\\.\pipe\juiz-<name>`
#[cfg(windows)]
pub fn ipc_pipe_name(name: &str) -> JuizResult<String> {
    validate_ipc_name(name)?;
    Ok(format!(r"\\.\pipe\juiz-{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for ok in ["juiz", "cam0", "a.b-c_d", &"x".repeat(64)] {
            assert!(validate_ipc_name(ok).is_ok(), "{ok}");
        }
        for ng in ["", ".hidden", "a/b", "a b", "日本", &"x".repeat(65), "../x"] {
            assert!(validate_ipc_name(ng).is_err(), "{ng}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn socket_path_is_under_dir() {
        let path = ipc_socket_path("cam0").unwrap();
        assert!(path.ends_with("cam0.sock"), "{path:?}");
        assert!(ipc_socket_path("bad/name").is_err());
    }
}
