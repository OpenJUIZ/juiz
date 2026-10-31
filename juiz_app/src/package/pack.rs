//! `juiz pack <dir>`(docs/spec/app-package.md §6)

use super::manifest::{exact_version, parse_req, Kind, Manifest, LANGUAGES};
use super::names::{check_entry_name, find_case_collision, is_reserved_path, normalize_relative};
use super::{
    plugin_file_name, sha256_hex_reader, warn, yaml, JUIZ_VERSION, KNOWN_TARGETS, MANIFEST_FILE,
};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use juiz_core::anyhow::{anyhow, bail, Context, Result};
use juiz_sdk::build_info::BuildInfo;
use juiz_sdk::serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// pack に渡す指定
#[derive(Debug, Clone)]
pub(crate) struct PackOptions {
    pub dir: PathBuf,
    pub output: Option<PathBuf>,
    pub targets: Vec<String>,
}

/// 集めたファイル 1 つ
#[derive(Debug, Clone)]
struct Collected {
    abs: PathBuf,
    executable: bool,
}

/// pack の結果
#[derive(Debug)]
pub(crate) struct Packed {
    pub path: PathBuf,
    pub manifest: Manifest,
}

/// 集めないディレクトリ(§6.2)
const SKIP_DIRS: &[&str] = &["target", "__pycache__", "dist-newstyle", "build"];

