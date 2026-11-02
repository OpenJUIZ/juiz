//! `juiz run <file.juizapp | name>`(docs/spec/app-package.md §7、§9、§10、§11)

use super::manifest::{check_format, exact_version, parse_req, Kind, Manifest};
use super::names::{check_entry_name, find_case_collision, is_reserved_path, normalize_relative};
use super::{depends, install};
use super::{sha256_hex_reader, yaml, JUIZ_VERSION, MANIFEST_FILE};
use juiz_core::anyhow::{anyhow, bail, Context, Result};
use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;
use juiz_sdk::build_info::BuildInfo;
use semver::Version;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::{CompressionMethod, ZipArchive};

/// 展開が終わったことを示す印(§7.2)
pub(crate) const COMPLETE_MARK: &str = ".juizapp-complete";
/// マニフェストの大きさの上限(§9 の段 1)
const MANIFEST_MAX: u64 = 1024 * 1024;
/// 展開後の合計の上限(§11)
const TOTAL_MAX: u64 = 4 * 1024 * 1024 * 1024;
/// 圧縮率の上限(§11)
const RATIO_MAX: u64 = 1000;

/// `juiz run` に渡す指定
#[derive(Debug, Clone, Default)]
pub(crate) struct RunOptions {
    pub package: String,
    pub entry: Option<String>,
    pub start_http_broker: bool,
    /// `--no-http`(issue #306)
    pub stop_http_broker: bool,
    pub ipc: Option<String>,
    pub namespace: Option<String>,
    /// `--depend <名前>=<URL>`(§10.2)
    pub depends: Vec<String>,
}

/// パッケージを動かそうとしている juiz(§1.1 のホスト)
#[derive(Debug, Clone)]
pub(crate) struct Host {
    pub juiz_version: Version,
    pub target: String,
    pub rustc: String,
    /// 読める Rust のプラグインの C ABI の版(rust-plugin-abi.md §9.2)
    pub rust_abi: Vec<u32>,
}

impl Host {
    pub(crate) fn current() -> Host {
        Host {
            juiz_version: Version::parse(JUIZ_VERSION).expect("the crate version is semver"),
            target: juiz_core::HOST_TARGET.to_owned(),
            rustc: BuildInfo::current().rustc_str().to_owned(),
            rust_abi: juiz_core::RUST_PLUGIN_ABI_VERSIONS.to_vec(),
        }
    }
}

/// 検証して展開したパッケージ
#[derive(Debug)]
pub(crate) struct Prepared {
    pub manifest: Manifest,
    /// 展開先(絶対パス)
    pub dir: PathBuf,
}

/// キャッシュの置き場所(§7.2)
pub(crate) fn cache_dir() -> Result<PathBuf> {
    if let Some(d) = std::env::var_os("JUIZ_CACHE_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    #[cfg(windows)]
    {
        if let Some(d) = std::env::var_os("LOCALAPPDATA").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(d).join("juiz").join("cache"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(d) = std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(d).join("juiz"));
        }
    }
    let home = home::home_dir()
        .ok_or_else(|| anyhow!("cannot find the home directory. Set JUIZ_CACHE_DIR."))?;
    #[cfg(windows)]
    return Ok(home
        .join("AppData")
        .join("Local")
        .join("juiz")
        .join("cache"));
    #[cfg(not(windows))]
    return Ok(home.join(".cache").join("juiz"));
}

/// `juiz run` の本体
pub(crate) fn run_package(opts: &RunOptions) -> Result<()> {
    let file = PathBuf::from(&opts.package);
    let host = Host::current();
    let data = install::data_dir()?;
    // 名前が .juizapp で終わるか、そのパスにファイルがあればファイル。それ以外はインストール済みの名前(§7)
    let prepared = if opts.package.ends_with(".juizapp") || file.is_file() {
        if !file.is_file() {
            bail!("{} is not found", opts.package);
        }
        prepare(&file, &cache_dir()?, &host)?
    } else {
        install::prepare_installed(&opts.package, &data, &host)?
    };
    let m = &prepared.manifest;
    check_python(m)?;
    let entry_raw = opts.entry.clone().unwrap_or_else(|| m.entry.clone());
    let entry = normalize_relative(&entry_raw)
        .filter(|e| !e.is_empty())
        .ok_or_else(|| anyhow!("entry '{entry_raw}' must be a relative path inside the package"))?;
    if !m.files.as_ref().is_some_and(|f| f.contains_key(&entry)) {
        bail!("entry conf '{entry}' is not in {} {}", m.name, m.version);
    }
    eprintln!(
        "Running {} {} from {}",
        m.name,
        m.version,
        prepared.dir.display()
    );
    start(&prepared, &prepared.dir.join(&entry), opts, &data)
}

