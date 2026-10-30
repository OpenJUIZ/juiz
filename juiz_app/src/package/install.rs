//! `juiz install` / `juiz uninstall` / `juiz packages` と、インストール済みのパッケージの検索
//! (docs/spec/app-package.md §8、#286)

use super::manifest::{Kind, Manifest};
use super::run::{check_host, check_rustc, extract, open_package, Host, Prepared, COMPLETE_MARK};
use super::{warn, MANIFEST_FILE};
use juiz_core::anyhow::{anyhow, bail, Context, Result};
use semver::{Version, VersionReq};
use std::path::{Path, PathBuf};

/// インストール済みのパッケージ 1 つ
#[derive(Debug, Clone)]
pub(crate) struct Installed {
    pub manifest: Manifest,
    /// 置き場所 `<データ>/packages/<name>/<version>/`
    pub dir: PathBuf,
}

/// `juiz install` の結果
#[derive(Debug)]
pub(crate) enum InstallOutcome {
    Installed(Installed),
    /// 同じ名前・版・中身がすでにあった
    AlreadyInstalled(Installed),
}

/// データの置き場所(§8.1)
pub(crate) fn data_dir() -> Result<PathBuf> {
    if let Some(d) = std::env::var_os("JUIZ_DATA_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    #[cfg(windows)]
    {
        if let Some(d) = std::env::var_os("LOCALAPPDATA").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(d).join("juiz").join("data"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(d) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(d).join("juiz"));
        }
    }
    let home = home::home_dir()
        .ok_or_else(|| anyhow!("cannot find the home directory. Set JUIZ_DATA_DIR."))?;
    #[cfg(windows)]
    return Ok(home.join("AppData").join("Local").join("juiz").join("data"));
    #[cfg(not(windows))]
    return Ok(home.join(".local").join("share").join("juiz"));
}

fn packages_dir(data: &Path) -> PathBuf {
    data.join("packages")
}

/// `<name>` か `<name>@<version>` を分ける
pub(crate) fn parse_spec(spec: &str) -> Result<(String, Option<Version>)> {
    match spec.split_once('@') {
        Some((name, ver)) => {
            let v = Version::parse(ver)
                .map_err(|e| anyhow!("'{ver}' in '{spec}' is not a semantic version: {e}"))?;
            Ok((name.to_owned(), Some(v)))
        }
        None => Ok((spec.to_owned(), None)),
    }
}

/// インストール済みのパッケージを名前・版の順に並べて返す。置き終わっていないもの(印が無いもの)は除く
pub(crate) fn list(data: &Path) -> Result<Vec<Installed>> {
    let root = packages_dir(data);
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }
    for name in std::fs::read_dir(&root)? {
        let name = name?;
        if !name.file_type()?.is_dir() || name.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        for ver in std::fs::read_dir(name.path())? {
            let dir = ver?.path();
            if !dir.join(COMPLETE_MARK).is_file() {
                continue;
            }
            let path = dir.join(MANIFEST_FILE);
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            match Manifest::parse_str(&text, &dir.display().to_string()) {
                Ok(manifest) => out.push(Installed { manifest, dir }),
                // 新しい juiz が入れた知らない format などは一覧から外す
                Err(e) => warn(&format!("skipping {}: {e}", dir.display())),
            }
        }
    }
    out.sort_by(|a, b| {
        (&a.manifest.name, &a.manifest.version).cmp(&(&b.manifest.name, &b.manifest.version))
    });
    Ok(out)
}

/// 名前が一致し、版が範囲に入る(範囲が無ければどれでも)うち一番新しいもの
pub(crate) fn find_newest<'a>(
    installed: &'a [Installed],
    name: &str,
    req: Option<&VersionReq>,
) -> Option<&'a Installed> {
    installed
        .iter()
        .filter(|i| i.manifest.name == name)
        .filter(|i| req.is_none_or(|r| r.matches(&i.manifest.version)))
        .max_by(|a, b| a.manifest.version.cmp(&b.manifest.version))
}

