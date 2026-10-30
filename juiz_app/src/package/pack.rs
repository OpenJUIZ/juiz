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
    let mut rust_files: Vec<String> = Vec::new();
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
                rust_files.push(rel);
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

    // 6. Rust のプラグインのビルド情報
    m.rust_juiz_sdk = None;
    m.rust_rustc = None;
    if !rust_files.is_empty() {
        let mut found: Option<(String, BuildInfo)> = None;
        for rel in &rust_files {
            let info = read_build_info(&files[rel].abs)
                .with_context(|| format!("cannot read the build information of {rel}"))?;
            match &found {
                None => found = Some((rel.clone(), info)),
                Some((first, first_info)) if *first_info != info => bail!(
                    "Rust plugins in the package must be built with the same juiz_sdk and rustc, but {first} is built with {first_info} and {rel} is built with {info}"
                ),
                _ => {}
            }
        }
        let (_, info) = found.unwrap();
        let sdk = info.juiz_sdk_str().to_owned();
        let required = format!("={sdk}");
        match &m.juiz {
            None => m.juiz = Some(required),
            Some(j) => {
                let ok =
                    exact_version(&parse_req("'juiz'", j)?).is_some_and(|v| v.to_string() == sdk);
                if !ok {
                    bail!("the package has Rust plugins built with juiz_sdk {sdk}, so 'juiz' in juizapp.yaml must be \"={sdk}\", but \"{j}\"");
                }
            }
        }
        m.rust_juiz_sdk = Some(sdk);
        m.rust_rustc = Some(info.rustc_str().to_owned());
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
    out.push(PluginRef {
        name: name.to_owned(),
        language: language.to_owned(),
        path: path.to_owned(),
        native: language != "python",
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

pub(crate) fn read_build_info(path: &Path) -> Result<BuildInfo> {
    let bytes = std::fs::read(path)?;
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

        std::fs::write(&p, b"juiz_sdk=1;rustc=a\0juiz_sdk=2;rustc=a\0").unwrap();
        assert!(read_build_info(&p).is_err());
        std::fs::write(&p, b"nothing").unwrap();
        assert!(read_build_info(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
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