pub(crate) fn pack(opts: &PackOptions) -> Result<Packed> {
    let dir = &opts.dir;
    let manifest_path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("cannot read {}", manifest_path.display()))?;
    let file_label = dir.display().to_string();
    let mut m = Manifest::parse_str(&text, &file_label)?;

    for t in &opts.targets {
        if t == "any" {
            bail!("'--target any' is not allowed; 'any' is written when the package has no native plugins");
        }
    }
    let files = collect_files(dir, &m.exclude, &opts.targets)?;

    // 2. entry の conf
    let entry = normalize_relative(&m.entry)
        .filter(|e| !e.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "entry '{}' must be a relative path inside the package",
                m.entry
            )
        })?;
    let entry_file = files
        .get(&entry)
        .ok_or_else(|| anyhow!("entry conf '{entry}' is not found in {}", dir.display()))?;
    let conf_text = std::fs::read_to_string(&entry_file.abs)
        .with_context(|| format!("cannot read entry conf {}", entry_file.abs.display()))?;
    let conf = yaml::parse(&conf_text).map_err(|e| anyhow!("entry conf '{entry}': {e}"))?;

    // 5. targets(3 のプラグインの確認に使うので先に求める)
    let dir_targets: BTreeSet<String> = files
        .keys()
        .filter_map(|p| {
            let rest = p.strip_prefix("plugins/")?;
            let (t, _) = rest.split_once('/')?;
            Some(t.to_owned())
        })
        .collect();
    let plugins = list_plugins(&conf)?;
    let has_native = plugins.iter().any(|p| p.native);
    let targets: Vec<String> = if !has_native {
        vec!["any".to_owned()]
    } else if !opts.targets.is_empty() {
        for t in &opts.targets {
            if !dir_targets.contains(t) {
                bail!(
                    "--target {t}: plugins/{t}/ is not found in {}",
                    dir.display()
                );
            }
        }
        opts.targets.clone()
    } else {
        if dir_targets.is_empty() {
            bail!("the entry conf has native plugins, but no plugins/<target>/ directory is found in {}", dir.display());
        }
        dir_targets.into_iter().collect()
    };
    for t in &targets {
        if t != "any" && !KNOWN_TARGETS.contains(&t.as_str()) {
            warn(&format!(
                "target '{t}' is not one of the usual targets {KNOWN_TARGETS:?}"
            ));
        }
    }
    if let Some(written) = &m.targets {
        if as_set(written) != as_set(&targets) {
            bail!("'targets' in juizapp.yaml is {written:?}, but the package has {targets:?}");
        }
    }

    // 3. 各プラグインのファイル
    let mut rust_files: Vec<RustFile> = Vec::new();
    let mut host_native: Vec<String> = Vec::new();
    for p in &plugins {
        let raw = &p.path;
        if p.native && !raw.contains("{target}") {
            bail!("the path '{raw}' of the {} plugin '{}' must contain '{{target}}' (ex. plugins/{{target}})", p.language, p.name);
        }
        let check_targets: Vec<&str> = if p.native {
            targets.iter().map(String::as_str).collect()
        } else {
            vec!["any"]
        };
        for t in check_targets {
            let path = raw.replace("{target}", t);
            let base = normalize_relative(&path).ok_or_else(|| {
                anyhow!("the path '{raw}' of the plugin '{}' must be a relative path inside the package", p.name)
            })?;
            let file_name = if p.native {
                plugin_file_name(t, &p.name)
            } else {
                format!("{}.py", p.name)
            };
            let rel = if base.is_empty() {
                file_name
            } else {
                format!("{base}/{file_name}")
            };
            if !files.contains_key(&rel) {
                bail!(
                    "the plugin file '{rel}' of '{}' is not found in {}",
                    p.name,
                    dir.display()
                );
            }
            if p.native && t == juiz_core::HOST_TARGET {
                host_native.push(rel.clone());
            }
            if p.language == "rust" {
                rust_files.push(RustFile {
                    rel,
                    forced_rust_abi: p.rust_abi_reason,
                });
            }
        }
    }

    // 4. languages
    let used: BTreeSet<&str> = plugins.iter().map(|p| p.language.as_str()).collect();
    let languages: Vec<String> = LANGUAGES
        .iter()
        .filter(|l| used.contains(*l))
        .map(|l| l.to_string())
        .collect();
    if let Some(written) = &m.languages {
        if as_set(written) != as_set(&languages) {
            bail!("'languages' in juizapp.yaml is {written:?}, but the plugins in the entry conf use {languages:?}");
        }
    }
    if languages.iter().any(|l| l == "c++" || l == "haskell") {
        for t in &targets {
            let sdk = format!("plugins/{t}/{}", plugin_file_name(t, "juiz_sdk"));
            if !files.contains_key(&sdk) {
                warn(&format!(
                    "C++ / Haskell plugins are in the package, but {sdk} is not"
                ));
            }
        }
    }

    // 依存する共有ライブラリが、受け取る側でも見つかるか(ホストのターゲットの分だけ見られる、§12.2)
    for rel in &host_native {
        for w in dependency_warnings(&files[rel].abs) {
            warn(&format!("{rel} {w}"));
        }
    }

    // 6. Rust のプラグインのビルド情報(C ABI か Rust ABI か、§12.1)
    m.rust_juiz_sdk = None;
    m.rust_abi = None;
    m.rust_rustc = None;
    if !rust_files.is_empty() {
        let rust = classify_rust_plugins(&rust_files, |rel| files[rel].abs.clone())?;
        m.rust_juiz_sdk = Some(rust.juiz_sdk.clone());
        m.rust_abi = rust.abi;
        if let Some((rustc, _)) = &rust.rust_abi {
            let sdk = &rust.juiz_sdk;
            match &m.juiz {
                None => m.juiz = Some(format!("={sdk}")),
                Some(j) => {
                    let ok = exact_version(&parse_req("'juiz'", j)?)
                        .is_some_and(|v| v.to_string() == *sdk);
                    if !ok {
                        bail!("the package has Rust plugins loaded through the Rust ABI ({}), built with juiz_sdk {sdk}, so 'juiz' in juizapp.yaml must be \"={sdk}\", but \"{j}\"", rust.rust_abi_files().join(", "));
                    }
                }
            }
            m.rust_rustc = Some(rustc.clone());
        } else {
            match &m.juiz {
                None => m.juiz = Some(format!(">={JUIZ_VERSION}")),
                Some(j) => {
                    let req = parse_req("'juiz'", j)?;
                    if let Some(w) = old_juiz_warning(j, &req) {
                        warn(&w);
                    }
                }
            }
        }
    } else if m.juiz.is_none() {
        m.juiz = Some(format!(">={JUIZ_VERSION}"));
    }

    // 7. 基礎の http_port と conf のポート
    if m.kind == Kind::Base {
        if let (Some(port), Some(conf_port)) = (
            m.base_http_port,
            conf.pointer("/option/http_broker/port")
                .filter(|v| !v.is_null()),
        ) {
            if conf_port.as_u64() != Some(port as u64) {
                bail!("'base.http_port' in juizapp.yaml is {port}, but option.http_broker.port in the entry conf is {conf_port}");
            }
        }
    }

    // 9. files
    let mut hashes = BTreeMap::new();
    for (rel, c) in &files {
        let f = File::open(&c.abs).with_context(|| format!("cannot open {}", c.abs.display()))?;
        hashes.insert(rel.clone(), format!("sha256:{}", sha256_hex_reader(f)?));
    }
    m.languages = Some(languages);
    m.targets = Some(targets);
    m.files = Some(hashes);

    // 10. zip
    let out = output_path(opts, &m)?;
    write_zip(&out, &m, &files).inspect_err(|_| {
        let _ = std::fs::remove_file(&out);
    })?;
    Ok(Packed {
        path: out,
        manifest: m,
    })
}