/// 展開先の conf で起動する(§7.1 の 3〜5)
fn start(prepared: &Prepared, conf: &Path, opts: &RunOptions, data: &Path) -> Result<()> {
    let app_dir = &prepared.dir;
    let m = &prepared.manifest;
    // 環境変数は他のスレッドを立てる前に入れる
    std::env::set_var("JUIZ_APP_DIR", app_dir);
    let text =
        std::fs::read_to_string(conf).with_context(|| format!("cannot read {}", conf.display()))?;
    if !yaml::parse(&text)
        .map_err(|e| anyhow!("{}: {e}", conf.display()))?
        .is_object()
    {
        bail!("{} must be a map", conf.display());
    }
    let mut manifest = crate::namespace::override_namespace(
        yaml_conf_load(conf.display().to_string())?,
        opts.namespace.as_deref(),
    )?;
    // 4. 基礎の IPC・HTTP ブローカーと、depends のつなぎ先(§10)
    let mut ipc = opts.ipc.clone();
    if m.kind == Kind::Base {
        ipc = depends::apply_base(m, &mut manifest, opts.ipc.as_deref())?;
    }
    depends::set_package_profile(m, &mut manifest)?;
    let cli = opts
        .depends
        .iter()
        .map(|s| depends::parse_depend_arg(s))
        .collect::<Result<Vec<_>>>()?;
    let installed = install::list(data)?;
    let deps = depends::resolve(m, &mut manifest, &cli, &installed)?;
    let (ipc, needs_http) = depends::ensure_brokers(m, &mut manifest, &deps, ipc.as_deref())?;
    let manifest = crate::add_ipc_broker(manifest, ipc.as_deref())?;
    let working_dir = conf.parent().unwrap_or(app_dir);
    let mut system = System::new(manifest)?
        .set_working_dir(working_dir)
        .set_subsystem_check(subsystem_check(m, deps, &installed));
    if let Some(flag) = http_broker_flag(opts, needs_http)? {
        system = system.start_http_broker(flag);
    }
    let mut system = system.setup().map_err(|e| {
        // 依存のエラーは setup の文脈を付けずに見せる
        match e.downcast_ref::<depends::DependError>() {
            Some(d) => anyhow!(d.0.clone()),
            None => e,
        }
    })?;
    system.run_and_do(|_| Ok(()))
}

/// HTTP ブローカーを立てるかどうか(§7.1、issue #306)。None なら conf の `option.http_broker` に従う
/// (書いていなければ立つ)。
/// - `--http` は conf の `start: false` を上書きして立てる。`--no-http` と両方なら `--http`
/// - `--no-http` は conf に書いていなくても、`start: true` でも立てない
/// - http で基礎につなぐときは立てる。`--no-http` とは両立しないのでエラー
fn http_broker_flag(opts: &RunOptions, needs_http: bool) -> Result<Option<bool>> {
    if opts.start_http_broker {
        return Ok(Some(true));
    }
    if opts.stop_http_broker {
        if needs_http {
            bail!("a dependency is connected over http, which needs the HTTP broker. Remove --no-http or connect it over ipc");
        }
        return Ok(Some(false));
    }
    Ok(needs_http.then_some(true))
}

/// depends の subsystem につなぐ前に、相手の profile の package を確かめる(§10.2、§10.3)
fn subsystem_check(
    m: &Manifest,
    deps: Vec<depends::Depend>,
    installed: &[install::Installed],
) -> Box<juiz_core::SubsystemCheckFunctionType> {
    let app = m.name.clone();
    let installed_bases: BTreeSet<String> = installed
        .iter()
        .filter(|i| i.manifest.kind == Kind::Base)
        .map(|i| i.manifest.name.clone())
        .collect();
    Box::new(move |system: &System, sub: &Value| -> JuizResult<()> {
        let Some(name) = sub.get("package").and_then(Value::as_str) else {
            return Ok(());
        };
        let Some(dep) = deps.iter().find(|d| d.name == name) else {
            return Ok(());
        };
        let not_reached = || depends::connect_error(&app, dep, installed_bases.contains(name));
        let bp = system
            .core_broker()
            .lock()?
            .create_broker_proxy(sub.clone())
            .map_err(|_| not_reached())?;
        let profile = juiz_borrow(&bp)?
            .system_profile_full()
            .map_err(|_| not_reached())?;
        // http・ipc などの BrokerProxy は値を {"__value__": …} で包んで返す
        let profile = match profile.get("__value__") {
            Some(v) => v.clone(),
            None => profile,
        };
        depends::check_profile(&app, dep, &profile)?;
        Ok(())
    })
}