/// `juiz install <file.juizapp>`(§8.2)。§9 の段 1〜6 を行ってから置く
pub(crate) fn install(file: &Path, data: &Path, host: &Host) -> Result<InstallOutcome> {
    let label = file.display().to_string();
    if !file.is_file() {
        bail!("{label} is not found");
    }
    let (mut zip, m) = open_package(file)?;
    check_host(&m, host)?;
    let id = format!("{} {}", m.name, m.version);
    let parent = packages_dir(data).join(&m.name);
    let dest = parent.join(m.version.to_string());
    if dest.join(COMPLETE_MARK).is_file() {
        let path = dest.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let old = Manifest::parse_str(&text, &dest.display().to_string())?;
        if old.files == m.files {
            check_rustc(&m, host)?;
            return Ok(InstallOutcome::AlreadyInstalled(Installed {
                manifest: old,
                dir: std::path::absolute(&dest)?,
            }));
        }
        bail!(
            "{id} is already installed with different contents. Run 'juiz uninstall {}@{}' first.",
            m.name,
            m.version
        );
    }
    // 印の無い置きかけ(前回の install が途中で止まった)は消して置き直す
    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .with_context(|| format!("cannot remove {}", dest.display()))?;
    }
    std::fs::create_dir_all(&parent)
        .with_context(|| format!("cannot create {}", parent.display()))?;
    extract(&mut zip, &m, &dest)?;
    if let Err(e) = check_rustc(&m, host) {
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::remove_dir(&parent);
        return Err(e);
    }
    let installed = Installed {
        manifest: m,
        dir: std::path::absolute(&dest)?,
    };
    warn_missing_depends(&installed.manifest, &list(data)?);
    Ok(InstallOutcome::Installed(installed))
}

/// `juiz install` は依存先を入れないので、足りない基礎を警告する(§8.2)
fn warn_missing_depends(m: &Manifest, installed: &[Installed]) {
    for (dep, range) in &m.depends {
        let req = VersionReq::parse(range).ok();
        let found = installed
            .iter()
            .filter(|i| i.manifest.kind == Kind::Base)
            .any(|i| {
                i.manifest.name == *dep
                    && req.as_ref().is_none_or(|r| r.matches(&i.manifest.version))
            });
        if !found {
            warn(&format!(
                "{} depends on {dep} {range}, but it is not installed. Install it with: juiz install {dep}-<version>.juizapp",
                m.name
            ));
        }
    }
}

/// `juiz uninstall <name>[@<version>]`。消した版の一覧を返す
pub(crate) fn uninstall(spec: &str, data: &Path) -> Result<Vec<Installed>> {
    let (name, version) = parse_spec(spec)?;
    let targets: Vec<Installed> = list(data)?
        .into_iter()
        .filter(|i| i.manifest.name == name)
        .filter(|i| version.as_ref().is_none_or(|v| *v == i.manifest.version))
        .collect();
    if targets.is_empty() {
        bail!("{spec} is not installed");
    }
    for t in &targets {
        std::fs::remove_dir_all(&t.dir)
            .with_context(|| format!("cannot remove {}", t.dir.display()))?;
    }
    let parent = packages_dir(data).join(&name);
    // 版が残っていなければ名前のディレクトリも消す(置きかけが残っていれば残す)
    if std::fs::read_dir(&parent).is_ok_and(|mut d| d.next().is_none()) {
        let _ = std::fs::remove_dir(&parent);
    }
    Ok(targets)
}

/// `juiz packages` の表(§8.2)
pub(crate) fn format_table(installed: &[Installed]) -> String {
    let rows: Vec<[String; 3]> = installed
        .iter()
        .map(|i| {
            [
                i.manifest.name.clone(),
                i.manifest.version.to_string(),
                i.manifest.kind.as_str().to_owned(),
            ]
        })
        .collect();
    let header = ["NAME".to_owned(), "VERSION".to_owned(), "KIND".to_owned()];
    let w0 = rows
        .iter()
        .chain([&header])
        .map(|r| r[0].len())
        .max()
        .unwrap_or(0);
    let w1 = rows
        .iter()
        .chain([&header])
        .map(|r| r[1].len())
        .max()
        .unwrap_or(0);
    let mut s = String::new();
    for r in std::iter::once(&header).chain(rows.iter()) {
        s.push_str(&format!("{:w0$}  {:w1$}  {}\n", r[0], r[1], r[2]));
    }
    s
}

