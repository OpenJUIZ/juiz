//! `juiz associate`: .juizapp をダブルクリックで起動できるようにする関連付け(docs/spec/app-package.md §17、#395)
//!
//! - Windows は HKCU\Software\Classes に書く(管理者権限は要らない、#395)。
//!   書く中身は [`entries`] で決め、読み書きは [`Registry`] で差し替えられるようにして、
//!   どの OS でも偽の Registry で確かめられるようにしている。
//! - Linux は freedesktop の MIME と .desktop([`linux`]、#396)。
//! - macOS は AppleScript のアプレット `~/Applications/JUIZ Launcher.app`([`macos`]、#396)。
//!
//! どの OS の実装もすべての OS でビルドしてテストし、[`run`] で今の OS のものを選ぶ。
#![allow(dead_code)]

pub(crate) mod linux;
pub(crate) mod macos;
pub(crate) mod tools;

use juiz_core::anyhow::Result;
use std::path::Path;

/// 拡張子のキー(HKCU\Software\Classes からの相対)
pub(crate) const EXT_KEY: &str = ".juizapp";
/// ProgID
pub(crate) const PROG_ID: &str = "JUIZ.App";
/// ダブルクリックで動くコマンドのキー
pub(crate) const OPEN_COMMAND_KEY: &str = r"JUIZ.App\shell\open\command";

/// 関連付けの読み書き(キーの既定値だけを扱う)
pub(crate) trait Registry {
    /// キーの既定値。キーか値が無ければ None
    fn get(&self, key: &str) -> Result<Option<String>>;
    /// キーを(無ければ作って)既定値を書く
    fn set(&mut self, key: &str, value: &str) -> Result<()>;
    /// キーを下のキーごと消す。無ければ何もしない
    fn delete_tree(&mut self, key: &str) -> Result<()>;
    /// 関連付けが変わったことをエクスプローラーに知らせる
    fn notify(&mut self);
}

/// 書くキーと既定値(§17.4)。`<アイコン>` は用意できるまで書かない
pub(crate) fn entries(exe: &Path) -> Vec<(String, String)> {
    let exe = exe.display();
    vec![
        (EXT_KEY.to_owned(), PROG_ID.to_owned()),
        (PROG_ID.to_owned(), "JUIZ application".to_owned()),
        (
            OPEN_COMMAND_KEY.to_owned(),
            format!("\"{exe}\" run --launched \"%1\""),
        ),
        (
            r"JUIZ.App\shell\install".to_owned(),
            "Install with juiz".to_owned(),
        ),
        (
            r"JUIZ.App\shell\install\command".to_owned(),
            format!("\"{exe}\" install --launched \"%1\""),
        ),
    ]
}

/// コマンドの文字列の先頭の実行ファイル(`"C:\...\juiz.exe" run ...` の引用符の中)
pub(crate) fn command_executable(command: &str) -> Option<String> {
    let command = command.trim_start();
    if let Some(rest) = command.strip_prefix('"') {
        rest.split_once('"').map(|(exe, _)| exe.to_owned())
    } else {
        command.split_whitespace().next().map(str::to_owned)
    }
}

/// `juiz associate` の結果
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Associated {
    /// 新しく書いた
    New,
    /// 別の juiz を指していたのを書き換えた(前のパス)
    Replaced(String),
    /// 既に今の juiz を指していたので何もしなかった
    Unchanged,
}

/// 今の juiz を .juizapp に関連付ける(§17.1)
pub(crate) fn associate(reg: &mut impl Registry, exe: &Path) -> Result<Associated> {
    let wanted = entries(exe);
    let mut same = true;
    for (key, value) in &wanted {
        if reg.get(key)?.as_deref() != Some(value.as_str()) {
            same = false;
            break;
        }
    }
    if same {
        return Ok(Associated::Unchanged);
    }
    let previous = reg
        .get(OPEN_COMMAND_KEY)?
        .and_then(|c| command_executable(&c))
        .filter(|p| Path::new(p) != exe);
    // 前の版で書いた余分なキーが残らないように ProgID は消してから書く
    reg.delete_tree(PROG_ID)?;
    for (key, value) in &wanted {
        reg.set(key, value)?;
    }
    reg.notify();
    Ok(match previous {
        Some(p) => Associated::Replaced(p),
        None => Associated::New,
    })
}

/// 関連付けを外す(§17.1)。外したものがあれば true
///
/// 拡張子のキーは、ほかのアプリの関連付けを消さないように、既定値が JUIZ.App のときだけ消す
pub(crate) fn remove(reg: &mut impl Registry) -> Result<bool> {
    let mut removed = false;
    if reg.get(EXT_KEY)?.as_deref() == Some(PROG_ID) {
        reg.delete_tree(EXT_KEY)?;
        removed = true;
    }
    if reg.get(PROG_ID)?.is_some() || reg.get(OPEN_COMMAND_KEY)?.is_some() {
        reg.delete_tree(PROG_ID)?;
        removed = true;
    }
    if removed {
        reg.notify();
    }
    Ok(removed)
}