/// §9 の段 7・8(Python)
fn check_python(m: &Manifest) -> Result<()> {
    let needs =
        m.has_language("python") || !m.python_requires.is_empty() || m.python_version.is_some();
    if !needs {
        return Ok(());
    }
    if !juiz_core::PYTHON_ENABLED {
        bail!(
            "{} needs Python, but this juiz was built without Python support. Install juiz with: pip install juiz-cli",
            m.name
        );
    }
    let r = juiz_core::python_check(&m.python_requires, m.python_version.as_deref())?;
    if !r.version_ok {
        bail!(
            "{} {} needs Python {}, but this juiz uses Python {}.",
            m.name,
            m.version,
            m.python_version.as_deref().unwrap_or(""),
            r.version
        );
    }
    if !r.missing.is_empty() {
        let virtual_env = std::env::var("VIRTUAL_ENV").ok();
        // 新しく作った venv などで juiz モジュールも無ければ、一緒に入れるよう案内する(無いと Python のプラグインを読めない、#321)
        let mut to_install = r.missing.clone();
        if r.pyjuiz_missing {
            to_install.insert(0, "pyjuiz".to_owned());
        }
        bail!(
            "{} {} needs Python packages that are not installed: {}\n{}",
            m.name,
            m.version,
            r.missing.join(", "),
            pip_install_hint(
                &r.executable,
                &to_install,
                virtual_env.as_deref(),
                r.externally_managed,
                cfg!(windows)
            )
        );
    }
    Ok(())
}

/// 足りない Python のパッケージを入れる pip のコマンドを、貼り付けてそのまま通る形で作る (#304)。
///
/// - 有効にした venv の中の Python なら `python -m pip ...`(venv の python が PATH の先頭にあるので、どのシェルでも通る)
/// - externally-managed(PEP 668、Homebrew の Python など)なら pip で直接入れられないので、
///   venv を作って入れ、有効にしたまま juiz を動かす手順を示す (#321)
/// - パスに空白などが無ければ、そのパスのまま
/// - 空白などがあれば、Windows は PowerShell(`& "..."`)と cmd(`"..."`)の両方を、それ以外は `'...'` で囲んだ形を示す
fn pip_install_hint(
    executable: &str,
    packages: &[String],
    virtual_env: Option<&str>,
    externally_managed: bool,
    windows: bool,
) -> String {
    let args = packages
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let in_venv = virtual_env.is_some_and(|venv| {
        let norm = |s: &str| {
            let s = s.replace('\\', "/").trim_end_matches('/').to_owned();
            if windows {
                s.to_lowercase()
            } else {
                s
            }
        };
        norm(executable).starts_with(&format!("{}/", norm(venv)))
    });
    if in_venv {
        return format!("Install them with: python -m pip install {args}");
    }
    if externally_managed {
        let activate = if windows {
            r".venv\Scripts\activate"
        } else {
            "source .venv/bin/activate"
        };
        return format!(
            "{executable} is externally managed (PEP 668), so pip cannot install into it.\n\
             Make a venv, install them there, and run juiz again with the venv activated:\n  \
             {} -m venv .venv\n  \
             {activate}\n  \
             python -m pip install {args}",
            shell_word(executable, windows)
        );
    }
    if windows && needs_quote(executable) {
        return format!(
            "Install them with:\n  PowerShell: & \"{executable}\" -m pip install {args}\n  cmd:        \"{executable}\" -m pip install {args}"
        );
    }
    format!(
        "Install them with: {} -m pip install {args}",
        shell_word(executable, windows)
    )
}

fn needs_quote(executable: &str) -> bool {
    executable
        .chars()
        .any(|c| c.is_whitespace() || "\"'&()^%!;,$`".contains(c))
}

/// 実行ファイルのパスを、シェルにそのまま貼れる形にする(空白などがあれば囲む。Windows は PowerShell の形)
fn shell_word(executable: &str, windows: bool) -> String {
    if !needs_quote(executable) {
        executable.to_owned()
    } else if windows {
        format!("& \"{executable}\"")
    } else {
        format!("'{}'", executable.replace('\'', "'\\''"))
    }
}

/// §9 の段 1・2 を行い、パッケージ内のマニフェストに必須の項目を確かめる
pub(crate) fn open_package(file: &Path) -> Result<(ZipArchive<File>, Manifest)> {
    let label = file.display().to_string();
    let not_package = || anyhow!("{label} is not a juiz app package (no juizapp.yaml at the root)");

    // 1. 入れ物
    let f = File::open(file).with_context(|| format!("cannot open {label}"))?;
    let mut zip = ZipArchive::new(f).map_err(|_| not_package())?;
    let text = {
        let entry = zip.by_name(MANIFEST_FILE).map_err(|_| not_package())?;
        if entry.size() > MANIFEST_MAX {
            bail!("juizapp.yaml of {label} is larger than 1 MiB");
        }
        let mut s = String::new();
        entry
            .take(MANIFEST_MAX + 1)
            .read_to_string(&mut s)
            .with_context(|| format!("cannot read juizapp.yaml of {label}"))?;
        s
    };
    let value = yaml::parse(&text).map_err(|e| anyhow!("juizapp.yaml of {label}: {e}"))?;

    // 2. format
    check_format(&value, &label)?;
    let m = Manifest::from_value(value, &label)?;
    m.check_packed(&label)?;
    Ok((zip, m))
}