fn as_set(v: &[String]) -> BTreeSet<&str> {
    v.iter().map(String::as_str).collect()
}

fn output_path(opts: &PackOptions, m: &Manifest) -> Result<PathBuf> {
    let file_name = if opts.targets.len() == 1 {
        format!("{}-{}-{}.juizapp", m.name, m.version, opts.targets[0])
    } else {
        format!("{}-{}.juizapp", m.name, m.version)
    };
    Ok(match &opts.output {
        None => PathBuf::from(file_name),
        Some(o) if o.is_dir() => o.join(file_name),
        Some(o) => o.clone(),
    })
}

/// conf の plugins に書いたプラグイン 1 つ
#[derive(Debug)]
struct PluginRef {
    name: String,
    language: String,
    path: String,
    native: bool,
    /// Rust ABI で読まれる理由(EC・ブローカー、または conf の `abi: rust`)。無ければ印で決める(§6.3 の 6)
    rust_abi_reason: Option<&'static str>,
}

/// entry の conf の plugins を並べる(§6.3 の 3)
fn list_plugins(conf: &Value) -> Result<Vec<PluginRef>> {
    let mut out = Vec::new();
    let Some(plugins) = conf.get("plugins").filter(|v| !v.is_null()) else {
        return Ok(out);
    };
    let plugins = plugins
        .as_object()
        .ok_or_else(|| anyhow!("'plugins' in the entry conf must be a map"))?;
    for (kind, entries) in plugins {
        let Some(entries) = entries.as_object() else {
            if entries.is_null() {
                continue;
            }
            bail!("'plugins.{kind}' in the entry conf must be a map");
        };
        let rust_only = matches!(kind.as_str(), "broker_factories" | "ec_factories");
        for (name, v) in entries {
            let language = if rust_only {
                "rust".to_owned()
            } else {
                v.get("language")
                    .and_then(Value::as_str)
                    .unwrap_or("rust")
                    .to_owned()
            };
            push_plugin(&mut out, kind, name, v, &language)?;
            if kind == "container_factories" {
                if let Some(procs) = v.get("processes").and_then(Value::as_object) {
                    for (cp_name, cp) in procs {
                        push_plugin(&mut out, kind, cp_name, cp, &language)?;
                    }
                }
            }
        }
    }
    Ok(out)
}

fn push_plugin(
    out: &mut Vec<PluginRef>,
    kind: &str,
    name: &str,
    v: &Value,
    language: &str,
) -> Result<()> {
    let path = v
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("plugins.{kind}.{name} in the entry conf has no 'path'"))?;
    if !LANGUAGES.contains(&language) {
        bail!("unknown language '{language}' of plugins.{kind}.{name} (one of {LANGUAGES:?})");
    }
    let rust_abi_reason = if language != "rust" {
        None
    } else if matches!(kind, "broker_factories" | "ec_factories") {
        Some("EC / broker plugin")
    } else {
        match v.get("abi") {
            None | Some(Value::Null) => None,
            Some(Value::String(a)) if a == "c" => None,
            Some(Value::String(a)) if a == "rust" => Some("'abi: rust' in the conf"),
            Some(a) => bail!(
                "'abi' of plugins.{kind}.{name} in the entry conf must be 'c' or 'rust', but {a}"
            ),
        }
    };
    out.push(PluginRef {
        name: name.to_owned(),
        language: language.to_owned(),
        path: path.to_owned(),
        native: language != "python",
        rust_abi_reason,
    });
    Ok(())
}

