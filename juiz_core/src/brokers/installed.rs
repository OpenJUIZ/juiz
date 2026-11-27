//! 置き場所に入れた C ABI のブローカー (docs/spec/broker-plugin-abi.md §6.4 の 3・§10、issue #638)
//!
//! ```text
//! ~/.juiz/brokers/<type_name>/<版>/juiz-broker.json    (JUIZ_BROKER_HOME で変えられる)
//!                                  lib/<ライブラリ>
//! ```
//!
//! - [`list_installed`]: 置き場所の全部と、それぞれが使われるか (`juiz broker list`)
//! - [`select_installed`]: その種類で読み込みに使う版 (§10.1。`juiz broker path`、読み込み)
//! - [`load_installed_broker`]: conf の `brokers` や ID の頭で、conf のプラグインにも組み込みにも無い種類が
//!   要るようになったときに、置き場所から読み込む (§6.4)。一度読み込んだライブラリはプロセスが終わるまで持つ

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::brokers::cabi::{BrokerEntry, SUPPORTED_BROKER_ABI_VERSIONS};
use crate::plugin::{RustPlugin, RustPluginAbi, HOST_TARGET};
use crate::prelude::*;
use juiz_sdk::anyhow::anyhow;

/// 置き場所の各版のディレクトリにある説明のファイル (§10.1)
pub const BROKER_PACKAGE_FILE: &str = "juiz-broker.json";

/// juiz-broker.json の形の版
pub const BROKER_PACKAGE_FORMAT: u32 = 1;

/// 置き場所を変える環境変数
pub const BROKER_HOME_ENV: &str = "JUIZ_BROKER_HOME";

/// juiz-broker.json の中身 (§10.1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerPackageManifest {
    pub format: u32,
    pub type_name: String,
    pub plugin_name: String,
    pub version: String,
    pub abi_version: u32,
    pub target: String,
    /// 版のディレクトリからの相対パス
    pub library: String,
}

impl BrokerPackageManifest {
    /// juiz-broker.json を読む。形が合わなければ理由の文
    pub fn read(dir: &Path) -> Result<Self, String> {
        let path = dir.join(BROKER_PACKAGE_FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let m: BrokerPackageManifest = serde_json::from_str(&text)
            .map_err(|e| format!("{} is broken: {e}", path.display()))?;
        if m.format != BROKER_PACKAGE_FORMAT {
            return Err(format!(
                "{} has format {}, but this juiz reads format {BROKER_PACKAGE_FORMAT}",
                path.display(),
                m.format
            ));
        }
        if semver::Version::parse(&m.version).is_err() {
            return Err(format!("the version '{}' in {} is not semver", m.version, path.display()));
        }
        let lib = Path::new(&m.library);
        if m.library.is_empty()
            || lib.is_absolute()
            || lib.components().any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(format!(
                "'library' in {} must be a relative path inside the directory, but '{}'",
                path.display(),
                m.library
            ));
        }
        Ok(m)
    }

    /// ライブラリの絶対パス
    pub fn library_path(&self, dir: &Path) -> PathBuf {
        dir.join(&self.library)
    }
}

/// 置き場所 (§10.1)。`JUIZ_BROKER_HOME`、無ければ `~/.juiz/brokers`。ホームがわからなければ None
pub fn broker_home() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os(BROKER_HOME_ENV).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    home::home_dir().map(|h| h.join(".juiz").join("brokers"))
}

/// 置き場所の 1 つの版が使えるかどうか (`juiz broker list` の状態)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledStatus {
    /// その種類で読み込みに使われる版
    Used,
    /// 使える版だが、新しい版がある
    Older,
    /// ABI の版をこの juiz が読めない
    AbiNotSupported(u32),
    /// 読めない (juiz-broker.json が無い・壊れている、名前が合わない、ターゲットが違う、など)
    Invalid(String),
}

impl std::fmt::Display for InstalledStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstalledStatus::Used => write!(f, "used"),
            InstalledStatus::Older => write!(f, "older"),
            InstalledStatus::AbiNotSupported(a) => write!(f, "ABI {a} is not supported by this juiz"),
            InstalledStatus::Invalid(why) => write!(f, "invalid ({why})"),
        }
    }
}