/// §9 の段 3(juiz の版)・4(ターゲット)
pub(crate) fn check_host(m: &Manifest, host: &Host) -> Result<()> {
    let id = format!("{} {}", m.name, m.version);
    // 3. juiz の版
    let range = m.juiz.as_deref().unwrap_or("*");
    let req = parse_req("'juiz'", range)?;
    if !req.matches(&host.juiz_version) {
        let mut msg = format!(
            "{id} requires juiz {range}, but this is juiz {}.",
            host.juiz_version
        );
        if let Some(v) = exact_version(&req) {
            msg.push_str(&format!(
                "\nInstall it with: cargo install juiz_app --version {v}  or  pip install juiz-cli=={v}"
            ));
        }
        bail!(msg);
    }

    // 4. ターゲット
    let targets = m.targets.clone().unwrap_or_default();
    let any = targets.len() == 1 && targets[0] == "any";
    if targets.iter().any(|t| t == "any") && !any {
        bail!("'targets' of {id} must not list 'any' with other targets");
    }
    if !any && !targets.iter().any(|t| t == &host.target) {
        bail!(
            "{id} is built for {}, but this machine is {}.",
            targets.join(", "),
            host.target
        );
    }
    Ok(())
}

/// §9 の段 6(Rust のプラグインのビルド情報)。C ABI のプラグインは ABI の版、Rust ABI のプラグインは rustc を比べる(§12.1)
pub(crate) fn check_rust_build(m: &Manifest, host: &Host) -> Result<()> {
    if !m.has_language("rust") {
        return Ok(());
    }
    if let Some(abi) = m.rust_abi {
        if !host.rust_abi.contains(&abi) {
            let supported = host
                .rust_abi
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "{} {} contains Rust plugins with plugin ABI {abi}, but this juiz supports ABI {supported}. Update juiz.",
                m.name,
                m.version
            );
        }
    }
    // 古い形のマニフェスト(rust.abi も rust.rustc も無い)は check_packed が止める。ここでは Rust ABI として扱う
    if m.rust_rustc.is_some() || m.rust_abi.is_none() {
        let rustc = m.rust_rustc.as_deref().unwrap_or("unknown");
        if rustc != host.rustc {
            bail!(
                "{} {} contains Rust plugins built with {rustc}, but this juiz was built with {}.",
                m.name,
                m.version,
                host.rustc
            );
        }
    }
    Ok(())
}

/// §9 の段 1〜6 を行い、キャッシュに展開する
pub(crate) fn prepare(file: &Path, cache_root: &Path, host: &Host) -> Result<Prepared> {
    let (mut zip, m) = open_package(file)?;
    check_host(&m, host)?;

    // 5. ファイル(キャッシュに展開済みなら省く)
    let hash = {
        let f = File::open(file)?;
        sha256_hex_reader(f)?
    };
    let apps = cache_root.join("apps");
    let dest = apps.join(format!("{}-{}-{}", m.name, m.version, &hash[..16]));
    if !dest.join(COMPLETE_MARK).is_file() {
        std::fs::create_dir_all(&apps)
            .with_context(|| format!("cannot create {}", apps.display()))?;
        extract(&mut zip, &m, &dest)?;
    }

    // 6. ビルド情報
    check_rust_build(&m, host)?;
    let dir = std::path::absolute(&dest)?;
    Ok(Prepared { manifest: m, dir })
}

/// zip の各エントリを §11 の規則で調べた結果
struct EntryInfo {
    index: usize,
    name: String,
    executable: bool,
}