/// Rust のプラグインのファイルから、juiz_sdk が埋め込むビルド情報 `juiz_sdk=…;rustc=…` を探す。
/// 読み込まずにバイト列から探すので、別のターゲットのバイナリでも読める
/// OS の基本のライブラリ(受け取る側にもあるとみなすもの)。scripts/bundle-plugin-deps.sh と同じ
fn is_system_library(name: &str) -> bool {
    if name.starts_with("/usr/lib/") || name.starts_with("/System/") {
        return true;
    }
    const LINUX: &[&str] = &[
        "linux-vdso.so",
        "linux-gate.so",
        "ld-linux",
        "ld64.so",
        "libc.so",
        "libm.so",
        "libdl.so",
        "libpthread.so",
        "librt.so",
        "libutil.so",
        "libresolv.so",
        "libgcc_s.so",
        "libstdc++.so",
    ];
    LINUX.iter().any(|p| name.starts_with(p))
}

/// プラグインが使う共有ライブラリのうち、プラグインの隣にも OS の基本のライブラリにも無いものを並べる
/// (Linux は ldd、macOS は otool -L。コマンドが無いか失敗したら何も言わない)
fn dependency_warnings(plugin: &Path) -> Vec<String> {
    let hint =
        "Put it next to the plugin (scripts/bundle-plugin-deps.sh, or juiz_sdk_bundle() in CMake).";
    let Some(dir) = plugin.parent().and_then(|d| std::fs::canonicalize(d).ok()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        let Some(text) = command_output("otool", &["-L"], plugin) else {
            return out;
        };
        for line in text.lines().skip(1) {
            let Some(dep) = line.split_whitespace().next() else {
                continue;
            };
            if is_system_library(dep) {
                continue;
            }
            let base = dep.rsplit('/').next().unwrap_or(dep);
            if plugin.file_name().is_some_and(|f| f == base) {
                continue; // 自分の install name
            }
            if dep.starts_with('@') {
                if !dir.join(base).is_file() {
                    out.push(format!("needs {dep}, but {base} is not next to it. {hint}"));
                }
            } else {
                out.push(format!("needs {dep}, which is outside the package. {hint}"));
            }
        }
    } else if cfg!(target_os = "linux") {
        let Some(text) = command_output("ldd", &[], plugin) else {
            return out;
        };
        for line in text.lines() {
            let line = line.trim();
            let Some((name, rest)) = line.split_once(" => ") else {
                continue;
            };
            if is_system_library(name) {
                continue;
            }
            if rest.starts_with("not found") {
                out.push(format!("needs {name}, which is not found. {hint}"));
                continue;
            }
            let path = Path::new(rest.split(" (").next().unwrap_or(rest));
            let found_dir = path.parent().and_then(|d| std::fs::canonicalize(d).ok());
            if found_dir.as_deref() != Some(dir.as_path()) {
                out.push(format!(
                    "needs {name}, which is found at {} outside the package. {hint}",
                    path.display()
                ));
            }
        }
    }
    out
}

