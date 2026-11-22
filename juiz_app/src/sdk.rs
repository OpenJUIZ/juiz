//! libjuiz_sdk の取得と場所の表示 (issue #548)
//!
//! ```text
//! juiz sdk install [--version <版>] [--target <ターゲット>] [--dir <dir>] [--force]
//! juiz sdk path [--version <版>] [--target <ターゲット>] [--dir <dir>] [--cmake-dir]
//! ```
//!
//! `--target` (issue #577) はホストと別の OS・CPU 向けのアーカイブを入れる。Zig のプラグインを 1 台から
//! 複数のターゲット向けにクロスビルドするときに使う。既定の置き場所は `<JUIZ_SDK_HOME>/<版>-<ターゲット>`
//! (ホストのターゲットを書いたときは、省いたときと同じ `<JUIZ_SDK_HOME>/<版>`)。
//!
//! C++ / Haskell のプラグインは、juiz と同じ版の libjuiz_sdk とヘッダ (GitHub Releases の
//! `juiz_sdk-<版>-<ターゲット>.tar.gz`、Windows は `.zip`、#145・#273) を使ってビルドする。
//! `juiz sdk install` はそれを取得し、`.sha256` と照合してから展開する。
//!
//! 置き場所・環境変数は scripts/install-juiz-sdk.sh / .ps1 と同じ:
//! - 既定の置き場所は `<JUIZ_SDK_HOME>/<版>`、`JUIZ_SDK_HOME` の既定は `~/.juiz/sdk`
//!   (Windows は `%USERPROFILE%\.juiz\sdk`)。juiz pkg の C++ 雛形 (CMakeLists.txt) の既定とそろう。
//! - 取得元は `<JUIZ_SDK_BASE_URL>/v<版>/<アーカイブ>`、`JUIZ_SDK_BASE_URL` の既定は
//!   `https://github.com/OpenJUIZ/juiz/releases/download`。`file://<ディレクトリ>` も使える
//!   (手元に置いたアーカイブ・テスト用)。

use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use juiz_core::anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};

/// 実行中の juiz の版 (= 既定で取得する libjuiz_sdk の版)
const JUIZ_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_BASE_URL: &str = "https://github.com/OpenJUIZ/juiz/releases/download";

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum SdkSubCommands {
    /// Download libjuiz_sdk and its headers for C++ / Haskell plugins from GitHub Releases
    Install {
        /// Version to install (default: the version of this juiz)
        #[arg(long = "version")]
        version: Option<String>,

        /// Target triple of the archive, for cross builds (default: this machine; one of
        /// x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin, x86_64-pc-windows-msvc)
        #[arg(long = "target")]
        target: Option<String>,

        /// Directory to install into (default: ~/.juiz/sdk/<version>, or ~/.juiz/sdk/<version>-<target> with --target
        /// for another machine; JUIZ_SDK_HOME changes ~/.juiz/sdk)
        #[arg(long = "dir")]
        dir: Option<String>,

        /// Download and install again even if it is already installed
        #[arg(long = "force")]
        force: bool,
    },

    /// Print the directory of the installed libjuiz_sdk (for -DCMAKE_PREFIX_PATH etc.)
    Path {
        /// Version (default: the version of this juiz)
        #[arg(long = "version")]
        version: Option<String>,

        /// Target triple (same as 'juiz sdk install --target')
        #[arg(long = "target")]
        target: Option<String>,

        /// Directory where it was installed (same as 'juiz sdk install --dir')
        #[arg(long = "dir")]
        dir: Option<String>,

        /// Print the directory of juiz_sdkConfig.cmake (for -Djuiz_sdk_DIR) instead
        #[arg(long = "cmake-dir")]
        cmake_dir: bool,
    },
}