/// §11 の規則で全エントリを調べてから、dest と同じ親の一時ディレクトリに展開し、`files` と照合して rename する
pub(crate) fn extract<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    m: &Manifest,
    dest: &Path,
) -> Result<()> {
    let entries = check_entries(zip, m)?;
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", dest.display()))?;
    let tmp = parent.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let result = (|| -> Result<()> {
        std::fs::create_dir_all(&tmp)?;
        let files = m.files.as_ref().unwrap();
        for e in &entries {
            let out_path = tmp.join(&e.name);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut entry = zip.by_index(e.index)?;
            let mut out = File::create(&out_path)?;
            let mut hasher = Sha256::new();
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = entry
                    .read(&mut buf)
                    .map_err(|err| anyhow!("cannot extract {}: {err}", e.name))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
            }
            drop(out);
            if e.name != MANIFEST_FILE {
                let actual = format!("sha256:{}", super::to_hex(&hasher.finalize()));
                if files.get(&e.name) != Some(&actual) {
                    bail!("hash mismatch: {}", e.name);
                }
            }
            set_mode(&out_path, e.executable)?;
        }
        File::create(tmp.join(COMPLETE_MARK))?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_dir_all(&tmp);
        // 別の起動が先に展開し終えていれば、それを使う
        if !dest.join(COMPLETE_MARK).is_file() {
            return Err(anyhow!(
                "cannot move the package to {}: {e}",
                dest.display()
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

/// §11 の規則と、`files` との集合の照合(§9.1)
fn check_entries<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    m: &Manifest,
) -> Result<Vec<EntryInfo>> {
    let unsafe_entry = |name: &str, why: &str| anyhow!("unsafe entry in package: {name} ({why})");
    let mut out = Vec::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i)?;
        let raw = e.name_raw().to_vec();
        let name = String::from_utf8(raw).map_err(|err| {
            unsafe_entry(
                &String::from_utf8_lossy(err.as_bytes()),
                "the name is not UTF-8",
            )
        })?;
        if e.is_dir() {
            let trimmed = name.trim_end_matches('/');
            check_entry_name(trimmed).map_err(|why| unsafe_entry(&name, &why))?;
            continue;
        }
        check_entry_name(&name).map_err(|why| unsafe_entry(&name, &why))?;
        let mode = e.unix_mode();
        if mode.is_some_and(|m| m & 0o170000 == 0o120000) || e.is_symlink() {
            return Err(unsafe_entry(&name, "symbolic link"));
        }
        if e.encrypted() {
            return Err(unsafe_entry(&name, "encrypted"));
        }
        if !matches!(
            e.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(unsafe_entry(&name, "unsupported compression method"));
        }
        let size = e.size();
        let compressed = e.compressed_size();
        if size > 0 && (compressed == 0 || size / compressed > RATIO_MAX) {
            return Err(unsafe_entry(&name, "compression ratio is too high"));
        }
        total = total.saturating_add(size);
        if total > TOTAL_MAX {
            return Err(unsafe_entry(
                &name,
                "the package is larger than 4 GiB when extracted",
            ));
        }
        if is_reserved_path(&name) {
            return Err(unsafe_entry(&name, "reserved path in package format 1"));
        }
        out.push(EntryInfo {
            index: i,
            executable: mode.is_some_and(|m| m & 0o111 != 0),
            name,
        });
    }
    if let Some(dup) = find_case_collision(out.iter().map(|e| e.name.as_str())) {
        return Err(unsafe_entry(&dup, "duplicated path (case-insensitive)"));
    }
    let files = m.files.as_ref().unwrap();
    let names: BTreeSet<&str> = out
        .iter()
        .map(|e| e.name.as_str())
        .filter(|n| *n != MANIFEST_FILE)
        .collect();
    if let Some(n) = names.iter().find(|n| !files.contains_key(**n)) {
        bail!("unexpected file in package: {n}");
    }
    if let Some(n) = files.keys().find(|k| !names.contains(k.as_str())) {
        bail!("missing file in package: {n}");
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::package::to_hex;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    pub(crate) fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_run_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn http_broker_flag_follows_b_and_q() {
        let o = |b, q| RunOptions {
            start_http_broker: b,
            stop_http_broker: q,
            ..Default::default()
        };
        // 指定なし: conf に従う。http でつなぐときは立てる
        assert_eq!(http_broker_flag(&o(false, false), false).unwrap(), None);
        assert_eq!(
            http_broker_flag(&o(false, false), true).unwrap(),
            Some(true)
        );
        // --http: 立てる
        assert_eq!(
            http_broker_flag(&o(true, false), false).unwrap(),
            Some(true)
        );
        // --no-http: 立てない。--http と両方なら --http
        assert_eq!(
            http_broker_flag(&o(false, true), false).unwrap(),
            Some(false)
        );
        assert_eq!(http_broker_flag(&o(true, true), false).unwrap(), Some(true));
        assert_eq!(http_broker_flag(&o(true, true), true).unwrap(), Some(true));
        // --no-http と http での接続は両立しない
        let e = http_broker_flag(&o(false, true), true)
            .unwrap_err()
            .to_string();
        assert!(e.contains("--no-http"), "{e}");
    }

    pub(crate) fn host() -> Host {
        Host {
            juiz_version: Version::parse("0.0.28").unwrap(),
            target: "x86_64-unknown-linux-gnu".to_owned(),
            rustc: "rustc 1.91.0 (f8297e351 2025-10-28)".to_owned(),
            rust_abi: vec![1],
        }
    }

    fn sha(b: &[u8]) -> String {
        format!("sha256:{}", to_hex(&Sha256::digest(b)))
    }

    /// manifest の files を entries から作り(override で書き換えられる)、zip を書く
    pub(crate) fn write_pkg(
        path: &Path,
        head: &str,
        entries: &[(&str, &[u8])],
        files_override: Option<&str>,
    ) {
        let files = files_override.map(str::to_owned).unwrap_or_else(|| {
            let mut s = String::from("files:\n");
            for (n, b) in entries {
                s.push_str(&format!("  \"{n}\": \"{}\"\n", sha(b)));
            }
            s
        });
        let manifest = format!("{head}\n{files}");
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let o = SimpleFileOptions::default();
        zip.start_file(MANIFEST_FILE, o).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        for (n, b) in entries {
            zip.start_file(*n, o).unwrap();
            zip.write_all(b).unwrap();
        }
        zip.finish().unwrap();
    }

    pub(crate) const HEAD: &str = "format: 1\nname: app\nversion: 0.1.0\njuiz: \">=0.0.28\"\nentry: app.conf\nlanguages: [python]\ntargets: [any]";

    #[test]
    fn pip_install_hint_can_be_pasted() {
        let pkgs = vec!["six>=1.16".to_owned(), "numpy".to_owned()];
        // パスに空白が無ければ、そのまま
        assert_eq!(
            pip_install_hint(r"C:\Python310\python.exe", &pkgs, None, false, true),
            r#"Install them with: C:\Python310\python.exe -m pip install "six>=1.16" "numpy""#
        );
        // 有効にした venv の中の Python なら python -m pip(大文字小文字と区切りの違いは Windows では無視する)
        assert_eq!(
            pip_install_hint(
                r"C:\Users\Taro Yamada\app\.venv\Scripts\python.exe",
                &pkgs,
                Some(r"c:/users/taro yamada/app/.venv"),
                false,
                true
            ),
            r#"Install them with: python -m pip install "six>=1.16" "numpy""#
        );
        assert_eq!(
            pip_install_hint(
                "/home/me/my app/.venv/bin/python3",
                &pkgs,
                Some("/home/me/my app/.venv/"),
                false,
                false
            ),
            r#"Install them with: python -m pip install "six>=1.16" "numpy""#
        );
        // venv の外の Python で、パスに空白があれば囲む(Windows は PowerShell と cmd の両方)
        assert_eq!(
            pip_install_hint(r"C:\Users\Taro Yamada\AppData\Local\Programs\Python\Python310\python.exe", &pkgs, Some(r"C:\other\.venv"), false, true),
            "Install them with:\n  PowerShell: & \"C:\\Users\\Taro Yamada\\AppData\\Local\\Programs\\Python\\Python310\\python.exe\" -m pip install \"six>=1.16\" \"numpy\"\n  cmd:        \"C:\\Users\\Taro Yamada\\AppData\\Local\\Programs\\Python\\Python310\\python.exe\" -m pip install \"six>=1.16\" \"numpy\""
        );
        assert_eq!(
            pip_install_hint("/opt/my python/bin/python3", &pkgs, None, false, false),
            r#"Install them with: '/opt/my python/bin/python3' -m pip install "six>=1.16" "numpy""#
        );
        // venv の名前で始まる別のディレクトリは venv の中とみなさない
        assert_eq!(
            pip_install_hint(
                "/home/me/.venv2/bin/python3",
                &pkgs,
                Some("/home/me/.venv"),
                false,
                false
            ),
            r#"Install them with: /home/me/.venv2/bin/python3 -m pip install "six>=1.16" "numpy""#
        );
        // externally-managed(Homebrew の Python など)なら、venv を作って入れる手順 (#321)
        let homebrew = "/opt/homebrew/opt/python@3.14/Frameworks/Python.framework/Versions/3.14/bin/python3.14";
        assert_eq!(
            pip_install_hint(homebrew, &pkgs, None, true, false),
            format!(
                "{homebrew} is externally managed (PEP 668), so pip cannot install into it.\n\
                 Make a venv, install them there, and run juiz again with the venv activated:\n  \
                 {homebrew} -m venv .venv\n  \
                 source .venv/bin/activate\n  \
                 python -m pip install \"six>=1.16\" \"numpy\""
            )
        );
        assert!(pip_install_hint("/opt/my python/bin/python3", &pkgs, None, true, false)
            .contains("\n  '/opt/my python/bin/python3' -m venv .venv\n"));
        assert!(pip_install_hint(r"C:\Python 3\python.exe", &pkgs, None, true, true)
            .contains("\n  & \"C:\\Python 3\\python.exe\" -m venv .venv\n  .venv\\Scripts\\activate\n"));
        // venv の中なら externally-managed かどうかによらず python -m pip
        assert_eq!(
            pip_install_hint("/home/me/.venv/bin/python", &pkgs, Some("/home/me/.venv"), true, false),
            r#"Install them with: python -m pip install "six>=1.16" "numpy""#
        );
    }

    #[test]
    fn extracts_and_reuses_cache() {
        let d = tmp("ok");
        let pkg = d.join("app-0.1.0.juizapp");
        write_pkg(
            &pkg,
            HEAD,
            &[("app.conf", b"name: a\n"), ("python/x.py", b"print(1)\n")],
            None,
        );
        let p = prepare(&pkg, &d.join("cache"), &host()).unwrap();
        assert!(p.dir.join(COMPLETE_MARK).is_file());
        assert_eq!(
            std::fs::read(p.dir.join("python/x.py")).unwrap(),
            b"print(1)\n"
        );
        assert!(p.dir.join(MANIFEST_FILE).is_file());
        let name = p.dir.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(
            name.starts_with("app-0.1.0-") && name.len() == "app-0.1.0-".len() + 16,
            "{name}"
        );
        // 展開済みなら中身を見ない(消したファイルが戻らない)
        std::fs::remove_file(p.dir.join("python/x.py")).unwrap();
        let p2 = prepare(&pkg, &d.join("cache"), &host()).unwrap();
        assert_eq!(p2.dir, p.dir);
        assert!(!p2.dir.join("python/x.py").exists());
        // 一時ディレクトリは残らない
        let left: Vec<_> = std::fs::read_dir(d.join("cache/apps")).unwrap().collect();
        assert_eq!(left.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    fn prepare_err(
        name: &str,
        head: &str,
        entries: &[(&str, &[u8])],
        files: Option<&str>,
        h: &Host,
    ) -> String {
        let d = tmp(name);
        let pkg = d.join("p.juizapp");
        write_pkg(&pkg, head, entries, files);
        let e = prepare(&pkg, &d.join("cache"), h).unwrap_err().to_string();
        // 展開途中の一時ディレクトリは残さない
        let apps = d.join("cache/apps");
        if apps.exists() {
            for entry in std::fs::read_dir(&apps).unwrap() {
                let name = entry.unwrap().file_name();
                assert!(!name.to_string_lossy().starts_with(".tmp-"), "{e}");
            }
        }
        let _ = std::fs::remove_dir_all(&d);
        e
    }

    #[test]
    fn rust_build_checks() {
        let h = host();
        let parse = |rust: &str| {
            Manifest::parse_str(
                &format!("format: 1\nname: app\nversion: 0.1.0\nlanguages: [rust]\nrust: {rust}"),
                "t",
            )
            .unwrap()
        };
        // C ABI だけなら rustc は問わない
        check_rust_build(&parse("{juiz_sdk: 0.0.30, abi: 1}"), &h).unwrap();
        assert!(check_rust_build(&parse("{juiz_sdk: 0.0.30, abi: 9}"), &h).is_err());
        // Rust ABI は rustc を比べる
        check_rust_build(
            &parse("{juiz_sdk: 0.0.30, rustc: rustc 1.91.0 (f8297e351 2025-10-28)}"),
            &h,
        )
        .unwrap();
        assert!(check_rust_build(&parse("{juiz_sdk: 0.0.30, rustc: x}"), &h).is_err());
        assert!(check_rust_build(&parse("{juiz_sdk: 0.0.30, abi: 1, rustc: x}"), &h).is_err());
    }

    #[test]
    fn rejects_by_stage() {
        let conf: &[(&str, &[u8])] = &[("app.conf", b"name: a\n")];
        let h = host();
        let e = prepare_err(
            "fmt",
            "format: 2\nname: app\nversion: 0.1.0",
            conf,
            None,
            &h,
        );
        assert!(e.contains("uses package format 2"), "{e}");
        let e = prepare_err(
            "ver",
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \"=0.0.29\"\nlanguages: [python]\ntargets: [any]",
            conf,
            None,
            &h,
        );
        assert!(
            e.contains("requires juiz =0.0.29, but this is juiz 0.0.28"),
            "{e}"
        );
        assert!(e.contains("cargo install juiz_app --version 0.0.29"), "{e}");
        let e = prepare_err(
            "tgt",
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \">=0.0.1\"\nlanguages: [rust]\ntargets: [aarch64-apple-darwin]\nrust: {juiz_sdk: 0.0.28, rustc: x}",
            conf,
            None,
            &h,
        );
        assert!(
            e.contains(
                "is built for aarch64-apple-darwin, but this machine is x86_64-unknown-linux-gnu"
            ),
            "{e}"
        );
        let e = prepare_err(
            "rustc",
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \"=0.0.28\"\nlanguages: [rust]\ntargets: [x86_64-unknown-linux-gnu]\nrust: {juiz_sdk: 0.0.28, rustc: rustc 1.90.0}",
            conf,
            None,
            &h,
        );
        assert!(
            e.contains("built with rustc 1.90.0, but this juiz was built with rustc 1.91.0"),
            "{e}"
        );
        let e = prepare_err(
            "abi",
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \">=0.0.28\"\nlanguages: [rust]\ntargets: [x86_64-unknown-linux-gnu]\nrust: {juiz_sdk: 0.0.28, abi: 2}",
            conf,
            None,
            &h,
        );
        assert!(
            e.contains("contains Rust plugins with plugin ABI 2, but this juiz supports ABI 1. Update juiz."),
            "{e}"
        );
        // C ABI と Rust ABI の両方を含むときは、両方を比べる
        let e = prepare_err(
            "abiandrustc",
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \"=0.0.28\"\nlanguages: [rust]\ntargets: [x86_64-unknown-linux-gnu]\nrust: {juiz_sdk: 0.0.28, abi: 1, rustc: rustc 1.90.0}",
            conf,
            None,
            &h,
        );
        assert!(e.contains("built with rustc 1.90.0"), "{e}");
        let e = prepare_err(
            "noabinorustc",
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \">=0.0.28\"\nlanguages: [rust]\ntargets: [x86_64-unknown-linux-gnu]\nrust: {juiz_sdk: 0.0.28}",
            conf,
            None,
            &h,
        );
        assert!(e.contains("'rust.abi' or 'rust.rustc' are missing"), "{e}");
        let e = prepare_err(
            "missingfield",
            "format: 1\nname: app\nversion: 0.1.0",
            conf,
            None,
            &h,
        );
        assert!(e.contains("'juiz' is missing"), "{e}");
    }

    #[test]
    fn rejects_file_mismatches() {
        let h = host();
        let e = prepare_err(
            "hash",
            HEAD,
            &[("app.conf", b"name: a\n")],
            Some(&format!("files:\n  app.conf: \"{}\"\n", sha(b"other"))),
            &h,
        );
        assert_eq!(e, "hash mismatch: app.conf");
        let e = prepare_err(
            "unexpected",
            HEAD,
            &[("app.conf", b"name: a\n"), ("extra", b"x")],
            Some(&format!("files:\n  app.conf: \"{}\"\n", sha(b"name: a\n"))),
            &h,
        );
        assert_eq!(e, "unexpected file in package: extra");
        let e = prepare_err(
            "missing",
            HEAD,
            &[("app.conf", b"name: a\n")],
            Some(&format!(
                "files:\n  app.conf: \"{}\"\n  gone.py: \"{}\"\n",
                sha(b"name: a\n"),
                sha(b"")
            )),
            &h,
        );
        assert_eq!(e, "missing file in package: gone.py");
    }

    #[test]
    fn rejects_unsafe_entries() {
        let h = host();
        for (name, why) in [
            ("../evil", "'..'"),
            ("/abs", "absolute"),
            ("a\\b", "'\\'"),
            ("CON", "reserved"),
            ("bin/x86_64-unknown-linux-gnu/juiz", "reserved path"),
            ("juizapp.yaml.sig", "reserved path"),
        ] {
            let e = prepare_err(
                "unsafe",
                HEAD,
                &[("app.conf", b"name: a\n"), (name, b"x")],
                None,
                &h,
            );
            assert!(
                e.starts_with(&format!("unsafe entry in package: {name}")) && e.contains(why),
                "{name}: {e}"
            );
        }
        let e = prepare_err("case", HEAD, &[("A.so", b"x"), ("a.so", b"y")], None, &h);
        assert!(e.contains("duplicated path"), "{e}");
    }

    #[test]
    fn rejects_non_package() {
        let d = tmp("nonpkg");
        let p = d.join("x.juizapp");
        std::fs::write(&p, b"not a zip").unwrap();
        let e = prepare(&p, &d.join("cache"), &host())
            .unwrap_err()
            .to_string();
        assert!(
            e.ends_with("is not a juiz app package (no juizapp.yaml at the root)"),
            "{e}"
        );
        let mut zip = ZipWriter::new(File::create(&p).unwrap());
        zip.start_file("app.conf", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        let e = prepare(&p, &d.join("cache"), &host())
            .unwrap_err()
            .to_string();
        assert!(e.contains("no juizapp.yaml"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