fn command_output(cmd: &str, args: &[&str], file: &Path) -> Option<String> {
    let o = std::process::Command::new(cmd)
        .args(args)
        .arg(file)
        .output()
        .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// pack に入れる Rust のプラグインのファイル 1 つ
#[derive(Debug, Clone)]
struct RustFile {
    rel: String,
    /// Rust ABI で読まれる理由(印より優先する)
    forced_rust_abi: Option<&'static str>,
}

/// Rust のプラグインを C ABI と Rust ABI に分けた結果(§6.3 の 6)
#[derive(Debug)]
struct RustPlugins {
    juiz_sdk: String,
    /// C ABI のプラグインの ABI の版(C ABI のものが無ければ None)
    abi: Option<u32>,
    /// Rust ABI のプラグインの rustc と、そのファイル(無ければ None)
    rust_abi: Option<(String, Vec<String>)>,
}

impl RustPlugins {
    fn rust_abi_files(&self) -> Vec<String> {
        self.rust_abi
            .as_ref()
            .map(|(_, f)| f.clone())
            .unwrap_or_default()
    }
}

/// C ABI が入った juiz の版(rust-plugin-abi.md、#291)。これより前の juiz は C ABI のプラグインを読めない
const RUST_CABI_SINCE: &str = "0.0.30";

/// `juiz` の範囲が C ABI を読めない juiz を含むときの警告(§6.3 の 6)
fn old_juiz_warning(written: &str, req: &semver::VersionReq) -> Option<String> {
    let since = semver::Version::parse(RUST_CABI_SINCE).expect("semver");
    let before = semver::Version::new(since.major, since.minor, since.patch - 1);
    req.matches(&before).then(|| {
        format!(
            "'juiz: {written}' in juizapp.yaml includes juiz older than {RUST_CABI_SINCE}, which cannot load the Rust plugins of the package (they use the plugin C ABI). Consider 'juiz: \">={RUST_CABI_SINCE}\"'."
        )
    })
}

/// 各 Rust のプラグインのファイルを読み、ビルド情報と ABI の印から C ABI / Rust ABI に分ける(§6.3 の 6)
fn classify_rust_plugins(
    rust_files: &[RustFile],
    abs: impl Fn(&str) -> PathBuf,
) -> Result<RustPlugins> {
    let mut sdk: Option<(String, String)> = None; // (版, 最初のファイル)
    let mut abi: Option<(u32, String)> = None;
    let mut rustc: Option<(String, String)> = None;
    let mut rust_abi_files: Vec<String> = Vec::new();
    for f in rust_files {
        let rel = &f.rel;
        let path = abs(rel);
        let bytes =
            std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let info = build_info_in(&bytes, &path)
            .with_context(|| format!("cannot read the build information of {rel}"))?;
        let this_sdk = info.juiz_sdk_str().to_owned();
        match &sdk {
            None => sdk = Some((this_sdk, rel.clone())),
            Some((s, first)) if *s != this_sdk => bail!(
                "Rust plugins in the package must be built with the same juiz_sdk, but {first} is built with juiz_sdk {s} and {rel} is built with juiz_sdk {this_sdk}"
            ),
            _ => {}
        }
        let marker = abi_marker_in(&bytes, &path)?;
        match (f.forced_rust_abi, marker) {
            (None, Some(v)) => match &abi {
                None => abi = Some((v, rel.clone())),
                Some((a, first)) if *a != v => bail!(
                    "Rust plugins in the package must use the same plugin ABI, but {first} uses ABI {a} and {rel} uses ABI {v}"
                ),
                _ => {}
            },
            (forced, _) => {
                let this_rustc = info.rustc_str().to_owned();
                match &rustc {
                    None => rustc = Some((this_rustc, rel.clone())),
                    Some((r, first)) if *r != this_rustc => {
                        let why = forced.unwrap_or("no plugin C ABI entry");
                        bail!(
                            "Rust plugins loaded through the Rust ABI must be built with the same rustc, but {first} is built with {r} and {rel} ({why}) is built with {this_rustc}"
                        )
                    }
                    _ => {}
                }
                if !rust_abi_files.contains(rel) {
                    rust_abi_files.push(rel.clone());
                }
            }
        }
    }
    let (juiz_sdk, _) = sdk.expect("rust_files is not empty");
    Ok(RustPlugins {
        juiz_sdk,
        abi: abi.map(|(a, _)| a),
        rust_abi: rustc.map(|(r, _)| (r, rust_abi_files)),
    })
}

/// ファイルのバイト列から ABI の印 `juiz_rust_abi=<版>;` を探す(rust-plugin-abi.md §9.3)
fn abi_marker_in(bytes: &[u8], path: &Path) -> Result<Option<u32>> {
    let needle = b"juiz_rust_abi=";
    let mut found: Option<u32> = None;
    let mut i = 0;
    while let Some(pos) = find(&bytes[i..], needle) {
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
            Some(f) if f != v => bail!(
                "{} has more than one plugin ABI marker (juiz_rust_abi={f} and juiz_rust_abi={v})",
                path.display()
            ),
            _ => {}
        }
    }
    Ok(found)
}

#[cfg(test)]
fn read_build_info(path: &Path) -> Result<BuildInfo> {
    let bytes = std::fs::read(path)?;
    build_info_in(&bytes, path)
}