/// 置き場所の 1 つの版
#[derive(Debug, Clone)]
pub struct InstalledBroker {
    /// 種類のディレクトリの名前
    pub type_name: String,
    /// 版のディレクトリの名前
    pub version: String,
    pub dir: PathBuf,
    /// 読めたときの juiz-broker.json
    pub manifest: Option<BrokerPackageManifest>,
    pub status: InstalledStatus,
}

/// 版のディレクトリを確かめる。使える (ABI の版とターゲットが合う) なら Ok
fn check_version_dir(type_name: &str, version: &str, dir: &Path) -> (Option<BrokerPackageManifest>, InstalledStatus) {
    let m = match BrokerPackageManifest::read(dir) {
        Ok(m) => m,
        Err(e) => return (None, InstalledStatus::Invalid(e)),
    };
    let status = if m.type_name != type_name {
        InstalledStatus::Invalid(format!("type_name in {BROKER_PACKAGE_FILE} is '{}'", m.type_name))
    } else if m.version != version {
        InstalledStatus::Invalid(format!("version in {BROKER_PACKAGE_FILE} is '{}'", m.version))
    } else if m.target != HOST_TARGET {
        InstalledStatus::Invalid(format!("built for {}", m.target))
    } else if !SUPPORTED_BROKER_ABI_VERSIONS.contains(&m.abi_version) {
        InstalledStatus::AbiNotSupported(m.abi_version)
    } else if !m.library_path(dir).is_file() {
        InstalledStatus::Invalid(format!("{} is not found", m.library))
    } else {
        InstalledStatus::Older
    };
    (Some(m), status)
}

fn sorted_dirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_owned();
                // 入れている途中の作業用ディレクトリ (.<名前>.tmp-…) は見ない
                (!name.starts_with('.')).then(|| (name, e.path()))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// ある種類の置き場所の全部の版。使える版のうち semver でいちばん新しいものを Used にする
pub fn list_installed_type(home: &Path, type_name: &str) -> Vec<InstalledBroker> {
    let mut out: Vec<InstalledBroker> = sorted_dirs(&home.join(type_name))
        .into_iter()
        .map(|(version, dir)| {
            let (manifest, status) = check_version_dir(type_name, &version, &dir);
            InstalledBroker {
                type_name: type_name.to_owned(),
                version,
                dir,
                manifest,
                status,
            }
        })
        .collect();
    let newest = out
        .iter()
        .enumerate()
        .filter(|(_, b)| b.status == InstalledStatus::Older)
        .filter_map(|(i, b)| semver::Version::parse(&b.version).ok().map(|v| (i, v)))
        .max_by(|a, b| a.1.cmp(&b.1))
        .map(|(i, _)| i);
    if let Some(i) = newest {
        out[i].status = InstalledStatus::Used;
    }
    out.sort_by(|a, b| {
        let va = semver::Version::parse(&a.version).ok();
        let vb = semver::Version::parse(&b.version).ok();
        va.cmp(&vb).then_with(|| a.version.cmp(&b.version))
    });
    out
}

/// 置き場所の全部の種類・版 (種類の名前の順、その中は版の順)
pub fn list_installed(home: &Path) -> Vec<InstalledBroker> {
    sorted_dirs(home)
        .into_iter()
        .flat_map(|(type_name, _)| list_installed_type(home, &type_name))
        .collect()
}

/// その種類で読み込みに使う版 (§10.1)。無ければ None
pub fn select_installed(home: &Path, type_name: &str) -> Option<InstalledBroker> {
    if !is_valid_type_name(type_name) {
        return None;
    }
    list_installed_type(home, type_name)
        .into_iter()
        .find(|b| b.status == InstalledStatus::Used)
}

