//! 置き場所に C ABI のブローカーを入れる・並べる・消す (docs/spec/broker-plugin-abi.md §10、issue #638)
//!
//! ```text
//! juiz broker install <名前> [--version <版>] [--from <URL|file://パス>] [--force]
//! juiz broker list
//! juiz broker path <名前> [--version <版>]
//! juiz broker remove <名前> [--version <版>]
//! ```
//!
//! - 置き場所は `<JUIZ_BROKER_HOME>/<種類名>/<版>/`、`JUIZ_BROKER_HOME` の既定は `~/.juiz/brokers`。
//!   読み込み (conf に無い種類を見たとき) は juiz_core::brokers::installed がする
//! - アーカイブは `juiz_broker-<種類名>-<版>-abi<N>-<ターゲット>.tar.gz` (Windows は `.zip`) と `.sha256`
//! - 取得元の頭は `JUIZ_BROKER_BASE_URL` で変えられる。`file://<ディレクトリ>` なら
//!   `<ディレクトリ>/v<版>/<アーカイブ>`、版を省けば `v*` のディレクトリを GitHub の API の代わりに見る (テスト用)

use std::path::{Path, PathBuf};

use clap::Subcommand;
use juiz_core::anyhow::{anyhow, bail, Context, Result};
use juiz_core::cabi::SUPPORTED_BROKER_ABI_VERSIONS;
use juiz_core::installed::{
    broker_abi_marker_in, broker_home, is_valid_type_name, list_installed, list_installed_type,
    select_installed, BrokerPackageManifest, InstalledStatus, BROKER_HOME_ENV,
};
use juiz_core::HOST_TARGET;
use juiz_sdk::serde_json;

use crate::sdk::{extract, fetch, verify_sha256};

/// 取得元を変える環境変数 (§10.4)
const BASE_URL_ENV: &str = "JUIZ_BROKER_BASE_URL";

/// 種類名から取得先のリポジトリ (owner/repo) への表 (§10.3)
const NAME_TABLE: &[(&str, &str)] = &[("zenoh", "OpenJUIZ/juiz_zenoh_broker")];

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum BrokerSubCommands {
    /// Download a broker plugin (C ABI) and install it, so that conf files can use its type_name
    Install {
        /// Broker type name (ex. zenoh)
        name: String,

        /// Version to install (default: the newest release that this juiz can load)
        #[arg(long = "version")]
        version: Option<String>,

        /// Archive to install (https://... or file://...), for broker types not in juiz's table
        #[arg(long = "from")]
        from: Option<String>,

        /// Install again even if the version is already installed
        #[arg(long = "force")]
        force: bool,
    },

    /// List the builtin broker types and the installed broker plugins
    List {},

    /// Print the directory of the installed broker plugin that juiz loads
    Path {
        /// Broker type name
        name: String,

        /// Version (default: the one that juiz loads)
        #[arg(long = "version")]
        version: Option<String>,
    },

    /// Remove an installed broker plugin (all versions unless --version)
    Remove {
        /// Broker type name
        name: String,

        /// Version to remove
        #[arg(long = "version")]
        version: Option<String>,
    },
}

/// `juiz broker ...` を実行して終了コードを返す
pub(crate) fn run_broker_command(subcommand: &BrokerSubCommands) -> i32 {
    let r = (|| -> Result<()> {
        let home = home()?;
        match subcommand {
            BrokerSubCommands::Install {
                name,
                version,
                from,
                force,
            } => {
                let base_url = std::env::var(BASE_URL_ENV).ok().filter(|s| !s.is_empty());
                let o = InstallOptions {
                    name,
                    version: version.as_deref(),
                    from: from.as_deref(),
                    force: *force,
                    home: &home,
                    base_url: base_url.as_deref(),
                    log: &|m| println!("{m}"),
                };
                match install(&o)? {
                    Outcome::Installed(m, dir) => {
                        println!(
                            "installed {} broker {} ({}, broker ABI {}) to {}",
                            m.type_name,
                            m.version,
                            m.plugin_name,
                            m.abi_version,
                            dir.display()
                        );
                        println!(
                            "Use it with 'type_name: {0}' in the conf, or IDs like {0}://<host>:<port>/…",
                            m.type_name
                        );
                    }
                    Outcome::AlreadyInstalled(version, dir) => println!(
                        "{name} broker {version} is already installed in {} (use --force to install it again)",
                        dir.display()
                    ),
                }
                Ok(())
            }
            BrokerSubCommands::List {} => {
                print!("{}", list_text(&home, &builtin_types()));
                Ok(())
            }
            BrokerSubCommands::Path { name, version } => {
                println!("{}", installed_path(&home, name, version.as_deref())?.display());
                Ok(())
            }
            BrokerSubCommands::Remove { name, version } => {
                for d in remove(&home, name, version.as_deref())? {
                    println!("removed {}", d.display());
                }
                Ok(())
            }
        }
    })();
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

fn home() -> Result<PathBuf> {
    broker_home().ok_or_else(|| anyhow!("cannot find the home directory. Set {BROKER_HOME_ENV}."))
}

/// この juiz に組み込んだブローカーの種類 (`juiz broker list` で builtin と出す)
fn builtin_types() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut v = vec!["http"];
    #[cfg(feature = "ipc-broker")]
    v.push("ipc");
    #[cfg(feature = "qmp-broker")]
    v.push("qmp");
    #[cfg(feature = "zenoh-broker")]
    v.push("zenoh");
    v
}