fn build_info_in(bytes: &[u8], path: &Path) -> Result<BuildInfo> {
    let needle = b"juiz_sdk=";
    let mut found: Option<BuildInfo> = None;
    let mut i = 0;
    while let Some(pos) = find(&bytes[i..], needle) {
        let start = i + pos;
        let end = bytes[start..]
            .iter()
            .take(512)
            .position(|b| *b == 0)
            .map(|n| start + n);
        i = start + needle.len();
        let Some(end) = end else { continue };
        let Ok(s) = std::str::from_utf8(&bytes[start..end]) else {
            continue;
        };
        if !s.contains(";rustc=") {
            continue;
        }
        let info = BuildInfo::parse(s);
        // 書式の断片 ("juiz_sdk=" と ";rustc=" が並んだだけの文字列など) はビルド情報ではない (#333)
        let sdk_is_version = info.juiz_sdk.as_deref().is_some_and(|v| semver::Version::parse(v).is_ok());
        let rustc_is_version = info.rustc.as_deref().is_some_and(|v| v.starts_with("rustc "));
        if !sdk_is_version || !rustc_is_version {
            continue;
        }
        match &found {
            None => found = Some(info),
            Some(f) if *f != info => bail!(
                "{} has more than one juiz build information ({f} and {info})",
                path.display()
            ),
            _ => {}
        }
    }
    found.ok_or_else(|| {
        anyhow!(
            "{} has no juiz build information. Is it a Rust plugin built with juiz_sdk?",
            path.display()
        )
    })
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn build_excludes(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(
            GlobBuilder::new(p)
                .literal_separator(true)
                .build()
                .map_err(|e| anyhow!("invalid glob '{p}' in 'exclude': {e}"))?,
        );
    }
    Ok(b.build()?)
}

/// `<dir>` の下から入れるファイルを集める(§6.2)。キーはパッケージの中のパス
fn collect_files(
    dir: &Path,
    exclude: &[String],
    targets: &[String],
) -> Result<BTreeMap<String, Collected>> {
    let excludes = build_excludes(exclude)?;
    let mut out = BTreeMap::new();
    walk(dir, "", &excludes, targets, &mut out)?;
    if let Some(dup) = find_case_collision(out.keys().map(String::as_str)) {
        bail!("'{dup}' collides with another file whose name differs only in case");
    }
    Ok(out)
}

fn walk(
    abs: &Path,
    rel: &str,
    excludes: &GlobSet,
    targets: &[String],
    out: &mut BTreeMap<String, Collected>,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(abs)
        .with_context(|| format!("cannot read {}", abs.display()))?
        .collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| anyhow!("file name {:?} in {} is not UTF-8", name, abs.display()))?;
        if name.starts_with('.') {
            continue;
        }
        let child_rel = if rel.is_empty() {
            name.to_owned()
        } else {
            format!("{rel}/{name}")
        };
        let meta = std::fs::symlink_metadata(e.path())?;
        if meta.file_type().is_symlink() {
            bail!("symbolic links can not be put in a package: {child_rel}");
        }
        if meta.is_dir() {
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            if rel == "plugins" && !targets.is_empty() && !targets.iter().any(|t| t == name) {
                continue;
            }
            if rel.is_empty() && name == "bin" {
                bail!("'bin/' is reserved and can not be put in a package (format 1)");
            }
            walk(&e.path(), &child_rel, excludes, targets, out)?;
            continue;
        }
        if rel.is_empty() && name == MANIFEST_FILE {
            continue;
        }
        if name.ends_with(".pyc") || name.ends_with(".juizapp") || excludes.is_match(&child_rel) {
            continue;
        }
        if is_reserved_path(&child_rel) {
            bail!("'{child_rel}' is reserved and can not be put in a package (format 1)");
        }
        check_entry_name(&child_rel)
            .map_err(|r| anyhow!("file name '{child_rel}' can not be put in a package ({r})"))?;
        out.insert(
            child_rel,
            Collected {
                abs: e.path(),
                executable: is_executable(&meta),
            },
        );
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
}

