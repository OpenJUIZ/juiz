//! `juiz pack` と `juiz run` のテスト (issue #285、docs/spec/app-package.md §6・§7)
//!
//! プラグイン (increment_process) は scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{juiz, path_str, plugin_file, temp_dir, Daemon};
use std::io::Read;
use std::path::Path;

/// Rust のプラグイン 1 つを plugins/<target>/ に置いたアプリのディレクトリを作る
fn write_rust_app(dir: &Path, juiz_field: &str) {
    let plugins = dir.join("plugins").join(juiz_core::HOST_TARGET);
    std::fs::create_dir_all(&plugins).unwrap();
    let src = plugin_file("increment_process");
    std::fs::copy(&src, plugins.join(Path::new(&src).file_name().unwrap())).unwrap();
    std::fs::write(
        dir.join("juizapp.yaml"),
        format!("format: 1\nname: inc_app\nversion: 0.1.0\n{juiz_field}exclude: [\"*.log\"]\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("app.conf"),
        r#"name: package_cli_test
option:
  http_broker:
    start: false
plugins:
  process_factories:
    increment_process:
      path: "plugins/{target}"
processes:
  - type_name: increment_process
    name: inc0
"#,
    )
    .unwrap();
    // 入れないもの
    std::fs::write(dir.join("debug.log"), "x").unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".git").join("HEAD"), "x").unwrap();
    std::fs::create_dir_all(dir.join("target")).unwrap();
    std::fs::write(dir.join("target").join("big"), "x").unwrap();
}

fn zip_names(path: &Path) -> Vec<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
        .collect()
}

fn zip_manifest(path: &Path) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut s = String::new();
    zip.by_name("juizapp.yaml")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

#[test]
fn pack_and_run_rust_app() {
    let dir = temp_dir("package_cli");
    let app = dir.join("app");
    let out = dir.join("out");
    let cache = dir.join("cache");
    write_rust_app(&app, "");
    std::fs::create_dir_all(&out).unwrap();

    let o = juiz()
        .args(["pack", path_str(&app), "-o", path_str(&out)])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let pkg = out.join("inc_app-0.1.0.juizapp");
    assert!(pkg.is_file(), "{o:?}");

    // 先頭はマニフェスト、残りは辞書順。入れないものは入っていない
    let lib = Path::new(&plugin_file("increment_process"))
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        zip_names(&pkg),
        vec![
            "juizapp.yaml".to_owned(),
            "app.conf".to_owned(),
            format!("plugins/{}/{lib}", juiz_core::HOST_TARGET),
        ]
    );
    let manifest = zip_manifest(&pkg);
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        manifest.contains(&format!("juiz: \"={version}\"")),
        "{manifest}"
    );
    assert!(manifest.contains("languages:\n  - rust\n"), "{manifest}");
    assert!(manifest.contains(juiz_core::HOST_TARGET), "{manifest}");
    assert!(manifest.contains("rustc: "), "{manifest}");
    assert!(manifest.contains("sha256:"), "{manifest}");

    // 同じ入力からは同じバイト列ができる
    let first = std::fs::read(&pkg).unwrap();
    let o = juiz()
        .args(["pack", path_str(&app), "-o", path_str(&out)])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert_eq!(std::fs::read(&pkg).unwrap(), first);

    // juiz run: 別のディレクトリから起動し、キャッシュに展開して動かす
    let mut cmd = juiz();
    cmd.current_dir(&dir)
        .env("JUIZ_CACHE_DIR", &cache)
        .env("RUST_LOG", "info")
        .args(["run", "out/inc_app-0.1.0.juizapp"]);
    let mut daemon = Daemon::spawn_command(cmd, dir.join("run.log"));
    daemon.wait_log("core://core/process/inc0::increment_process");
    daemon.stop();
    let text = daemon.log_text();
    assert!(text.contains("Running inc_app 0.1.0 from"), "{text}");
    let apps: Vec<_> = std::fs::read_dir(cache.join("apps"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(apps.len(), 1, "{apps:?}");
    assert!(apps[0].starts_with("inc_app-0.1.0-"), "{apps:?}");
    assert!(cache
        .join("apps")
        .join(&apps[0])
        .join(".juizapp-complete")
        .is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pack_rejects_inexact_juiz_with_rust_plugins() {
    let dir = temp_dir("package_cli_inexact");
    let app = dir.join("app");
    write_rust_app(&app, "juiz: \">=0.0.1\"\n");
    let o = juiz()
        .current_dir(&dir)
        .args(["pack", "app"])
        .output()
        .unwrap();
    assert!(!o.status.success(), "{o:?}");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("'juiz' in juizapp.yaml must be"), "{err}");
    // 何も書かない
    assert!(!dir.join("inc_app-0.1.0.juizapp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "embed-python")]
#[test]
fn run_reports_missing_python_packages() {
    let dir = temp_dir("package_cli_python");
    let app = dir.join("app");
    std::fs::create_dir_all(app.join("python")).unwrap();
    std::fs::write(
        app.join("juizapp.yaml"),
        "format: 1\nname: py_app\nversion: 0.2.0\npython:\n  requires: [\"surely-not-installed-juiz-package>=1\"]\n",
    )
    .unwrap();
    std::fs::write(
        app.join("app.conf"),
        "name: py_app\nplugins:\n  process_factories:\n    listener:\n      language: python\n      path: python\n",
    )
    .unwrap();
    std::fs::write(app.join("python").join("listener.py"), "x = 1\n").unwrap();
    let o = juiz()
        .current_dir(&dir)
        .args(["pack", "app"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let pkg = dir.join("py_app-0.2.0.juizapp");
    let manifest = zip_manifest(&pkg);
    assert!(manifest.contains("targets:\n  - any\n"), "{manifest}");
    assert!(
        manifest.contains(&format!("juiz: \">={}\"", env!("CARGO_PKG_VERSION"))),
        "{manifest}"
    );

    let o = juiz()
        .current_dir(&dir)
        .env("JUIZ_CACHE_DIR", dir.join("cache"))
        .args(["run", "py_app-0.2.0.juizapp"])
        .output()
        .unwrap();
    assert!(!o.status.success(), "{o:?}");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains(
            "needs Python packages that are not installed: surely-not-installed-juiz-package>=1"
        ),
        "{err}"
    );
    assert!(
        err.contains("-m pip install \"surely-not-installed-juiz-package>=1\""),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_rejects_missing_file() {
    let o = juiz()
        .args(["run", "no_such_app.juizapp"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("no_such_app.juizapp is not found"));
}
