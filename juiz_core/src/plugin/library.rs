//! プラグインの共有ライブラリを開く (issue #273)。

use libloading::Library;
use std::path::Path;

/// プラグインの共有ライブラリを開く。
///
/// Windows では LOAD_WITH_ALTERED_SEARCH_PATH で開き、プラグインが依存する DLL (C++ / Haskell の
/// プラグインの juiz_sdk.dll など) を、juiz.exe のディレクトリより先にプラグインのディレクトリから探す。
/// Linux の rpath $ORIGIN に当たる。PATH も今までどおり探す
/// (LOAD_LIBRARY_SEARCH_* のフラグは PATH を探さなくなるので使わない)。
/// このフラグは絶対パスでないと働かないので、相対パスはカレントディレクトリからの絶対パスにする。
///
/// Linux / macOS では今までどおり dlopen で開く。依存するライブラリはプラグインの rpath で探す。
pub(crate) unsafe fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    #[cfg(windows)]
    {
        use libloading::os::windows::{Library as WinLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
        let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        WinLibrary::load_with_flags(abs, LOAD_WITH_ALTERED_SEARCH_PATH).map(Library::from)
    }
    #[cfg(not(windows))]
    {
        Library::new(path)
    }
}