fn check_name(name: &str) -> Result<()> {
    if !is_valid_type_name(name) {
        bail!("'{name}' is not a broker type name (use letters, digits, '_' and '-')");
    }
    Ok(())
}

/// 版の指定を確かめる。先頭の `v` は省く
fn check_version(version: &str) -> Result<String> {
    let v = version.strip_prefix('v').unwrap_or(version);
    semver::Version::parse(v).map_err(|e| anyhow!("'{v}' is not a version like 0.1.0: {e}"))?;
    Ok(v.to_owned())
}

/// ホストの集合の ABI の版 (大きい順)
fn host_abis() -> Vec<u32> {
    let mut v = SUPPORTED_BROKER_ABI_VERSIONS.to_vec();
    v.sort_unstable_by(|a, b| b.cmp(a));
    v
}

fn archive_ext(target: &str) -> &'static str {
    if target.contains("-windows-") {
        "zip"
    } else {
        "tar.gz"
    }
}

/// アーカイブの名前 (§10.2)
pub(crate) fn archive_file_name(name: &str, version: &str, abi: u32, target: &str) -> String {
    format!(
        "juiz_broker-{name}-{version}-abi{abi}-{target}.{}",
        archive_ext(target)
    )
}

fn repo_of(name: &str) -> Option<&'static str> {
    NAME_TABLE.iter().find(|(n, _)| *n == name).map(|(_, r)| *r)
}

fn not_in_table(name: &str) -> juiz_core::anyhow::Error {
    anyhow!(
        "the broker type '{name}' is not in juiz's table ({}). Install it with 'juiz broker install {name} --from <URL>'.",
        NAME_TABLE.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
    )
}

pub(crate) struct InstallOptions<'a> {
    pub name: &'a str,
    pub version: Option<&'a str>,
    pub from: Option<&'a str>,
    pub force: bool,
    pub home: &'a Path,
    /// JUIZ_BROKER_BASE_URL
    pub base_url: Option<&'a str>,
    pub log: &'a dyn Fn(&str),
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Installed(BrokerPackageManifest, PathBuf),
    AlreadyInstalled(String, PathBuf),
}

/// 取得するアーカイブ
struct Archive {
    url: String,
    file_name: String,
    /// 中身
    data: Vec<u8>,
    /// .sha256 の中身。file:// で無かったときは None
    sums: Option<Vec<u8>>,
}

/// URL の中身を読む。無い (ファイルが無い・404) ときは None
fn try_fetch(url: &str) -> Result<Option<Vec<u8>>> {
    if let Some(p) = url.strip_prefix("file://") {
        if !Path::new(file_url_path(p)).is_file() {
            return Ok(None);
        }
        return fetch(url).map(Some);
    }
    match fetch(url) {
        Ok(d) => Ok(Some(d)),
        Err(e) if e.to_string().contains("404") => Ok(None),
        Err(e) => Err(e),
    }
}

/// file:///C:/... の先頭の / を外す (sdk.rs の fetch と同じ)
fn file_url_path(p: &str) -> &str {
    if cfg!(windows) && p.starts_with('/') && p.as_bytes().get(2) == Some(&b':') {
        &p[1..]
    } else {
        p
    }
}

/// URL を取得し、`.sha256` も取る。https で `.sha256` が無ければエラー
fn fetch_with_sums(url: &str, log: &dyn Fn(&str)) -> Result<Archive> {
    let file_name = url
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("'{url}' has no file name"))?
        .to_owned();
    log(&format!("downloading {url}"));
    let data = fetch(url)?;
    let sums_url = format!("{url}.sha256");
    let sums = try_fetch(&sums_url)?;
    if sums.is_none() && !url.starts_with("file://") {
        bail!("{sums_url} is not found. juiz installs a broker only with its checksum");
    }
    Ok(Archive {
        url: url.to_owned(),
        file_name,
        data,
        sums,
    })
}

