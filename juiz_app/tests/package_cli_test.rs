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
    // Rust のプラグインは OS の基本のライブラリしか使わないので、依存の警告は出ない (issue #287)
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains(" needs "), "{err}");
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

/// プラグインの無い(conf だけの)パッケージを作って pack する
fn pack_conf_only(dir: &Path, name: &str, manifest_extra: &str, conf: &str) -> std::path::PathBuf {
    let src = dir.join(format!("{name}_src"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("juizapp.yaml"),
        format!("format: 1\nname: {name}\n{manifest_extra}"),
    )
    .unwrap();
    std::fs::write(src.join("app.conf"), conf).unwrap();
    let o = juiz()
        .current_dir(dir)
        .args(["pack", path_str(&src)])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let version = manifest_extra
        .lines()
        .find_map(|l| l.strip_prefix("version: "))
        .unwrap();
    dir.join(format!("{name}-{version}.juizapp"))
}

/// IPC で相手の system の profile を取る。つながらなければ None
fn ipc_profile(name: &str) -> Option<juiz_sdk::serde_json::Value> {
    use juiz_core::prelude::*;
    let mut out = None;
    let _ = System::new(
        jvalue!({ "name": "package_cli_probe", "option": { "http_broker": { "start": false } } }),
    )
    .unwrap()
    .setup()
    .unwrap()
    .run_and_do_once(|system| {
        let bp = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("ipc", name, true)?;
        // BrokerProxy は値を {"__value__": …} で包んで返す
        out = juiz_lock(&bp)?
            .system_profile_full()
            .ok()
            .map(|p| p.get("__value__").cloned().unwrap_or(p));
        Ok(())
    });
    out
}

fn wait_profile(
    name: &str,
    ok: impl Fn(&juiz_sdk::serde_json::Value) -> bool,
) -> juiz_sdk::serde_json::Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if let Some(p) = ipc_profile(name) {
            if ok(&p) {
                return p;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "ipc://{name} did not become ready: {:?}",
            ipc_profile(name)
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// 基礎を install して名前で起動し、depends を持つアプリが ipc でつながる (issue #286、§8・§10)
#[test]
fn install_base_and_connect_app() {
    let dir = temp_dir("package_depends");
    let data = dir.join("data");
    let cache = dir.join("cache");
    // 基礎の名前は IPC 名になるので、テストごとに変える
    let base = format!("base_{}", std::process::id() % 100000);
    let quiet = "option:\n  http_broker:\n    start: false\n";
    let base_pkg = pack_conf_only(
        &dir,
        &base,
        "version: 1.2.0\nkind: base\nbase:\n  http_port: 8073\n",
        &format!("name: robot_base_system\n{quiet}"),
    );
    let app_pkg = pack_conf_only(
        &dir,
        "my_patrol",
        &format!("version: 0.1.0\ndepends:\n  {base}: \">=1.2, <2\"\n"),
        "name: my_patrol_system\noption:\n  http_broker:\n    start: false\n    port: 8074\n",
    );
    let juiz_env = || {
        let mut c = juiz();
        c.current_dir(&dir)
            .env("JUIZ_DATA_DIR", &data)
            .env("JUIZ_CACHE_DIR", &cache);
        c
    };
    let run_app_err = || {
        let o = juiz_env()
            .args(["run", path_str(&app_pkg)])
            .output()
            .unwrap();
        assert!(!o.status.success(), "{o:?}");
        String::from_utf8_lossy(&o.stderr).into_owned()
    };

    // 基礎が入っていない
    let err = run_app_err();
    assert!(
        err.contains(&format!(
            "my_patrol depends on {base} >=1.2, <2, but {base} is not installed. Install it with: juiz install {base}-<version>.juizapp"
        )),
        "{err}"
    );

    // install と packages。アプリを入れると足りない基礎を警告する
    let o = juiz_env()
        .args(["install", path_str(&app_pkg)])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("but it is not installed"),
        "{o:?}"
    );
    let o = juiz_env()
        .args(["install", path_str(&base_pkg)])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stdout).contains(&format!("Installed {base} 1.2.0 into")),
        "{o:?}"
    );
    let o = juiz_env().args(["packages"]).output().unwrap();
    let table = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(table.starts_with("NAME"), "{table}");
    assert!(
        table.contains(&format!("{base}")) && table.contains("1.2.0") && table.contains("base"),
        "{table}"
    );
    assert!(
        table.contains("my_patrol") && table.contains("app"),
        "{table}"
    );

    // 入っているが動いていない
    let err = run_app_err();
    assert!(
        err.contains(&format!(
            "but {base} is not running at ipc://{base}. Start it first with: juiz run {base}"
        )),
        "{err}"
    );

    // パッケージを使わずに同じ IPC 名で動いているもの
    {
        let plain = dir.join("plain.conf");
        std::fs::write(&plain, format!("name: plain\n{quiet}")).unwrap();
        let mut d = Daemon::spawn(
            &["-d", "-q", "--ipc", &base, "-f", path_str(&plain)],
            dir.join("plain.log"),
        );
        let p = wait_profile(&base, |_| true);
        assert!(p.get("package").is_none(), "{p}");
        let err = run_app_err();
        assert!(
            err.contains(&format!(
                "ipc://{base} is running but is not started from a package"
            )),
            "{err}"
        );
        d.stop();
    }

    // 基礎を名前で起動すると、IPC 名はパッケージ名で、profile に package が載る
    let mut cmd = juiz_env();
    cmd.args(["run", &base]);
    let mut base_daemon = Daemon::spawn_command(cmd, dir.join("base.log"));
    let p = wait_profile(&base, |p| p.get("package").is_some());
    assert_eq!(p["package"]["name"], base.as_str(), "{p}");
    assert_eq!(p["package"]["version"], "1.2.0", "{p}");
    assert_eq!(p["package"]["kind"], "base", "{p}");
    assert!(base_daemon
        .log_text()
        .contains(&format!("Running {base} 1.2.0 from")));

    // アプリはインストール済みの名前でも起動でき、基礎に subsystem としてつながる
    let mut cmd = juiz_env();
    cmd.args(["run", "my_patrol"]);
    let mut app_daemon = Daemon::spawn_command(cmd, dir.join("app.log"));
    let p = wait_profile(&base, |p| {
        p.get("mastersystem").is_some_and(|m| !m.is_null())
    });
    assert!(p["mastersystem"].is_object(), "{p}");
    app_daemon.stop();

    // --depend で http につなぐ。ポートを省くとインストール済みの基礎の base.http_port (8073) を使う
    let mut cmd = juiz_env();
    cmd.env("RUST_LOG", "info").args([
        "run",
        "my_patrol",
        "--depend",
        &format!("{base}=http://127.0.0.1"),
    ]);
    let mut app_daemon = Daemon::spawn_command(cmd, dir.join("app_http.log"));
    app_daemon.wait_log("Added subsystem(");
    let log = app_daemon.log_text();
    assert!(log.contains("127.0.0.1:8073"), "{log}");
    app_daemon.stop();
    base_daemon.stop();

    // 消す
    let o = juiz_env().args(["uninstall", &base]).output().unwrap();
    assert!(o.status.success(), "{o:?}");
    let o = juiz_env().args(["packages"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&o.stdout).contains(&base), "{o:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