/// `juiz associate [--remove]` の本体
pub(crate) fn run(remove_flag: bool) -> Result<()> {
    #[cfg(windows)]
    {
        let mut reg = windows::HkcuClasses;
        if remove_flag {
            report_removed(remove(&mut reg)?, &[]);
            return Ok(());
        }
        let exe = crate::package::depcheck::juiz_executable()?;
        report_associated(&associate(&mut reg, &exe)?, &[], &exe);
        println!("Double-click a .juizapp to run it; right-click it for \"Install with juiz\".");
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let apps = macos::apps_dir_from_env()?;
        let mut tools = tools::SystemTools;
        if remove_flag {
            let (removed, warnings) = macos::remove(&mut tools, &apps)?;
            report_removed(removed, &warnings);
            return Ok(());
        }
        let exe = crate::package::depcheck::juiz_executable()?;
        let (r, warnings) = macos::associate(&mut tools, &apps, &exe)?;
        report_associated(&r, &warnings, &exe);
        println!(
            "Double-click a .juizapp to run it in Terminal ({} opens it).",
            apps.join(macos::APP_NAME).display()
        );
        println!(
            "The first time, macOS asks whether JUIZ Launcher may control Terminal; allow it."
        );
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dirs = linux::Dirs::from_env()?;
        let mut tools = tools::SystemTools;
        if remove_flag {
            let (removed, warnings) = linux::remove(&mut tools, &dirs)?;
            report_removed(removed, &warnings);
            return Ok(());
        }
        let exe = crate::package::depcheck::juiz_executable()?;
        let (r, warnings) = linux::associate(&mut tools, &dirs, &exe)?;
        report_associated(&r, &warnings, &exe);
        println!(
            "Double-click a .juizapp to run it in a terminal; use \"Open With\" > \"Install with juiz\" to install it."
        );
        Ok(())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = remove_flag;
        juiz_core::anyhow::bail!("juiz associate is not available on this OS")
    }
}

fn report_warnings(warnings: &[String]) {
    for w in warnings {
        eprintln!("warning: {w}");
    }
}

fn report_removed(removed: bool, warnings: &[String]) {
    report_warnings(warnings);
    if removed {
        println!("Removed the association of .juizapp");
    } else {
        println!(".juizapp is not associated with juiz");
    }
}

fn report_associated(r: &Associated, warnings: &[String], exe: &Path) {
    report_warnings(warnings);
    match r {
        Associated::Unchanged => {
            println!(".juizapp is already associated with {}", exe.display())
        }
        Associated::Replaced(prev) => {
            println!(".juizapp was associated with {prev}; now {}", exe.display())
        }
        Associated::New => println!("Associated .juizapp with {}", exe.display()),
    }
}

/// HKCU\Software\Classes の読み書き
#[cfg(windows)]
mod windows {
    use super::Registry;
    use juiz_core::anyhow::{anyhow, Result};
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    pub(super) struct HkcuClasses;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn path(key: &str) -> Vec<u16> {
        wide(&format!(r"Software\Classes\{key}"))
    }