/// `--from` の値を URL にする。`file://` も `https://` も付いていなければローカルのファイル
fn from_url(from: &str) -> Result<String> {
    if from.starts_with("file://") || from.starts_with("https://") || from.starts_with("http://") {
        return Ok(from.to_owned());
    }
    let p = std::fs::canonicalize(from).with_context(|| format!("cannot find {from}"))?;
    let s = p.display().to_string();
    // Windows の canonicalize は \\?\C:\... を返す
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/");
    Ok(if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    })
}

/// `--version` があるとき: ホストの集合の ABI の版を大きい順に試す
fn fetch_version(o: &InstallOptions, version: &str) -> Result<Archive> {
    let base = release_base(o)?;
    for abi in host_abis() {
        let file = archive_file_name(o.name, version, abi, HOST_TARGET);
        let url = format!("{}/v{version}/{file}", base.trim_end_matches('/'));
        if try_fetch(&format!("{url}.sha256"))?.is_some() || try_fetch_exists(&url)? {
            return fetch_with_sums(&url, o.log);
        }
    }
    bail!(
        "{} broker {version} for {HOST_TARGET} with broker ABI {} is not found in {base}/v{version}/",
        o.name,
        abi_list()
    )
}

/// 中身を落とさずに有るかだけ知りたいときの代わり (file:// だけ見る。https は .sha256 で見る)
fn try_fetch_exists(url: &str) -> Result<bool> {
    Ok(match url.strip_prefix("file://") {
        Some(p) => Path::new(file_url_path(p)).is_file(),
        None => false,
    })
}

fn abi_list() -> String {
    SUPPORTED_BROKER_ABI_VERSIONS
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 取得元の頭。JUIZ_BROKER_BASE_URL か、名前の表のリポジトリの releases/download
fn release_base(o: &InstallOptions) -> Result<String> {
    if let Some(b) = o.base_url {
        return Ok(b.to_owned());
    }
    let repo = repo_of(o.name).ok_or_else(|| not_in_table(o.name))?;
    Ok(format!("https://github.com/{repo}/releases/download"))
}

/// リリース 1 つ (GitHub の API の形の必要なところだけ、または file:// の `v*` のディレクトリ)
#[derive(Debug, Clone)]
struct Release {
    version: semver::Version,
    draft: bool,
    prerelease: bool,
    /// (アーカイブの名前, URL)
    assets: Vec<(String, String)>,
}

/// 版を省いたとき: リリースを新しい順に見て、ホストのターゲットとホストの集合の ABI の版のアーカイブを持つ最初のもの
fn fetch_latest(o: &InstallOptions) -> Result<Archive> {
    let releases = match o.base_url {
        Some(b) if b.starts_with("file://") => file_releases(b)?,
        _ => {
            let repo = repo_of(o.name).ok_or_else(|| not_in_table(o.name))?;
            github_releases(repo, o.log)?
        }
    };
    let (chosen, skipped) = choose_release(o.name, &releases)?;
    if let Some((ver, abi)) = skipped {
        let plugin = repo_of(o.name)
            .and_then(|r| r.rsplit('/').next())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("the {} broker", o.name));
        (o.log)(&format!(
            "note: {plugin} {ver} needs broker ABI {abi}; this juiz supports ABI {}. Installing {}.",
            abi_list(),
            chosen.0
        ));
    }
    let url = match o.base_url {
        // file:// 以外の頭が書いてあれば、API で選んだ版をそこから取る
        Some(b) if !b.starts_with("file://") => {
            format!("{}/v{}/{}", b.trim_end_matches('/'), chosen.0, chosen.1)
        }
        _ => chosen.2,
    };
    fetch_with_sums(&url, o.log)
}

