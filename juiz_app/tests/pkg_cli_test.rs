//! `juiz pkg` のテスト (docs/spec/pkg.md §11、issue #457)
//!
//! 雛形を一時ディレクトリに `--juiz-source` で作り、そのまま
//! scripts/dev.sh → `juiz -f app/app.conf` → 型を全部 call → scripts/pack.sh → `juiz run` が通ることを確かめる。
//! Rust の雛形のビルドは target/pkg-test に置く (ワークスペースの target と分け、2 回目からは速い)。
mod common;

use common::{juiz, path_str, run, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .unwrap()
}

/// テストしている juiz があるディレクトリを先頭にした PATH (雛形の scripts が juiz を呼ぶため)
fn path_with_juiz() -> String {
    let bin = assert_cmd::cargo::cargo_bin("juiz");
    let dir = bin.parent().unwrap().to_path_buf();
    let mut paths = vec![dir];
    if let Some(p) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(paths)
        .unwrap()
        .into_string()
        .unwrap()
}

#[test]
fn pkg_target_prints_host_target() {
    let out = run(&["pkg", "target"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        juiz_core::HOST_TARGET
    );
}

#[test]
fn pkg_create_errors() {
    let dir = temp_dir("pkg_cli_errors");
    let out_dir = dir.join("x");
    let o = path_str(&out_dir);
    // 名前の規則に合わない (直した例を出す)
    let r = run(&["pkg", "create", "My-Robot", "-b", "rust", "-o", o]);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("'my_robot'"), "{err}");
    // 知らない言語 (受け付ける値を並べる)
    let r = run(&["pkg", "create", "my_robot", "-b", "go", "-o", o]);
    assert!(!r.status.success());
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("rust") && err.contains("python"), "{err}");
    // 何も残さない
    assert!(!out_dir.exists());
    // 空でないディレクトリには作らない
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("keep"), "x").unwrap();
    let r = run(&["pkg", "create", "my_robot", "-b", "rust", "-o", o]);
    assert!(!r.status.success());
    assert_eq!(std::fs::read_dir(&out_dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 生成した app.conf のポートをテスト用に変える (既定の 8000 は他と重なりうるので)
fn set_port(app: &Path, port: u16) {
    let conf = app.join("app").join("app.conf");
    let text = std::fs::read_to_string(&conf).unwrap();
    assert!(text.contains("\"port\": 8000\n"), "{text}");
    std::fs::write(&conf, text.replace("\"port\": 8000\n", &format!("\"port\": {port}\n"))).unwrap();
}

fn create(dir: &Path, name: &str, lang: &str, extra: &[&str]) -> PathBuf {
    let app = dir.join(name);
    let repo = repo_root();
    let mut args = vec![
        "pkg",
        "create",
        name,
        "-b",
        lang,
        "-o",
        path_str(&app),
        "--juiz-source",
        path_str(&repo),
        "--license",
        "mit-apache",
        "--author",
        "juiz test",
    ];
    args.extend_from_slice(extra);
    let out = run(&args);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Next steps:"), "{stdout}");
    app
}

fn script(app: &Path, name: &str, args: &[&str]) {
    let out = Command::new(app.join("scripts").join(name))
        .args(args)
        .current_dir(app)
        .env("PATH", path_with_juiz())
        .env("CARGO_TARGET_DIR", repo_root().join("target").join("pkg-test"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{name}: {out:?}");
}

/// juiz を起動し、id が呼べるようになるまで待つ
fn start(cmd_args: &[&str], app: &Path, log: PathBuf, first_id: &str) -> Daemon {
    let mut cmd = juiz();
    cmd.args(cmd_args).current_dir(app);
    let d = Daemon::spawn_command(cmd, log);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let out = juiz()
            .args(["process", "call", first_id, "{}", "-p"])
            .current_dir(app)
            .output()
            .unwrap();
        if out.status.success() && String::from_utf8_lossy(&out.stdout).contains("Value(") {
            return d;
        }
        if Instant::now() > deadline {
            panic!("{first_id} did not start. log:\n{}", d.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn call(app: &Path, kind: &str, id: &str, args: &str) -> String {
    let out = juiz()
        .args([kind, "call", id, args, "-p"])
        .current_dir(app)
        .output()
        .unwrap();
    assert!(out.status.success(), "{id}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// §4 の型を全部呼んで、決めた値が返ることを確かめる
fn call_all(app: &Path, name: &str, port: u16) {
    let h = format!("http://127.0.0.1:{port}");
    let add = format!("{h}/process/{name}_add0::{name}_add");
    assert!(call(app, "process", &add, r#"{"a": 1, "b": 2}"#).contains("Value(3)"));
    assert!(call(app, "process", &add, "{}").contains("Value(0)"));
    let inc = format!("{h}/container_process/{name}_increment0::{name}_increment:{name}0");
    let get = format!("{h}/container_process/{name}_get0::{name}_get:{name}0");
    assert!(call(app, "container-process", &get, "{}").contains("Value(0)"));
    assert!(call(app, "container-process", &inc, r#"{"step": 2}"#).contains("Value(2)"));
    assert!(call(app, "container-process", &inc, "{}").contains("Value(3)"));
    assert!(call(app, "container-process", &get, "{}").contains("Value(3)"));
    // _get は readonly
    let out = juiz()
        .args(["-s", &h, "container-process", "list", "--long"])
        .current_dir(app)
        .output()
        .unwrap();
    let list = String::from_utf8_lossy(&out.stdout);
    assert!(list.contains("readonly"), "{list}");
}

fn create_build_run_pack(lang: &str, name: &str, port: u16, build: bool, extra: &[&str]) {
    let dir = temp_dir(&format!("pkg_cli_{lang}"));
    let app = create(&dir, name, lang, extra);
    set_port(&app, port);
    if build {
        script(&app, "dev.sh", &[]);
        assert!(app
            .join("app")
            .join("plugins")
            .join(juiz_core::HOST_TARGET)
            .is_dir());
    }
    let add = format!("http://127.0.0.1:{port}/process/{name}_add0::{name}_add");

    let mut d = start(&["-f", "app/app.conf", "-d"], &app, dir.join("dev.log"), &add);
    call_all(&app, name, port);
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());

    // dev.sh でビルド済みなので、pack はビルドを飛ばす (release ビルドの時間を省く)
    script(&app, "pack.sh", &["--no-build"]);
    let pkg = app.join("dist").join(format!("{name}-0.1.0.juizapp"));
    assert!(pkg.is_file());
    check_package(&pkg, lang);
    let mut d = start(
        &["run", path_str(&pkg)],
        &app,
        dir.join("run.log"),
        &add,
    );
    call_all(&app, name, port);
    assert!(d.stop() || cfg!(not(unix)), "{}", d.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}

/// できた .juizapp の中身を確かめる (C++ は libjuiz_sdk を同梱し、同じ版の juiz を求める)
fn check_package(pkg: &Path, lang: &str) {
    let file = std::fs::File::open(pkg).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    let manifest = {
        let mut f = zip.by_name("juizapp.yaml").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        s
    };
    assert!(names.iter().any(|n| n == "LICENSE-MIT"), "{names:?}");
    if lang == "cpp" {
        let sdk = format!(
            "plugins/{}/{}juiz_sdk.{}",
            juiz_core::HOST_TARGET,
            if cfg!(windows) { "" } else { "lib" },
            if cfg!(target_os = "macos") { "dylib" } else if cfg!(windows) { "dll" } else { "so" }
        );
        assert!(names.contains(&sdk), "{sdk} is not in {names:?}");
        assert!(
            manifest.contains(&format!("juiz: \"={}\"", env!("CARGO_PKG_VERSION"))),
            "{manifest}"
        );
    }
}

#[cfg(unix)]
#[test]
fn pkg_create_rust_component_builds_runs_and_packs() {
    create_build_run_pack("rust", "pkg_rust_robot", 8078, true, &[]);
}

#[cfg(all(unix, feature = "embed-python"))]
#[test]
fn pkg_create_python_component_runs_and_packs() {
    create_build_run_pack("python", "pkg_py_robot", 8079, false, &[]);
}

/// ワークスペースでビルドした libjuiz_sdk を、配布と同じ形 (scripts/package-juiz-sdk.sh のアーカイブ) にして展開する。
/// 雛形の CMakeLists.txt は、これを --juiz-sdk-dir で指して find_package(juiz_sdk <版> EXACT) する
fn sdk_from_workspace(dir: &Path) -> PathBuf {
    let lib = if cfg!(target_os = "macos") {
        "libjuiz_sdk.dylib"
    } else {
        "libjuiz_sdk.so"
    };
    // juiz と同じ所にあればそれを使う (scripts/test.sh の cargo build で target/debug にできる)
    let bin_dir = assert_cmd::cargo::cargo_bin("juiz").parent().unwrap().to_path_buf();
    let lib_dir = [bin_dir, repo_root().join("target").join("debug")]
        .into_iter()
        .find(|d| d.join(lib).is_file())
        .unwrap_or_else(|| panic!("{lib} is not built. Run cargo build (scripts/test.sh does) first"));
    let out = Command::new(repo_root().join("scripts").join("package-juiz-sdk.sh"))
        .arg(juiz_core::HOST_TARGET)
        .arg(dir)
        .env("JUIZ_SDK_LIB_DIR", &lib_dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "package-juiz-sdk.sh: {out:?}");
    let archive = String::from_utf8(out.stdout).unwrap().trim().to_owned();
    let out = Command::new("tar")
        .args(["-xzf", &archive, "-C"])
        .arg(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "tar: {out:?}");
    let sdk = dir.join(format!(
        "juiz_sdk-{}-{}",
        env!("CARGO_PKG_VERSION"),
        juiz_core::HOST_TARGET
    ));
    assert!(sdk.join("lib").join("cmake").join("juiz_sdk").is_dir(), "{}", sdk.display());
    sdk
}

#[cfg(unix)]
#[test]
fn pkg_create_cpp_component_builds_runs_and_packs() {
    let dir = temp_dir("pkg_cli_cpp_sdk");
    let sdk = sdk_from_workspace(&dir);
    create_build_run_pack("cpp", "pkg_cpp_robot", 8077, true, &["--juiz-sdk-dir", path_str(&sdk)]);
    let _ = std::fs::remove_dir_all(&dir);
}