/// §6.4 のとおり zip を書く
fn write_zip(out: &Path, m: &Manifest, files: &BTreeMap<String, Collected>) -> Result<()> {
    let f = File::create(out).with_context(|| format!("cannot create {}", out.display()))?;
    let mut zip = ZipWriter::new(f);
    let base = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    zip.start_file(MANIFEST_FILE, base.unix_permissions(0o644))?;
    zip.write_all(m.to_yaml().as_bytes())?;
    for (rel, c) in files {
        let size = std::fs::metadata(&c.abs)?.len();
        let mode = if c.executable { 0o755 } else { 0o644 };
        zip.start_file(
            rel.as_str(),
            base.unix_permissions(mode)
                .large_file(size >= u32::MAX as u64),
        )?;
        let mut src = File::open(&c.abs)?;
        std::io::copy(&mut src, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_libraries() {
        use super::is_system_library;
        assert!(is_system_library("libc.so.6"));
        assert!(is_system_library("libstdc++.so.6"));
        assert!(is_system_library("/usr/lib/libSystem.B.dylib"));
        assert!(!is_system_library("libjuiz_sdk.so"));
        assert!(!is_system_library("libgmp.so.10"));
        assert!(!is_system_library("@rpath/libjuiz_sdk.dylib"));
    }

    use super::*;

    #[test]
    fn reads_build_info_from_bytes() {
        let dir = std::env::temp_dir().join(format!("juiz_pack_bi_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("libx.so");
        std::fs::write(
            &p,
            b"\x7fELF..juiz_sdk=0.0.28\0..juiz_sdk=0.0.28;rustc=rustc 1.91.0 (x)\0tail",
        )
        .unwrap();
        let info = read_build_info(&p).unwrap();
        assert_eq!(info.juiz_sdk.as_deref(), Some("0.0.28"));
        assert_eq!(info.rustc.as_deref(), Some("rustc 1.91.0 (x)"));

        std::fs::write(&p, b"juiz_sdk=0.0.1;rustc=rustc a\0juiz_sdk=0.0.2;rustc=rustc a\0").unwrap();
        assert!(read_build_info(&p).is_err());
        // 書式の断片 (format! の "juiz_sdk=" と ";rustc=" が並んだもの) は読み飛ばす (#333)
        std::fs::write(
            &p,
            b"juiz_sdk=;rustc=native_abi_tagcontainer_tag\0juiz_sdk=0.0.30;rustc=rustc 1.88.0 (x)\0",
        )
        .unwrap();
        let info = read_build_info(&p).unwrap();
        assert_eq!(info.juiz_sdk.as_deref(), Some("0.0.30"));
        assert_eq!(info.rustc.as_deref(), Some("rustc 1.88.0 (x)"));
        std::fs::write(&p, b"nothing").unwrap();
        assert!(read_build_info(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn rust_file(dir: &Path, name: &str, bytes: &[u8], forced: Option<&'static str>) -> RustFile {
        std::fs::write(dir.join(name), bytes).unwrap();
        RustFile {
            rel: name.to_owned(),
            forced_rust_abi: forced,
        }
    }

    #[test]
    fn classifies_rust_plugins() {
        let dir = std::env::temp_dir().join(format!("juiz_pack_cls_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let abs = |rel: &str| dir.join(rel);
        let cabi_a = rust_file(
            &dir,
            "a",
            b"juiz_sdk=0.0.30;rustc=rustc A\0juiz_rust_abi=1;\0",
            None,
        );
        let cabi_b = rust_file(
            &dir,
            "b",
            b"juiz_sdk=0.0.30;rustc=rustc B\0juiz_rust_abi=1;\0",
            None,
        );
        // C ABI だけなら rustc が違ってもよい
        let r = classify_rust_plugins(&[cabi_a.clone(), cabi_b.clone()], abs).unwrap();
        assert_eq!(r.juiz_sdk, "0.0.30");
        assert_eq!(r.abi, Some(1));
        assert!(r.rust_abi.is_none());

        // 印の無い古いプラグインと EC は Rust ABI
        let old = rust_file(&dir, "old", b"juiz_sdk=0.0.30;rustc=rustc A\0", None);
        let ec = rust_file(
            &dir,
            "ec",
            b"juiz_sdk=0.0.30;rustc=rustc A\0juiz_rust_abi=1;\0",
            Some("EC / broker plugin"),
        );
        let r = classify_rust_plugins(&[cabi_b.clone(), old.clone(), ec.clone()], abs).unwrap();
        assert_eq!(r.abi, Some(1));
        let (rustc, files) = r.rust_abi.unwrap();
        assert_eq!(rustc, "rustc A");
        assert_eq!(files, vec!["old".to_owned(), "ec".to_owned()]);

        // Rust ABI 同士の rustc 違いはエラー
        let ec_b = rust_file(
            &dir,
            "ec_b",
            b"juiz_sdk=0.0.30;rustc=rustc B\0",
            Some("EC / broker plugin"),
        );
        let e = classify_rust_plugins(&[old.clone(), ec_b], abs)
            .unwrap_err()
            .to_string();
        assert!(e.contains("same rustc"), "{e}");
        // juiz_sdk の違いはどちらでもエラー
        let other_sdk = rust_file(
            &dir,
            "c",
            b"juiz_sdk=0.0.31;rustc=rustc A\0juiz_rust_abi=1;\0",
            None,
        );
        let e = classify_rust_plugins(&[cabi_a.clone(), other_sdk], abs)
            .unwrap_err()
            .to_string();
        assert!(e.contains("same juiz_sdk"), "{e}");
        // ABI の版の違いはエラー
        let abi2 = rust_file(
            &dir,
            "d",
            b"juiz_sdk=0.0.30;rustc=rustc A\0juiz_rust_abi=2;\0",
            None,
        );
        let e = classify_rust_plugins(&[cabi_a, abi2], abs)
            .unwrap_err()
            .to_string();
        assert!(e.contains("same plugin ABI"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_abi_marker() {
        let p = Path::new("x");
        assert_eq!(
            abi_marker_in(b"..juiz_rust_abi=1;\0..", p).unwrap(),
            Some(1)
        );
        assert_eq!(
            abi_marker_in(b"juiz_rust_abi=1;juiz_rust_abi=1;", p).unwrap(),
            Some(1)
        );
        // 版の形でないものは印とみなさない
        assert_eq!(
            abi_marker_in(b"juiz_rust_abi=x; juiz_rust_abi=12", p).unwrap(),
            None
        );
        assert!(abi_marker_in(b"juiz_rust_abi=1; juiz_rust_abi=2;", p).is_err());
    }

    #[test]
    fn warns_old_juiz_range() {
        let w = |j: &str| old_juiz_warning(j, &semver::VersionReq::parse(j).unwrap());
        assert!(w(">=0.0.1").is_some());
        assert!(w(">=0.0.30").is_none());
        assert!(w("^0.0.30").is_none());
    }

    #[test]
    fn rust_abi_reason_from_conf() {
        let conf = yaml::parse(
            r#"
plugins:
  ec_factories: { timer_ec: { path: "plugins/{target}" } }
  process_factories:
    a: { path: "plugins/{target}" }
    b: { path: "plugins/{target}", abi: rust }
    c: { path: "plugins/{target}", abi: c }
"#,
        )
        .unwrap();
        let ps = list_plugins(&conf).unwrap();
        let get = |n: &str| ps.iter().find(|p| p.name == n).unwrap().rust_abi_reason;
        assert!(get("timer_ec").is_some());
        assert!(get("a").is_none());
        assert!(get("b").is_some());
        assert!(get("c").is_none());
        let bad =
            yaml::parse("plugins: { process_factories: { a: { path: p, abi: cpp } } }").unwrap();
        assert!(list_plugins(&bad).is_err());
    }

    #[test]
    fn lists_plugins_of_every_kind() {
        let conf = yaml::parse(
            r#"
plugins:
  broker_factories: { qmp_broker: { path: "plugins/{target}" } }
  process_factories:
    talker: { path: "plugins/{target}" }
    listener: { language: python, path: python }
  container_factories:
    cont: { language: c++, path: "plugins/{target}", processes: { cont_get: { path: "plugins/{target}" } } }
  ec_factories: { timer_ec: { path: "plugins/{target}" } }
"#,
        )
        .unwrap();
        let ps = list_plugins(&conf).unwrap();
        let get = |n: &str| ps.iter().find(|p| p.name == n).unwrap();
        assert_eq!(get("qmp_broker").language, "rust");
        assert_eq!(get("listener").language, "python");
        assert!(!get("listener").native);
        assert_eq!(get("cont_get").language, "c++");
        assert_eq!(get("timer_ec").language, "rust");
        assert_eq!(ps.len(), 6);
    }
}