/// (選んだ (版, アーカイブ名, URL), 飛ばした新しい版と その ABI の版)
#[allow(clippy::type_complexity)]
fn choose_release(
    name: &str,
    releases: &[Release],
) -> Result<((semver::Version, String, String), Option<(semver::Version, u32)>)> {
    let mut sorted: Vec<&Release> = releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease && r.version.pre.is_empty())
        .collect();
    sorted.sort_by(|a, b| b.version.cmp(&a.version));
    let mut skipped: Option<(semver::Version, u32)> = None;
    let ext = archive_ext(HOST_TARGET);
    for r in sorted {
        let v = r.version.to_string();
        for abi in host_abis() {
            let file = archive_file_name(name, &v, abi, HOST_TARGET);
            if let Some((_, url)) = r.assets.iter().find(|(n, _)| *n == file) {
                return Ok(((r.version.clone(), file, url.clone()), skipped));
            }
        }
        if skipped.is_none() {
            // このターゲットのアーカイブはあるが、ABI の版が合わない
            let prefix = format!("juiz_broker-{name}-{v}-abi");
            let suffix = format!("-{HOST_TARGET}.{ext}");
            let other = r.assets.iter().find_map(|(n, _)| {
                n.strip_prefix(&prefix)?
                    .strip_suffix(&suffix)?
                    .parse::<u32>()
                    .ok()
            });
            if let Some(abi) = other {
                skipped = Some((r.version.clone(), abi));
            }
        }
    }
    match skipped {
        Some((v, abi)) => bail!(
            "no release of the {name} broker can be loaded by this juiz. The newest one ({v}) needs broker ABI {abi}; this juiz supports ABI {}. Update juiz.",
            abi_list()
        ),
        None => bail!("no release of the {name} broker has an archive for {HOST_TARGET}"),
    }
}

/// `file://<ディレクトリ>` の `v*` のディレクトリをリリースとして並べる (テスト用、§10.4)
fn file_releases(base: &str) -> Result<Vec<Release>> {
    let dir = file_url_path(base.strip_prefix("file://").unwrap_or(base)).to_owned();
    let base = base.trim_end_matches('/');
    let mut out = Vec::new();
    let rd = std::fs::read_dir(&dir).with_context(|| format!("cannot read {dir}"))?;
    for e in rd.filter_map(|e| e.ok()) {
        let Some(n) = e.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(v) = n.strip_prefix('v').and_then(|v| semver::Version::parse(v).ok()) else {
            continue;
        };
        let assets = std::fs::read_dir(e.path())
            .map(|rd| {
                rd.filter_map(|f| f.ok())
                    .filter_map(|f| f.file_name().to_str().map(str::to_owned))
                    .map(|f| (f.clone(), format!("{base}/{n}/{f}")))
                    .collect()
            })
            .unwrap_or_default();
        out.push(Release {
            version: v,
            draft: false,
            prerelease: false,
            assets,
        });
    }
    Ok(out)
}

/// GitHub の REST API でリリースの一覧を取る。GITHUB_TOKEN があれば付ける
fn github_releases(repo: &str, log: &dyn Fn(&str)) -> Result<Vec<Release>> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=100");
    log(&format!("looking for releases in https://github.com/{repo}"));
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("juiz/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) {
        req = req.bearer_auth(t);
    }
    let res = req.send().with_context(|| format!("cannot get {url}"))?;
    let status = res.status();
    if !status.is_success() {
        bail!("{url}: {status}");
    }
    let v: serde_json::Value = res.json().with_context(|| format!("cannot read {url}"))?;
    parse_github_releases(&v)
}

fn parse_github_releases(v: &serde_json::Value) -> Result<Vec<Release>> {
    let arr = v
        .as_array()
        .ok_or_else(|| anyhow!("the GitHub API returned an unexpected value"))?;
    Ok(arr
        .iter()
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?;
            let version = semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
            let assets = r
                .get("assets")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((
                                x.get("name")?.as_str()?.to_owned(),
                                x.get("browser_download_url")?.as_str()?.to_owned(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(Release {
                version,
                draft: r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false),
                prerelease: r.get("prerelease").and_then(|d| d.as_bool()).unwrap_or(false),
                assets,
            })
        })
        .collect())
}