/// インストール済みのパッケージを `juiz run <name>[@<version>]` するときの検証(§9: 段 1・5 を省く)
pub(crate) fn prepare_installed(spec: &str, data: &Path, host: &Host) -> Result<Prepared> {
    let (name, version) = parse_spec(spec)?;
    let installed = list(data)?;
    let found = match &version {
        Some(v) => installed
            .iter()
            .find(|i| i.manifest.name == name && i.manifest.version == *v),
        None => find_newest(&installed, &name, None),
    };
    let Some(found) = found else {
        if installed.iter().any(|i| i.manifest.name == name) {
            bail!("{spec} is not installed. Installed versions are shown by 'juiz packages'.");
        }
        bail!(
            "'{spec}' is neither a .juizapp file nor an installed package. Install it with: juiz install {name}-<version>.juizapp"
        );
    };
    let m = found.manifest.clone();
    m.check_packed(&found.dir.display().to_string())?;
    check_host(&m, host)?;
    check_rustc(&m, host)?;
    Ok(Prepared {
        manifest: m,
        dir: found.dir.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::run::tests::{host, tmp, write_pkg, HEAD};

    const BASE_HEAD: &str = "format: 1\nname: robot_base\nversion: 1.2.0\nkind: base\njuiz: \">=0.0.28\"\nlanguages: [python]\ntargets: [any]";

    #[test]
    fn install_list_run_uninstall() {
        let d = tmp("install");
        let data = d.join("data");
        let pkg = d.join("robot_base-1.2.0.juizapp");
        write_pkg(&pkg, BASE_HEAD, &[("app.conf", b"name: b\n")], None);
        let r = install(&pkg, &data, &host()).unwrap();
        let InstallOutcome::Installed(i) = r else {
            panic!("{r:?}")
        };
        assert_eq!(
            i.dir,
            std::path::absolute(data.join("packages/robot_base/1.2.0")).unwrap()
        );
        assert!(i.dir.join("app.conf").is_file());
        assert!(i.dir.join(MANIFEST_FILE).is_file());
        // 同じものをもう一度入れても何もしない
        assert!(matches!(
            install(&pkg, &data, &host()).unwrap(),
            InstallOutcome::AlreadyInstalled(_)
        ));
        // 同じ版で中身が違えばエラー
        let other = d.join("other.juizapp");
        write_pkg(&other, BASE_HEAD, &[("app.conf", b"name: c\n")], None);
        let e = install(&other, &data, &host()).unwrap_err().to_string();
        assert!(
            e.contains("robot_base 1.2.0 is already installed with different contents. Run 'juiz uninstall robot_base@1.2.0' first."),
            "{e}"
        );
        // 別の版は並べて置ける
        let v13 = d.join("robot_base-1.3.0.juizapp");
        write_pkg(
            &v13,
            &BASE_HEAD.replace("1.2.0", "1.3.0"),
            &[("app.conf", b"name: b\n")],
            None,
        );
        install(&v13, &data, &host()).unwrap();
        let all = list(&data).unwrap();
        assert_eq!(all.len(), 2);
        let table = format_table(&all);
        assert_eq!(
            table,
            "NAME        VERSION  KIND\nrobot_base  1.2.0    base\nrobot_base  1.3.0    base\n"
        );
        // 名前で起動するときは一番新しい版、@ で版を選べる
        let p = prepare_installed("robot_base", &data, &host()).unwrap();
        assert_eq!(p.manifest.version.to_string(), "1.3.0");
        let p = prepare_installed("robot_base@1.2.0", &data, &host()).unwrap();
        assert_eq!(p.manifest.version.to_string(), "1.2.0");
        let e = prepare_installed("robot_base@9.0.0", &data, &host())
            .unwrap_err()
            .to_string();
        assert!(e.contains("robot_base@9.0.0 is not installed"), "{e}");
        let e = prepare_installed("nothing", &data, &host())
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("neither a .juizapp file nor an installed package"),
            "{e}"
        );
        let req = VersionReq::parse(">=1.2, <1.3").unwrap();
        assert_eq!(
            find_newest(&all, "robot_base", Some(&req))
                .unwrap()
                .manifest
                .version
                .to_string(),
            "1.2.0"
        );
        // 消す
        let gone = uninstall("robot_base@1.2.0", &data).unwrap();
        assert_eq!(gone.len(), 1);
        assert_eq!(list(&data).unwrap().len(), 1);
        uninstall("robot_base", &data).unwrap();
        assert!(list(&data).unwrap().is_empty());
        assert!(!data.join("packages/robot_base").exists());
        let e = uninstall("robot_base", &data).unwrap_err().to_string();
        assert_eq!(e, "robot_base is not installed");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn install_rejects_other_target() {
        let d = tmp("install_target");
        let pkg = d.join("x.juizapp");
        write_pkg(
            &pkg,
            "format: 1\nname: app\nversion: 0.1.0\njuiz: \">=0.0.1\"\nlanguages: [rust]\ntargets: [aarch64-apple-darwin]\nrust: {juiz_sdk: 0.0.28, rustc: x}",
            &[("app.conf", b"name: a\n")],
            None,
        );
        let e = install(&pkg, &d.join("data"), &host())
            .unwrap_err()
            .to_string();
        assert!(e.contains("is built for aarch64-apple-darwin"), "{e}");
        assert!(list(&d.join("data")).unwrap().is_empty());
        // HEAD の app は入る
        let ok = d.join("app.juizapp");
        write_pkg(&ok, HEAD, &[("app.conf", b"name: a\n")], None);
        install(&ok, &d.join("data"), &host()).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn parses_spec() {
        assert_eq!(parse_spec("a").unwrap(), ("a".to_owned(), None));
        assert_eq!(
            parse_spec("a@1.2.0").unwrap(),
            ("a".to_owned(), Some(Version::parse("1.2.0").unwrap()))
        );
        assert!(parse_spec("a@1.2").is_err());
    }
}
