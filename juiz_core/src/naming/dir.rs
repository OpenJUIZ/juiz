//! 登録ファイルの置き場所 (docs/spec/naming.md §3.1)
//!
//! Unix では IPC のソケットの置き場所 (docs/spec/ipc-broker.md §2.2) と同じディレクトリを使う。
//! Windows では名前付きパイプにディレクトリが無いので、`%LOCALAPPDATA%\juiz\run` にする。

use std::path::{Path, PathBuf};

use juiz_sdk::anyhow::anyhow;

use crate::prelude::*;

/// 置き場所を指定する環境変数 (IPC ブローカーと共通)
pub const IPC_DIR_ENV: &str = "JUIZ_IPC_DIR";

/// 登録ファイルを置く、`<dir>` の下のディレクトリの名前
pub const NAMES_DIR: &str = "names";

/// ディレクトリが無ければ作る。Unix ではモード 0700 (途中のディレクトリも)
pub(crate) fn create_dir_0700(dir: &Path) -> JuizResult<()> {
    if dir.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| anyhow!("can not create directory {}: {e}", dir.display()))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
            .map_err(|e| anyhow!("can not create directory {}: {e}", dir.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn uid() -> u32 {
    // SAFETY: getuid は失敗しない
    unsafe { libc::getuid() }
}

/// IPC のソケットと登録ファイルを置くディレクトリ `<dir>`。無ければ作る。
///
/// `JUIZ_IPC_DIR` → `$XDG_RUNTIME_DIR/juiz` → `/tmp/juiz-<uid>` の順。`/tmp/juiz-<uid>` は
/// 自分の持ち物でモード 0700 でなければエラー (他人が先に用意した場所は使わない)。
#[cfg(unix)]
pub fn runtime_dir() -> JuizResult<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if let Some(dir) = std::env::var_os(IPC_DIR_ENV).filter(|d| !d.is_empty()) {
        let dir = PathBuf::from(dir);
        create_dir_0700(&dir)?;
        return Ok(dir);
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        let dir = PathBuf::from(runtime).join("juiz");
        create_dir_0700(&dir)?;
        return Ok(dir);
    }
    let dir = PathBuf::from(format!("/tmp/juiz-{}", uid()));
    create_dir_0700(&dir)?;
    let meta = std::fs::symlink_metadata(&dir)
        .map_err(|e| anyhow!("can not read ipc directory {}: {e}", dir.display()))?;
    if !meta.is_dir() || meta.uid() != uid() || meta.permissions().mode() & 0o777 != 0o700 {
        return Err(anyhow!(JuizError::InvalidValueError {
            message: format!(
                "ipc directory {} must be a directory owned by you with mode 0700. Remove it or set {IPC_DIR_ENV}",
                dir.display()
            )
        }));
    }
    Ok(dir)
}

/// 登録ファイルを置くディレクトリの親 `<dir>`。無ければ作る。
///
/// `JUIZ_IPC_DIR` → `%LOCALAPPDATA%\juiz\run` の順。
#[cfg(windows)]
pub fn runtime_dir() -> JuizResult<PathBuf> {
    if let Some(dir) = std::env::var_os(IPC_DIR_ENV).filter(|d| !d.is_empty()) {
        let dir = PathBuf::from(dir);
        create_dir_0700(&dir)?;
        return Ok(dir);
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .filter(|d| !d.is_empty())
        .ok_or_else(|| anyhow!("LOCALAPPDATA is not set. Set {IPC_DIR_ENV}"))?;
    let dir = PathBuf::from(local).join("juiz").join("run");
    create_dir_0700(&dir)?;
    Ok(dir)
}

/// 登録ファイルを置くディレクトリ `<dir>/names`。無ければ作る
pub fn names_dir() -> JuizResult<PathBuf> {
    let dir = runtime_dir()?.join(NAMES_DIR);
    create_dir_0700(&dir)?;
    Ok(dir)
}