/// アーカイブを取得し、照合して置き場所に展開する (§10.4 の install)
pub(crate) fn install(o: &InstallOptions) -> Result<Outcome> {
    check_name(o.name)?;
    let version = o.version.map(check_version).transpose()?;
    // 版がわかっていれば、取得する前に入っているか見る
    if let Some(v) = &version {
        let dest = o.home.join(o.name).join(v);
        if !o.force && dest.join(juiz_core::installed::BROKER_PACKAGE_FILE).is_file() {
            return Ok(Outcome::AlreadyInstalled(v.clone(), dest));
        }
    }
    let archive = match (o.from, &version) {
        (Some(from), _) => fetch_with_sums(&from_url(from)?, o.log)?,
        (None, Some(v)) => fetch_version(o, v)?,
        (None, None) => fetch_latest(o)?,
    };
    match &archive.sums {
        Some(s) => verify_sha256(&archive.data, s).with_context(|| archive.file_name.clone())?,
        None => (o.log)(&format!(
            "warning: {}.sha256 is not found. The archive is not checked",
            archive.url
        )),
    }
    let ext = if archive.file_name.ends_with(".zip") {
        "zip"
    } else if archive.file_name.ends_with(".tar.gz") || archive.file_name.ends_with(".tgz") {
        "tar.gz"
    } else {
        bail!("{} is not a .tar.gz or .zip archive", archive.file_name);
    };

    let type_dir = o.home.join(o.name);
    std::fs::create_dir_all(&type_dir)
        .with_context(|| format!("cannot create {}", type_dir.display()))?;
    let work = type_dir.join(format!(".{}.tmp-{}", o.name, std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).with_context(|| format!("cannot create {}", work.display()))?;
    let r = (|| -> Result<Outcome> {
        extract(&archive.data, ext, &work)?;
        let m = check_extracted(o.name, &work).with_context(|| archive.file_name.clone())?;
        if let Some(v) = &version {
            if m.version != *v {
                bail!(
                    "{} is version {}, but --version {v} was given",
                    archive.file_name,
                    m.version
                );
            }
        }
        let dest = type_dir.join(&m.version);
        if dest.exists() {
            if !o.force {
                return Ok(Outcome::AlreadyInstalled(m.version.clone(), dest));
            }
            std::fs::remove_dir_all(&dest)
                .with_context(|| format!("cannot remove {}", dest.display()))?;
        }
        std::fs::rename(&work, &dest)
            .with_context(|| format!("cannot move {} to {}", work.display(), dest.display()))?;
        Ok(Outcome::Installed(m, dest))
    })();
    let _ = std::fs::remove_dir_all(&work);
    r
}

/// 展開した中身の juiz-broker.json を確かめる (§10.4 の install の 3)
fn check_extracted(name: &str, dir: &Path) -> Result<BrokerPackageManifest> {
    let m = BrokerPackageManifest::read(dir).map_err(|e| anyhow!("{e}"))?;
    if m.type_name != name {
        bail!("the archive is the broker type '{}', not '{name}'", m.type_name);
    }
    if !SUPPORTED_BROKER_ABI_VERSIONS.contains(&m.abi_version) {
        bail!(
            "the archive uses broker ABI {}, but this juiz supports ABI {}. Update juiz",
            m.abi_version,
            abi_list()
        );
    }
    if m.target != HOST_TARGET {
        bail!("the archive is built for {}, but this machine is {HOST_TARGET}", m.target);
    }
    let lib = m.library_path(dir);
    let bytes = std::fs::read(&lib)
        .with_context(|| format!("the library {} in juiz-broker.json is not found", m.library))?;
    match broker_abi_marker_in(&bytes).map_err(|e| anyhow!("{}: {e}", m.library))? {
        Some(v) if v == m.abi_version => {}
        Some(v) => bail!(
            "{} has the broker ABI marker juiz_broker_abi={v};, but juiz-broker.json says abi_version {}",
            m.library,
            m.abi_version
        ),
        None => bail!(
            "{} has no broker ABI marker (juiz_broker_abi={};). Is it a broker plugin with the C ABI?",
            m.library,
            m.abi_version
        ),
    }
    Ok(m)
}

/// `juiz broker list` の表示
fn list_text(home: &Path, builtin: &[&str]) -> String {
    let mut rows: Vec<[String; 5]> = vec![[
        "TYPE".into(),
        "VERSION".into(),
        "PLUGIN".into(),
        "STATUS".into(),
        "PATH".into(),
    ]];
    for b in builtin {
        rows.push([b.to_string(), "-".into(), "-".into(), "builtin".into(), "-".into()]);
    }
    for b in list_installed(home) {
        let plugin = b
            .manifest
            .as_ref()
            .map(|m| format!("{} (ABI {})", m.plugin_name, m.abi_version))
            .unwrap_or_else(|| "-".into());
        let mut status = b.status.to_string();
        if b.status == InstalledStatus::Used && builtin.contains(&b.type_name.as_str()) {
            // 組み込みが先に使われる (§6.4)
            status = "not used (builtin)".into();
        }
        rows.push([b.type_name, b.version, plugin, status, b.dir.display().to_string()]);
    }
    let mut widths = [0usize; 5];
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    let mut s = String::new();
    for r in &rows {
        let mut line = String::new();
        for (i, c) in r.iter().enumerate() {
            if i + 1 == r.len() {
                line.push_str(c);
            } else {
                line.push_str(c);
                line.push_str(&" ".repeat(widths[i] - c.chars().count() + 2));
            }
        }
        s.push_str(line.trim_end());
        s.push('\n');
    }
    if rows.len() == 1 + builtin.len() {
        s.push_str(&format!(
            "(no broker plugins are installed in {}. Install one with 'juiz broker install <name>')\n",
            home.display()
        ));
    }
    s
}

/// `juiz broker path` (§10.4)
fn installed_path(home: &Path, name: &str, version: Option<&str>) -> Result<PathBuf> {
    check_name(name)?;
    match version {
        None => select_installed(home, name).map(|b| b.dir).ok_or_else(|| {
            anyhow!(
                "no {name} broker that this juiz can load is installed in {}. Run 'juiz broker install {name}'.",
                home.display()
            )
        }),
        Some(v) => {
            let v = check_version(v)?;
            let b = list_installed_type(home, name)
                .into_iter()
                .find(|b| b.version == v)
                .ok_or_else(|| {
                    anyhow!(
                        "{name} broker {v} is not installed in {}. Run 'juiz broker install {name} --version {v}'.",
                        home.display()
                    )
                })?;
            match b.status {
                InstalledStatus::Used | InstalledStatus::Older => Ok(b.dir),
                s => bail!("{name} broker {v} in {} can not be loaded: {s}", b.dir.display()),
            }
        }
    }
}

/// `juiz broker remove` (§10.4)。消したディレクトリを返す
fn remove(home: &Path, name: &str, version: Option<&str>) -> Result<Vec<PathBuf>> {
    check_name(name)?;
    let type_dir = home.join(name);
    match version {
        Some(v) => {
            let v = check_version(v)?;
            let d = type_dir.join(&v);
            if !d.is_dir() {
                bail!("{name} broker {v} is not installed in {}", home.display());
            }
            std::fs::remove_dir_all(&d).with_context(|| format!("cannot remove {}", d.display()))?;
            // 最後の版なら種類のディレクトリも消す
            if std::fs::read_dir(&type_dir).map(|mut r| r.next().is_none()).unwrap_or(false) {
                let _ = std::fs::remove_dir(&type_dir);
            }
            Ok(vec![d])
        }
        None => {
            if !type_dir.is_dir() {
                bail!("no {name} broker is installed in {}", home.display());
            }
            let mut removed: Vec<PathBuf> = std::fs::read_dir(&type_dir)
                .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect())
                .unwrap_or_default();
            removed.sort();
            std::fs::remove_dir_all(&type_dir)
                .with_context(|| format!("cannot remove {}", type_dir.display()))?;
            Ok(removed)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::cell::RefCell;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_broker_cmd_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn lib_name() -> &'static str {
        if HOST_TARGET.contains("-windows-") {
            "lib/juiz_foo_broker.dll"
        } else if HOST_TARGET.contains("-apple-") {
            "lib/libjuiz_foo_broker.dylib"
        } else {
            "lib/libjuiz_foo_broker.so"
        }
    }

    /// §10.2 の並びのアーカイブを作る。library は中身 (印を入れるかどうかは呼ぶ側)
    pub(crate) fn make_archive(
        name: &str,
        version: &str,
        abi: u32,
        target: &str,
        library: &[u8],
    ) -> Vec<u8> {
        let lib = lib_name();
        let json = serde_json::to_string_pretty(&BrokerPackageManifest {
            format: 1,
            type_name: name.to_owned(),
            plugin_name: format!("juiz_{name}_broker"),
            version: version.to_owned(),
            abi_version: abi,
            target: target.to_owned(),
            library: lib.to_owned(),
        })
        .unwrap();
        let files: Vec<(&str, &[u8])> = vec![
            ("juiz-broker.json", json.as_bytes()),
            (lib, library),
            ("LICENSE-MIT", b"MIT\n"),
        ];
        if archive_ext(target) == "zip" {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (p, c) in &files {
                w.start_file(*p, zip::write::SimpleFileOptions::default()).unwrap();
                std::io::Write::write_all(&mut w, c).unwrap();
            }
            w.finish().unwrap().into_inner()
        } else {
            let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            for (p, c) in &files {
                let mut h = tar::Header::new_gnu();
                h.set_size(c.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, p, *c).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap()
        }
    }

    fn sha256_line(data: &[u8], file: &str) -> String {
        let h: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
        format!("{h}  {file}\n")
    }

    /// `<base>/v<版>/` にアーカイブと .sha256 を置く
    fn release(base: &Path, name: &str, version: &str, abi: u32, library: &[u8]) -> PathBuf {
        let data = make_archive(name, version, abi, HOST_TARGET, library);
        let dir = base.join(format!("v{version}"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = archive_file_name(name, version, abi, HOST_TARGET);
        std::fs::write(dir.join(&file), &data).unwrap();
        std::fs::write(dir.join(format!("{file}.sha256")), sha256_line(&data, &file)).unwrap();
        dir.join(file)
    }

    fn lib_with(abi: u32) -> Vec<u8> {
        format!("\x7fELF...juiz_broker_abi={abi};\0...").into_bytes()
    }

    fn file_url(p: &Path) -> String {
        from_url(p.to_str().unwrap()).unwrap()
    }

    fn opts<'a>(
        name: &'a str,
        home: &'a Path,
        base: Option<&'a str>,
        log: &'a dyn Fn(&str),
    ) -> InstallOptions<'a> {
        InstallOptions {
            name,
            version: None,
            from: None,
            force: false,
            home,
            base_url: base,
            log,
        }
    }

    #[test]
    fn archive_names() {
        assert_eq!(
            archive_file_name("zenoh", "0.1.0", 1, "x86_64-unknown-linux-gnu"),
            "juiz_broker-zenoh-0.1.0-abi1-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            archive_file_name("zenoh", "0.1.0", 1, "x86_64-pc-windows-msvc"),
            "juiz_broker-zenoh-0.1.0-abi1-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(repo_of("zenoh"), Some("OpenJUIZ/juiz_zenoh_broker"));
        assert!(not_in_table("foo").to_string().contains("--from <URL>"));
    }

    #[test]
    fn install_latest_version_path_and_remove() {
        let d = tmp("latest");
        let base = d.join("releases");
        release(&base, "foo", "0.1.0", 1, &lib_with(1));
        release(&base, "foo", "0.2.0", 1, &lib_with(1));
        // ABI が合わない新しい版は飛ばす
        release(&base, "foo", "0.3.0", 99, &lib_with(99));
        let base_url = file_url(&base);
        let home = d.join("home");
        let logs = RefCell::new(Vec::<String>::new());
        let log = |m: &str| logs.borrow_mut().push(m.to_owned());

        let r = install(&opts("foo", &home, Some(&base_url), &log)).unwrap();
        let Outcome::Installed(m, dir) = r else { panic!("{r:?}") };
        assert_eq!(m.version, "0.2.0");
        assert_eq!(dir, home.join("foo").join("0.2.0"));
        assert!(dir.join(lib_name()).is_file());
        assert!(
            logs.borrow().iter().any(|l| l.contains("needs broker ABI 99") && l.contains("Installing 0.2.0")),
            "{:?}",
            logs.borrow()
        );
        // 作業用ディレクトリは残らない
        assert_eq!(std::fs::read_dir(home.join("foo")).unwrap().count(), 1);

        // 同じ版は入れ直さない。--force なら入れ直す
        let r = install(&opts("foo", &home, Some(&base_url), &log)).unwrap();
        assert!(matches!(r, Outcome::AlreadyInstalled(ref v, _) if v == "0.2.0"), "{r:?}");
        let mut o = opts("foo", &home, Some(&base_url), &log);
        o.force = true;
        assert!(matches!(install(&o).unwrap(), Outcome::Installed(..)));

        // --version で古い版も入れる。path は新しいほう、--version ならその版
        let mut o = opts("foo", &home, Some(&base_url), &log);
        o.version = Some("v0.1.0");
        assert!(matches!(install(&o).unwrap(), Outcome::Installed(..)));
        assert!(matches!(install(&o).unwrap(), Outcome::AlreadyInstalled(..)));
        assert_eq!(installed_path(&home, "foo", None).unwrap(), home.join("foo").join("0.2.0"));
        assert_eq!(
            installed_path(&home, "foo", Some("0.1.0")).unwrap(),
            home.join("foo").join("0.1.0")
        );
        assert!(installed_path(&home, "foo", Some("0.9.0"))
            .unwrap_err()
            .to_string()
            .contains("juiz broker install foo --version 0.9.0"));
        assert!(installed_path(&home, "bar", None)
            .unwrap_err()
            .to_string()
            .contains("juiz broker install bar"));

        let text = list_text(&home, &["http", "qmp"]);
        assert!(text.contains("http "), "{text}");
        assert!(text.lines().any(|l| l.starts_with("foo") && l.contains("0.2.0") && l.contains("used")), "{text}");
        assert!(text.lines().any(|l| l.starts_with("foo") && l.contains("0.1.0") && l.contains("older")), "{text}");
        assert!(text.contains("juiz_foo_broker (ABI 1)"), "{text}");

        // remove
        assert_eq!(remove(&home, "foo", Some("0.1.0")).unwrap(), vec![home.join("foo").join("0.1.0")]);
        assert_eq!(remove(&home, "foo", None).unwrap(), vec![home.join("foo").join("0.2.0")]);
        assert!(!home.join("foo").exists());
        assert!(remove(&home, "foo", None).is_err());
        assert!(list_text(&home, &["http"]).contains("no broker plugins are installed"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_from_file() {
        let d = tmp("from");
        let home = d.join("home");
        let archive = release(&d.join("r"), "bar", "1.0.0", 1, &lib_with(1));
        // 表に無い種類は --from で入れる
        let e = install(&opts("bar", &home, None, &|_| {})).unwrap_err();
        assert!(e.to_string().contains("--from <URL>"), "{e}");

        let logs = RefCell::new(Vec::<String>::new());
        let log = |m: &str| logs.borrow_mut().push(m.to_owned());
        let path = archive.display().to_string();
        let mut o = opts("bar", &home, None, &log);
        o.from = Some(&path);
        assert!(matches!(install(&o).unwrap(), Outcome::Installed(..)));

        // file:// で .sha256 が無ければ警告を出して続ける
        std::fs::remove_file(format!("{path}.sha256")).unwrap();
        o.force = true;
        assert!(matches!(install(&o).unwrap(), Outcome::Installed(..)));
        assert!(logs.borrow().iter().any(|l| l.starts_with("warning:") && l.contains(".sha256")));

        // 名前が違う
        let mut o = opts("baz", &home, None, &|_| {});
        o.from = Some(&path);
        let e = install(&o).unwrap_err();
        assert!(format!("{e:#}").contains("the broker type 'bar', not 'baz'"), "{e:#}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn broken_archives_leave_nothing() {
        let d = tmp("broken");
        let home = d.join("home");
        let base = d.join("r");
        std::fs::create_dir_all(&base).unwrap();
        let base_url = file_url(&base);
        // 印が無い
        release(&base, "foo", "0.1.0", 1, b"no marker");
        let mut o = opts("foo", &home, Some(&base_url), &|_| {});
        o.version = Some("0.1.0");
        let e = install(&o).unwrap_err();
        assert!(format!("{e:#}").contains("has no broker ABI marker"), "{e:#}");
        // 印と juiz-broker.json が違う
        release(&base, "foo", "0.2.0", 1, &lib_with(2));
        o.version = Some("0.2.0");
        let e = install(&o).unwrap_err();
        assert!(format!("{e:#}").contains("juiz_broker_abi=2;"), "{e:#}");
        // .sha256 が違う
        let a = release(&base, "foo", "0.3.0", 1, &lib_with(1));
        std::fs::write(format!("{}.sha256", a.display()), format!("{}  x\n", "0".repeat(64))).unwrap();
        o.version = Some("0.3.0");
        let e = install(&o).unwrap_err();
        assert!(format!("{e:#}").contains("checksum mismatch"), "{e:#}");
        // 無い版
        o.version = Some("0.9.0");
        let e = install(&o).unwrap_err();
        assert!(e.to_string().contains("is not found"), "{e}");
        // 何も残さない
        let left: Vec<_> = std::fs::read_dir(home.join("foo")).map(|r| r.count()).into_iter().collect();
        assert!(left.is_empty() || left == vec![0], "{left:?}");
        // 名前に区切りは入れられない
        assert!(install(&opts("../x", &home, Some(&base_url), &|_| {})).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn github_release_listing() {
        let ok = archive_file_name("zenoh", "0.2.1", 1, HOST_TARGET);
        let newer = archive_file_name("zenoh", "0.3.0", 2, HOST_TARGET);
        let v = serde_json::json!([
            {"tag_name": "v0.4.0", "draft": true, "prerelease": false, "assets": [{"name": archive_file_name("zenoh", "0.4.0", 1, HOST_TARGET), "browser_download_url": "https://x/4"}]},
            {"tag_name": "v0.3.5", "draft": false, "prerelease": true, "assets": []},
            {"tag_name": "v0.3.0", "draft": false, "prerelease": false, "assets": [{"name": newer, "browser_download_url": "https://x/3"}]},
            {"tag_name": "v0.2.1", "draft": false, "prerelease": false, "assets": [{"name": ok, "browser_download_url": "https://x/2"}]},
            {"tag_name": "not-a-version", "assets": []}
        ]);
        let rs = parse_github_releases(&v).unwrap();
        assert_eq!(rs.len(), 4);
        let ((ver, file, url), skipped) = choose_release("zenoh", &rs).unwrap();
        assert_eq!(ver.to_string(), "0.2.1");
        assert_eq!(file, ok);
        assert_eq!(url, "https://x/2");
        assert_eq!(skipped.map(|(v, a)| (v.to_string(), a)), Some(("0.3.0".into(), 2)));
        let e = choose_release("zenoh", &rs[..3]).unwrap_err();
        assert!(e.to_string().contains("needs broker ABI 2"), "{e}");
        assert!(choose_release("zenoh", &[]).is_err());
    }
}