/// 種類の名前に使える文字か (置き場所のディレクトリの名前になるので、区切りや `..` を入れさせない)
pub fn is_valid_type_name(type_name: &str) -> bool {
    !type_name.is_empty()
        && type_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// ファイルのバイト列から ABI の印 `juiz_broker_abi=<版>;` を探す (§9.3)。
/// 2 つ以上の違う版があればエラーの文
pub fn broker_abi_marker_in(bytes: &[u8]) -> Result<Option<u32>, String> {
    let needle = b"juiz_broker_abi=";
    let mut found: Option<u32> = None;
    let mut i = 0;
    while let Some(pos) = bytes[i..].windows(needle.len()).position(|w| w == needle) {
        let start = i + pos + needle.len();
        i = start;
        let digits = bytes[start..]
            .iter()
            .take(10)
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits == 0 || bytes.get(start + digits) != Some(&b';') {
            continue;
        }
        let Ok(v) = std::str::from_utf8(&bytes[start..start + digits])
            .unwrap_or("")
            .parse::<u32>()
        else {
            continue;
        };
        match found {
            None => found = Some(v),
            Some(f) if f != v => {
                return Err(format!(
                    "more than one broker ABI marker (juiz_broker_abi={f} and juiz_broker_abi={v})"
                ))
            }
            _ => {}
        }
    }
    Ok(found)
}

/// 読み込んだ表。ライブラリのパスごと。ライブラリはプロセスが終わるまで外さない (§13)
static LOADED: Mutex<Vec<(PathBuf, Arc<BrokerEntry>)>> = Mutex::new(Vec::new());

/// 置き場所から type_name のブローカーを読み込む (§6.4 の 3)。置き場所に無ければ Ok(None)
pub fn load_installed_broker(type_name: &str) -> JuizResult<Option<Arc<BrokerEntry>>> {
    let Some(home) = broker_home() else {
        return Ok(None);
    };
    load_installed_broker_from(&home, type_name)
}

/// [`load_installed_broker`] の置き場所を指定する形
pub fn load_installed_broker_from(home: &Path, type_name: &str) -> JuizResult<Option<Arc<BrokerEntry>>> {
    let Some(b) = select_installed(home, type_name) else {
        return Ok(None);
    };
    let m = b.manifest.as_ref().expect("Used has a manifest");
    let lib = m.library_path(&b.dir);
    let mut loaded = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, e)) = loaded.iter().find(|(p, _)| *p == lib) {
        return Ok(Some(e.clone()));
    }
    let plugin = RustPlugin::load_with_abi(lib.clone(), RustPluginAbi::Auto)?;
    let label = lib.display().to_string();
    let entry_fn = plugin.broker_cabi_entry().ok_or_else(|| {
        anyhow!(JuizError::BrokerPluginLoadError {
            plugin: label.clone(),
            message: format!("it has no '{}'", juiz_sdk::broker_cabi::ENTRY_SYMBOL),
        })
    })?;
    let keep: Arc<dyn std::any::Any + Send + Sync> = plugin.library();
    // SAFETY: broker_factory_cabi は §3.1 の入口
    let entry = unsafe { BrokerEntry::load(entry_fn, &label, Some(keep)) }?;
    if entry.type_name() != type_name {
        return Err(anyhow!(JuizError::BrokerPluginLoadError {
            plugin: label,
            message: format!(
                "it is installed as the broker type '{type_name}', but its type_name is '{}'",
                entry.type_name()
            ),
        }));
    }
    log::info!(
        "BrokerFactory(type_name={type_name}) (installed {} {}, C ABI) Loaded from {}",
        m.plugin_name,
        m.version,
        lib.display()
    );
    loaded.push((lib, entry.clone()));
    Ok(Some(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_installed_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(home: &Path, type_name: &str, version: &str, abi: u32, target: &str) -> PathBuf {
        let dir = home.join(type_name).join(version);
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib").join("libx.so"), b"x").unwrap();
        let m = BrokerPackageManifest {
            format: 1,
            type_name: type_name.to_owned(),
            plugin_name: format!("juiz_{type_name}_broker"),
            version: version.to_owned(),
            abi_version: abi,
            target: target.to_owned(),
            library: "lib/libx.so".to_owned(),
        };
        std::fs::write(dir.join(BROKER_PACKAGE_FILE), serde_json::to_string_pretty(&m).unwrap()).unwrap();
        dir
    }

    #[test]
    fn newest_usable_version_is_used() {
        let home = tmp("select");
        put(&home, "foo", "0.1.0", 1, HOST_TARGET);
        put(&home, "foo", "0.10.0", 1, HOST_TARGET);
        put(&home, "foo", "0.2.0", 1, HOST_TARGET);
        put(&home, "foo", "0.11.0", 99, HOST_TARGET);
        put(&home, "foo", "0.12.0", 1, "no-such-target");
        let d = put(&home, "foo", "0.13.0", 1, HOST_TARGET);
        // ディレクトリの名前と juiz-broker.json の版が違う
        std::fs::rename(&d, home.join("foo").join("0.14.0")).unwrap();
        std::fs::create_dir_all(home.join("foo").join(".0.15.0.tmp-1")).unwrap();

        let list = list_installed_type(&home, "foo");
        let s: Vec<(String, String)> = list.iter().map(|b| (b.version.clone(), b.status.to_string())).collect();
        assert_eq!(s[0], ("0.1.0".into(), "older".into()));
        assert_eq!(s[1], ("0.2.0".into(), "older".into()));
        assert_eq!(s[2], ("0.10.0".into(), "used".into()));
        assert_eq!(s[3], ("0.11.0".into(), "ABI 99 is not supported by this juiz".into()));
        assert!(s[4].1.contains("built for no-such-target"), "{s:?}");
        assert!(s[5].1.contains("version in juiz-broker.json is '0.13.0'"), "{s:?}");
        assert_eq!(s.len(), 6);
        assert_eq!(select_installed(&home, "foo").unwrap().version, "0.10.0");
        assert!(select_installed(&home, "bar").is_none());
        assert!(select_installed(&home, "../foo").is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn broken_manifests_are_invalid() {
        let home = tmp("broken");
        let d = home.join("foo").join("0.1.0");
        std::fs::create_dir_all(&d).unwrap();
        let list = list_installed(&home);
        assert!(matches!(list[0].status, InstalledStatus::Invalid(_)), "{list:?}");
        std::fs::write(d.join(BROKER_PACKAGE_FILE), "{").unwrap();
        assert!(list_installed(&home)[0].status.to_string().contains("broken"));
        let d = put(&home, "foo", "0.2.0", 1, HOST_TARGET);
        std::fs::remove_file(d.join("lib").join("libx.so")).unwrap();
        assert!(list_installed(&home)[1].status.to_string().contains("lib/libx.so is not found"));
        let mut m = BrokerPackageManifest::read(&d).unwrap();
        m.library = "../x.so".into();
        std::fs::write(d.join(BROKER_PACKAGE_FILE), serde_json::to_string(&m).unwrap()).unwrap();
        assert!(BrokerPackageManifest::read(&d).unwrap_err().contains("relative path inside"));
        assert!(select_installed(&home, "foo").is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn markers() {
        assert_eq!(broker_abi_marker_in(b"abc").unwrap(), None);
        assert_eq!(broker_abi_marker_in(b"..juiz_broker_abi=1;\0..").unwrap(), Some(1));
        assert_eq!(broker_abi_marker_in(b"juiz_broker_abi=;juiz_broker_abi=x;juiz_broker_abi=12;").unwrap(), Some(12));
        assert!(broker_abi_marker_in(b"juiz_broker_abi=1;juiz_broker_abi=2;").is_err());
        assert_eq!(
            broker_abi_marker_in(juiz_sdk::broker_cabi::BROKER_ABI_MARKER.as_bytes()).unwrap(),
            Some(juiz_sdk::broker_cabi::BROKER_ABI_VERSION)
        );
    }

    #[test]
    fn type_names() {
        assert!(is_valid_type_name("zenoh"));
        assert!(is_valid_type_name("my_broker-2"));
        assert!(!is_valid_type_name(""));
        assert!(!is_valid_type_name(".."));
        assert!(!is_valid_type_name("a/b"));
    }
}