/// `juiz sdk ...` を実行して終了コードを返す
pub(crate) fn run_sdk_command(subcommand: &SdkSubCommands) -> i32 {
    let r: Result<()> = match subcommand {
        SdkSubCommands::Install {
            version,
            target,
            dir,
            force,
        } => (|| -> Result<()> {
            let version = resolve_version(version.as_deref())?;
            let target = resolve_target(target.as_deref())?;
            let dest = resolve_dir(&version, target, dir.as_deref())?;
            let base_url = std::env::var("JUIZ_SDK_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
            let outcome = install(&InstallOptions {
                version: &version,
                target,
                base_url: &base_url,
                dest: &dest,
                force: *force,
                log: &|m| println!("{m}"),
            })?;
            match outcome {
                Outcome::Installed => println!("installed libjuiz_sdk {version} to {}", dest.display()),
                Outcome::AlreadyInstalled => println!(
                    "libjuiz_sdk {version} is already installed in {} (use --force to install it again)",
                    dest.display()
                ),
            }
            println!();
            print!("{}", usage_hint(&version, &dest));
            Ok(())
        })(),
        SdkSubCommands::Path {
            version,
            target,
            dir,
            cmake_dir,
        } => (|| -> Result<()> {
            let version = resolve_version(version.as_deref())?;
            let target = resolve_target(target.as_deref())?;
            let dest = resolve_dir(&version, target, dir.as_deref())?;
            println!("{}", installed_path(&version, target, &dest, *cmake_dir)?.display());
            Ok(())
        })(),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

/// 版の指定を確かめる。先頭の `v` は省く。省けば実行中の juiz の版
fn resolve_version(version: Option<&str>) -> Result<String> {
    let v = version.unwrap_or(JUIZ_VERSION);
    let v = v.strip_prefix('v').unwrap_or(v);
    semver::Version::parse(v)
        .map_err(|e| anyhow!("'{v}' is not a version like {JUIZ_VERSION}: {e}"))?;
    Ok(v.to_owned())
}

/// 配布しているアーカイブのターゲット (sdk-release.yml の 5 種)
const RELEASED_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

/// `--target` の指定を確かめる。省けばこの juiz が動いている OS・CPU
fn resolve_target(target: Option<&str>) -> Result<&'static str> {
    match target {
        Some(t) => RELEASED_TARGETS
            .iter()
            .find(|r| **r == t)
            .copied()
            .ok_or_else(|| {
                anyhow!(
                    "libjuiz_sdk is not distributed for the target '{t}'. Use one of: {}",
                    RELEASED_TARGETS.join(", ")
                )
            }),
        None => host_target().ok_or_else(|| {
            anyhow!(
                "libjuiz_sdk is not distributed for {} {}. Build it with 'cargo build -p juiz_sdk' in the juiz repository.",
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        }),
    }
}

/// SDK を置くディレクトリ。`--dir` が無ければ `<JUIZ_SDK_HOME>/<版>`、
/// ホストと別のターゲットなら `<JUIZ_SDK_HOME>/<版>-<ターゲット>`
fn resolve_dir(version: &str, target: &str, dir: Option<&str>) -> Result<PathBuf> {
    if let Some(d) = dir {
        return Ok(PathBuf::from(d));
    }
    Ok(sdk_home()?.join(default_dir_name(version, target, host_target())))
}

fn default_dir_name(version: &str, target: &str, host: Option<&str>) -> String {
    if host == Some(target) {
        version.to_owned()
    } else {
        format!("{version}-{target}")
    }
}

/// `JUIZ_SDK_HOME`、無ければ `~/.juiz/sdk` (Windows は `%USERPROFILE%\.juiz\sdk`)
fn sdk_home() -> Result<PathBuf> {
    if let Some(d) = std::env::var_os("JUIZ_SDK_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    let home = home::home_dir()
        .ok_or_else(|| anyhow!("cannot find the home directory. Set JUIZ_SDK_HOME."))?;
    Ok(home.join(".juiz").join("sdk"))
}

/// この juiz が動いている OS・CPU 向けのアーカイブのターゲット名。配布していなければ None
///
/// Releases に置いているのは sdk-release.yml の 5 種。Windows は MSVC の DLL 1 種だけで、
/// 境界は C ABI なので MinGW などの juiz・プラグインからも使える (#273)。
fn host_target() -> Option<&'static str> {
    target_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn target_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

/// アーカイブの名前 (拡張子なし) と拡張子
fn archive_name(version: &str, target: &str) -> (String, &'static str) {
    let ext = if target.contains("-windows-") {
        "zip"
    } else {
        "tar.gz"
    };
    (format!("juiz_sdk-{version}-{target}"), ext)
}

/// 展開済みかどうかの目印。どの版のアーカイブにもある C のヘッダ
fn marker(dir: &Path) -> PathBuf {
    dir.join("cjuiz").join("include").join("juiz_sdk.h")
}

/// juiz_sdkConfig.cmake のあるディレクトリ (0.0.27 以降のアーカイブ、#162)
fn cmake_dir(dir: &Path) -> PathBuf {
    dir.join("lib").join("cmake").join("juiz_sdk")
}

/// `juiz sdk path` の出力。入っていなければ `juiz sdk install` を案内するエラー
fn installed_path(version: &str, target: &str, dir: &Path, want_cmake_dir: bool) -> Result<PathBuf> {
    if !marker(dir).is_file() {
        bail!(
            "libjuiz_sdk {version} is not installed in {}. Run 'juiz sdk install{}{}'.",
            dir.display(),
            if version == JUIZ_VERSION {
                String::new()
            } else {
                format!(" --version {version}")
            },
            if host_target() == Some(target) {
                String::new()
            } else {
                format!(" --target {target}")
            }
        );
    }
    if want_cmake_dir {
        let c = cmake_dir(dir);
        if !c.join("juiz_sdkConfig.cmake").is_file() {
            bail!(
                "{} has no juiz_sdkConfig.cmake (libjuiz_sdk archives have it from 0.0.27)",
                dir.display()
            );
        }
        return Ok(c);
    }
    Ok(dir.to_path_buf())
}

pub(crate) struct InstallOptions<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub base_url: &'a str,
    pub dest: &'a Path,
    pub force: bool,
    pub log: &'a dyn Fn(&str),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Installed,
    AlreadyInstalled,
}

/// アーカイブを取得し、SHA-256 を照合して `dest` に展開する
///
/// 展開は `dest` と同じ親ディレクトリの作業用ディレクトリで行い、最後に名前を変えて置く。
/// 途中で失敗しても、前から入っていたものは消さない。
pub(crate) fn install(o: &InstallOptions) -> Result<Outcome> {
    if !o.force && marker(o.dest).is_file() {
        return Ok(Outcome::AlreadyInstalled);
    }
    let (name, ext) = archive_name(o.version, o.target);
    let url = format!(
        "{}/v{}/{name}.{ext}",
        o.base_url.trim_end_matches('/'),
        o.version
    );
    (o.log)(&format!("downloading {url}"));
    let archive = fetch(&url).map_err(|e| {
        e.context(format!(
            "cannot download libjuiz_sdk {}. Check that the version is published at https://github.com/OpenJUIZ/juiz/releases",
            o.version
        ))
    })?;
    let sums = fetch(&format!("{url}.sha256"))?;
    verify_sha256(&archive, &sums).with_context(|| format!("{name}.{ext}"))?;

    let parent = match o.dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&parent)
        .with_context(|| format!("cannot create {}", parent.display()))?;
    let work = parent.join(format!(".{name}.tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).with_context(|| format!("cannot create {}", work.display()))?;
    let r = (|| -> Result<()> {
        extract(&archive, ext, &work)?;
        let root = work.join(&name);
        if !marker(&root).is_file() {
            bail!("the archive {name}.{ext} does not have {name}/cjuiz/include/juiz_sdk.h");
        }
        if o.dest.exists() {
            std::fs::remove_dir_all(o.dest)
                .with_context(|| format!("cannot remove {}", o.dest.display()))?;
        }
        std::fs::rename(&root, o.dest)
            .with_context(|| format!("cannot move {} to {}", root.display(), o.dest.display()))?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&work);
    r.map(|_| Outcome::Installed)
}

/// URL の中身を読む。`file://` はローカルのファイル
fn fetch(url: &str) -> Result<Vec<u8>> {
    if let Some(p) = url.strip_prefix("file://") {
        // file:///C:/... の先頭の / を外す
        let p = if cfg!(windows) && p.starts_with('/') && p.as_bytes().get(2) == Some(&b':') {
            &p[1..]
        } else {
            p
        };
        return std::fs::read(p).with_context(|| format!("cannot read {p}"));
    }
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("juiz/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(None)
        .build()?;
    let res = client
        .get(url)
        .send()
        .with_context(|| format!("cannot get {url}"))?;
    let status = res.status();
    if !status.is_success() {
        bail!("{url}: {status}");
    }
    let mut buf = Vec::new();
    res.take(u64::MAX)
        .read_to_end(&mut buf)
        .with_context(|| format!("cannot read {url}"))?;
    Ok(buf)
}

/// `.sha256` (`<16進> <名前>` または `<16進>` だけ) と照合する
fn verify_sha256(data: &[u8], sums: &[u8]) -> Result<()> {
    let text = String::from_utf8_lossy(sums);
    let expected = text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("the .sha256 file is empty"))?
        .to_ascii_lowercase();
    let actual: String = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if expected != actual {
        bail!("checksum mismatch (expected {expected}, got {actual})");
    }
    Ok(())
}

/// アーカイブを `dir` に展開する。`..` や絶対パスを含む項目は tar / zip のクレートが外へ書かせない
fn extract(data: &[u8], ext: &str, dir: &Path) -> Result<()> {
    match ext {
        "zip" => {
            let mut z =
                zip::ZipArchive::new(std::io::Cursor::new(data)).context("broken zip archive")?;
            z.extract(dir).context("cannot extract the zip archive")?;
        }
        _ => {
            let gz = flate2::read::GzDecoder::new(data);
            let mut t = tar::Archive::new(gz);
            t.set_preserve_permissions(true);
            t.unpack(dir).context("cannot extract the tar.gz archive")?;
        }
    }
    Ok(())
}

/// 入れた後の案内 (scripts/install-juiz-sdk.sh と同じ内容)
fn usage_hint(version: &str, dest: &Path) -> String {
    let d = dest.display().to_string();
    let cmake_d = d.replace('\\', "/");
    if cfg!(windows) {
        format!(
            "CMake: configure with -DCMAKE_PREFIX_PATH={cmake_d} and write
  find_package(juiz_sdk {version} EXACT CONFIG REQUIRED)
  target_link_libraries(<plugin> PRIVATE juiz_sdk::juiz_sdk)
  juiz_sdk_copy_runtime(<plugin>)   # copies juiz_sdk.dll next to the plugin's DLL
  ('juiz sdk path' prints this directory)

Other build systems: add include directories {d}\\cjuiz\\include and {d}\\cppjuiz\\include,
  link {d}\\lib\\juiz_sdk.dll.lib, and put {d}\\bin\\juiz_sdk.dll next to the plugin's DLL
  (juiz looks for the DLLs a plugin depends on in the plugin's directory, and in PATH).
"
        )
    } else {
        format!(
            "Haskell (hsjuiz): add this to your cabal.project
  package hsjuiz
    extra-lib-dirs: {d}/lib
    ghc-options: -optl-Wl,-rpath,{d}/lib

C++ (CMake): configure with -DCMAKE_PREFIX_PATH=\"$(juiz sdk path)\" and write
  find_package(juiz_sdk {version} EXACT CONFIG REQUIRED)
  target_link_libraries(<plugin> PRIVATE juiz_sdk::juiz_sdk)

C++ (others): add include directories {d}/cjuiz/include and {d}/cppjuiz/include,
     and link {d}/lib/libjuiz_sdk with rpath {d}/lib
     (or PKG_CONFIG_PATH={d}/lib/pkgconfig pkg-config --cflags --libs juiz_sdk)
"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("juiz_sdk_cmd_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha256_line(data: &[u8], file: &str) -> String {
        let h: String = Sha256::digest(data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("{h}  {file}\n")
    }

    /// Releases と同じ並びのアーカイブを `<base>/v<版>/` に作る
    fn make_release(base: &Path, version: &str, target: &str, header: &str) {
        let (name, ext) = archive_name(version, target);
        let files: Vec<(String, &[u8])> = vec![
            (
                format!("{name}/cjuiz/include/juiz_sdk.h"),
                header.as_bytes(),
            ),
            (
                format!("{name}/lib/cmake/juiz_sdk/juiz_sdkConfig.cmake"),
                b"# config\n",
            ),
            (format!("{name}/cppjuiz/include/juiz/juiz.h"), b"// cpp\n"),
        ];
        let data = if ext == "zip" {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (p, c) in &files {
                w.start_file(p.as_str(), zip::write::SimpleFileOptions::default())
                    .unwrap();
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
        };
        let dir = base.join(format!("v{version}"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = format!("{name}.{ext}");
        std::fs::write(dir.join(&file), &data).unwrap();
        std::fs::write(
            dir.join(format!("{file}.sha256")),
            sha256_line(&data, &file),
        )
        .unwrap();
    }

    fn opts<'a>(
        base: &'a str,
        dest: &'a Path,
        target: &'a str,
        force: bool,
        log: &'a dyn Fn(&str),
    ) -> InstallOptions<'a> {
        InstallOptions {
            version: "1.2.3",
            target,
            base_url: base,
            dest,
            force,
            log,
        }
    }

    #[test]
    fn targets_match_the_released_archives() {
        assert_eq!(
            target_for("linux", "x86_64"),
            Some("x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            target_for("linux", "aarch64"),
            Some("aarch64-unknown-linux-gnu")
        );
        assert_eq!(target_for("macos", "aarch64"), Some("aarch64-apple-darwin"));
        assert_eq!(target_for("macos", "x86_64"), Some("x86_64-apple-darwin"));
        assert_eq!(
            target_for("windows", "x86_64"),
            Some("x86_64-pc-windows-msvc")
        );
        assert_eq!(target_for("windows", "aarch64"), None);
        assert_eq!(target_for("freebsd", "x86_64"), None);
        assert_eq!(archive_name("0.2.11", "x86_64-pc-windows-msvc").1, "zip");
        assert_eq!(archive_name("0.2.11", "aarch64-apple-darwin").1, "tar.gz");
    }

    /// `--target` (issue #577): 配布している 5 種だけを受け、ホストと別なら置き場所を `<版>-<ターゲット>` にする
    #[test]
    fn other_targets() {
        for t in RELEASED_TARGETS {
            assert_eq!(resolve_target(Some(t)).unwrap(), *t);
        }
        let e = resolve_target(Some("riscv64gc-unknown-linux-gnu")).unwrap_err();
        assert!(e.to_string().contains("aarch64-apple-darwin"), "{e}");
        if let Some(h) = host_target() {
            assert_eq!(resolve_target(None).unwrap(), h);
        }
        for (os, arch) in [("linux", "x86_64"), ("linux", "aarch64"), ("macos", "aarch64"), ("macos", "x86_64"), ("windows", "x86_64")] {
            assert!(RELEASED_TARGETS.contains(&target_for(os, arch).unwrap()));
        }
        let host = Some("x86_64-unknown-linux-gnu");
        assert_eq!(default_dir_name("1.2.3", "x86_64-unknown-linux-gnu", host), "1.2.3");
        assert_eq!(
            default_dir_name("1.2.3", "aarch64-apple-darwin", host),
            "1.2.3-aarch64-apple-darwin"
        );
        assert_eq!(
            default_dir_name("1.2.3", "aarch64-apple-darwin", None),
            "1.2.3-aarch64-apple-darwin"
        );
        assert_eq!(
            resolve_dir("1.2.3", "aarch64-apple-darwin", Some("/x/y")).unwrap(),
            PathBuf::from("/x/y")
        );
    }

    #[test]
    fn version_is_checked() {
        assert_eq!(resolve_version(None).unwrap(), JUIZ_VERSION);
        assert_eq!(resolve_version(Some("v0.2.11")).unwrap(), "0.2.11");
        assert!(resolve_version(Some("../x")).is_err());
        assert!(resolve_version(Some("latest")).is_err());
    }

    #[test]
    fn install_tar_gz_and_zip() {
        for target in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
            let d = tmp(&format!("ok_{target}"));
            let base = d.join("releases");
            make_release(&base, "1.2.3", target, "// v1\n");
            let base_url = format!("file://{}", base.display());
            let dest = d.join("sdk").join("1.2.3");
            let logs = RefCell::new(Vec::new());
            let log = |m: &str| logs.borrow_mut().push(m.to_owned());

            assert_eq!(
                install(&opts(&base_url, &dest, target, false, &log)).unwrap(),
                Outcome::Installed
            );
            assert_eq!(std::fs::read_to_string(marker(&dest)).unwrap(), "// v1\n");
            assert_eq!(installed_path("1.2.3", target, &dest, false).unwrap(), dest);
            assert_eq!(
                installed_path("1.2.3", target, &dest, true).unwrap(),
                cmake_dir(&dest)
            );
            assert!(logs.borrow()[0].ends_with(&format!(
                "/v1.2.3/juiz_sdk-1.2.3-{target}.{}",
                archive_name("1.2.3", target).1
            )));
            // 作業用ディレクトリは残らない
            assert_eq!(std::fs::read_dir(d.join("sdk")).unwrap().count(), 1);

            // 入っていれば取得しない。--force なら入れ直す
            make_release(&base, "1.2.3", target, "// v2\n");
            assert_eq!(
                install(&opts(&base_url, &dest, target, false, &log)).unwrap(),
                Outcome::AlreadyInstalled
            );
            assert_eq!(std::fs::read_to_string(marker(&dest)).unwrap(), "// v1\n");
            assert_eq!(
                install(&opts(&base_url, &dest, target, true, &log)).unwrap(),
                Outcome::Installed
            );
            assert_eq!(std::fs::read_to_string(marker(&dest)).unwrap(), "// v2\n");
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn checksum_mismatch_keeps_nothing() {
        let target = "aarch64-apple-darwin";
        let d = tmp("mismatch");
        let base = d.join("releases");
        make_release(&base, "1.2.3", target, "// v1\n");
        let sum = base
            .join("v1.2.3")
            .join("juiz_sdk-1.2.3-aarch64-apple-darwin.tar.gz.sha256");
        std::fs::write(&sum, format!("{}  x\n", "0".repeat(64))).unwrap();
        let base_url = format!("file://{}", base.display());
        let dest = d.join("sdk").join("1.2.3");
        let e = install(&opts(&base_url, &dest, target, false, &|_| {})).unwrap_err();
        assert!(format!("{e:#}").contains("checksum mismatch"), "{e:#}");
        assert!(!dest.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_release_is_reported() {
        let d = tmp("missing");
        let base_url = format!("file://{}", d.display());
        let dest = d.join("sdk").join("1.2.3");
        let e = install(&opts(
            &base_url,
            &dest,
            "x86_64-unknown-linux-gnu",
            false,
            &|_| {},
        ))
        .unwrap_err();
        assert!(
            format!("{e:#}").contains("cannot download libjuiz_sdk 1.2.3"),
            "{e:#}"
        );
        let e = installed_path("1.2.3", host_target().unwrap_or("x"), &dest, false).unwrap_err();
        assert!(
            e.to_string().contains("juiz sdk install --version 1.2.3'"),
            "{e}"
        );
        // ホストと別のターゲットは --target を付けて案内する (issue #577)
        let e = installed_path("1.2.3", "no-such-target", &dest, false).unwrap_err();
        assert!(
            e.to_string()
                .contains("juiz sdk install --version 1.2.3 --target no-such-target'"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sha256_file_forms() {
        let data = b"abc";
        let line = sha256_line(data, "f");
        verify_sha256(data, line.as_bytes()).unwrap();
        verify_sha256(
            data,
            line.split_whitespace()
                .next()
                .unwrap()
                .to_uppercase()
                .as_bytes(),
        )
        .unwrap();
        assert!(verify_sha256(data, b"").is_err());
    }
}