    impl Registry for HkcuClasses {
        fn get(&self, key: &str) -> Result<Option<String>> {
            let sub = path(key);
            let mut size: u32 = 0;
            let rc = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    sub.as_ptr(),
                    std::ptr::null(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut size,
                )
            };
            if rc == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if rc != ERROR_SUCCESS {
                return Err(anyhow!(
                    "cannot read HKCU\\Software\\Classes\\{key} (error {rc})"
                ));
            }
            let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
            let mut size = (buf.len() * 2) as u32;
            let rc = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    sub.as_ptr(),
                    std::ptr::null(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            if rc == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if rc != ERROR_SUCCESS {
                return Err(anyhow!(
                    "cannot read HKCU\\Software\\Classes\\{key} (error {rc})"
                ));
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Ok(Some(String::from_utf16_lossy(&buf[..len])))
        }

        fn set(&mut self, key: &str, value: &str) -> Result<()> {
            let sub = path(key);
            let mut hkey: HKEY = std::ptr::null_mut();
            let rc = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    sub.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_WRITE,
                    std::ptr::null(),
                    &mut hkey,
                    std::ptr::null_mut(),
                )
            };
            if rc != ERROR_SUCCESS {
                return Err(anyhow!(
                    "cannot create HKCU\\Software\\Classes\\{key} (error {rc})"
                ));
            }
            let data = wide(value);
            let rc = unsafe {
                RegSetValueExW(
                    hkey,
                    std::ptr::null(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            };
            unsafe { RegCloseKey(hkey) };
            if rc != ERROR_SUCCESS {
                return Err(anyhow!(
                    "cannot write HKCU\\Software\\Classes\\{key} (error {rc})"
                ));
            }
            Ok(())
        }

        fn delete_tree(&mut self, key: &str) -> Result<()> {
            let sub = path(key);
            let rc = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, sub.as_ptr()) };
            if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
                return Err(anyhow!(
                    "cannot delete HKCU\\Software\\Classes\\{key} (error {rc})"
                ));
            }
            Ok(())
        }

        fn notify(&mut self) {
            unsafe {
                SHChangeNotify(
                    SHCNE_ASSOCCHANGED as i32,
                    SHCNF_IDLIST,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 本物のレジストリに、テスト用の名前のキーで書いて読んで消せる
        #[test]
        fn hkcu_roundtrip() {
            let mut reg = HkcuClasses;
            let key = r"JUIZ.Test.395\shell\open\command";
            reg.delete_tree("JUIZ.Test.395").unwrap();
            assert_eq!(reg.get(key).unwrap(), None);
            reg.set(key, r#""C:\ジュイズ\juiz.exe" run --launched "%1""#)
                .unwrap();
            assert_eq!(
                reg.get(key).unwrap().as_deref(),
                Some(r#""C:\ジュイズ\juiz.exe" run --launched "%1""#)
            );
            reg.delete_tree("JUIZ.Test.395").unwrap();
            assert_eq!(reg.get(key).unwrap(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[derive(Default)]
    struct FakeRegistry {
        values: BTreeMap<String, String>,
        notified: usize,
    }

    impl Registry for FakeRegistry {
        fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.values.get(key).cloned())
        }
        fn set(&mut self, key: &str, value: &str) -> Result<()> {
            self.values.insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        fn delete_tree(&mut self, key: &str) -> Result<()> {
            let prefix = format!("{key}\\");
            self.values
                .retain(|k, _| k != key && !k.starts_with(&prefix));
            Ok(())
        }
        fn notify(&mut self) {
            self.notified += 1;
        }
    }

    fn exe(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    #[test]
    fn writes_the_keys_of_spec() {
        let mut reg = FakeRegistry::default();
        let r = associate(&mut reg, &exe(r"C:\venv\Scripts\juiz.exe")).unwrap();
        assert_eq!(r, Associated::New);
        assert_eq!(reg.values[".juizapp"], "JUIZ.App");
        assert_eq!(reg.values["JUIZ.App"], "JUIZ application");
        assert_eq!(
            reg.values[r"JUIZ.App\shell\open\command"],
            r#""C:\venv\Scripts\juiz.exe" run --launched "%1""#
        );
        assert_eq!(reg.values[r"JUIZ.App\shell\install"], "Install with juiz");
        assert_eq!(
            reg.values[r"JUIZ.App\shell\install\command"],
            r#""C:\venv\Scripts\juiz.exe" install --launched "%1""#
        );
        assert_eq!(reg.notified, 1);
    }

    #[test]
    fn same_juiz_does_nothing_and_other_juiz_is_reported() {
        let mut reg = FakeRegistry::default();
        associate(&mut reg, &exe(r"C:\a\juiz.exe")).unwrap();
        assert_eq!(
            associate(&mut reg, &exe(r"C:\a\juiz.exe")).unwrap(),
            Associated::Unchanged
        );
        assert_eq!(reg.notified, 1);
        assert_eq!(
            associate(&mut reg, &exe(r"C:\b\juiz.exe")).unwrap(),
            Associated::Replaced(r"C:\a\juiz.exe".to_owned())
        );
        assert_eq!(
            reg.values[r"JUIZ.App\shell\open\command"],
            r#""C:\b\juiz.exe" run --launched "%1""#
        );
        assert_eq!(reg.notified, 2);
    }

    #[test]
    fn old_extra_keys_under_prog_id_are_dropped() {
        let mut reg = FakeRegistry::default();
        reg.set(r"JUIZ.App\shell\edit\command", "old").unwrap();
        associate(&mut reg, &exe(r"C:\a\juiz.exe")).unwrap();
        assert!(!reg.values.contains_key(r"JUIZ.App\shell\edit\command"));
    }

    #[test]
    fn remove_keeps_other_apps_extension() {
        let mut reg = FakeRegistry::default();
        assert!(!remove(&mut reg).unwrap());
        assert_eq!(reg.notified, 0);

        associate(&mut reg, &exe(r"C:\a\juiz.exe")).unwrap();
        assert!(remove(&mut reg).unwrap());
        assert!(reg.values.is_empty());

        // .juizapp を別のアプリが持っていたら拡張子のキーは残す
        associate(&mut reg, &exe(r"C:\a\juiz.exe")).unwrap();
        reg.set(".juizapp", "Other.App").unwrap();
        assert!(remove(&mut reg).unwrap());
        assert_eq!(reg.values.len(), 1);
        assert_eq!(reg.values[".juizapp"], "Other.App");
    }

    #[test]
    fn command_executable_reads_quoted_and_bare() {
        assert_eq!(
            command_executable(r#""C:\Program Files\juiz\juiz.exe" run --launched "%1""#)
                .as_deref(),
            Some(r"C:\Program Files\juiz\juiz.exe")
        );
        assert_eq!(
            command_executable(r"C:\juiz.exe run %1").as_deref(),
            Some(r"C:\juiz.exe")
        );
        assert_eq!(command_executable(""), None);
    }
}
