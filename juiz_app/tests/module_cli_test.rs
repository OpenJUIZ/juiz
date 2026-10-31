//! プラグインを直接読み込む起動方法 (`juiz --process / --container / --component`) のテスト (issue #222)
//!
//! - `-1` で 1 つずつ作る、`-c <型名>` で指定した型だけ作る、`-e` で 1 回実行、`--print` で結果を出す、`-m` でマニフェストを出す
//! - `-d` (常駐) と `-r <Hz>` (周期実行) の経路
//! - Windows でプラグインの隣の juiz_sdk.dll を探す (issue #273)
//!
//! プラグイン (increment_process, example_container とその ContainerProcess, native_pose_component, cpp_component) は
//! scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{path_str, plugin_file, run, run_ok, temp_dir, Daemon};
use std::path::{Path, PathBuf};

/// -d の HTTP ブローカーが使うポート (既定の 8000 は使わない)
const PORT: u16 = 8071;

fn write_conf(dir: &Path) -> PathBuf {
    let conf = dir.join("juiz.conf");
    std::fs::write(
        &conf,
        format!("name: module_cli_test\noption:\n  http_broker:\n    port: {PORT}\n"),
    )
    .unwrap();
    conf
}

fn juiz_with(conf: &Path, args: &[&str]) -> String {
    let mut all = vec!["-f", path_str(conf)];
    all.extend_from_slice(args);
    run_ok(&all)
}

#[test]
fn process_module_create_every_and_execute() {
    let dir = temp_dir("module_cli_process");
    let conf = write_conf(&dir);
    let inc = plugin_file("increment_process");
    let out = juiz_with(&conf, &["--process", &inc, "-1", "-e", "--print", "-m"]);
    assert!(out.contains("type_name: \"increment_process\""), "{out}");
    assert!(out.contains("Value(Number(1))"), "{out}");

    // -e なしは作るだけ
    let out = juiz_with(&conf, &["--process", &inc, "-1", "--print"]);
    assert!(!out.contains("Value("), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn process_module_create_by_type_name() {
    let dir = temp_dir("module_cli_process_c");
    let conf = write_conf(&dir);
    let inc = plugin_file("increment_process");
    let out = juiz_with(
        &conf,
        &[
            "--process",
            &inc,
            "-c",
            "increment_process",
            "-e",
            "--print",
        ],
    );
    assert!(out.contains("Value(Number(1))"), "{out}");

    // 型名が違えば作らない
    let out = juiz_with(&conf, &["--process", &inc, "-c", "other", "-e", "--print"]);
    assert!(!out.contains("Value("), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn container_module_create_every_and_execute() {
    let dir = temp_dir("module_cli_container");
    let conf = write_conf(&dir);
    let out = juiz_with(
        &conf,
        &[
            "--container",
            &plugin_file("example_container"),
            "--container_process",
            &plugin_file("example_container_increment"),
            "-1",
            "-e",
            "--print",
            "-m",
        ],
    );
    assert!(out.contains("type_name: \"example_container\""), "{out}");
    assert!(
        out.contains("type_name: \"example_container_increment\""),
        "{out}"
    );
    assert!(out.contains("Value(Number(1))"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn container_module_create_by_type_name() {
    let dir = temp_dir("module_cli_container_c");
    let conf = write_conf(&dir);
    let container = plugin_file("example_container");
    let get = plugin_file("example_container_get");
    let out = juiz_with(
        &conf,
        &[
            "--container",
            &container,
            "--container_process",
            &get,
            "-c",
            "example_container,example_container_get",
            "-e",
            "--print",
        ],
    );
    assert!(out.contains("Value(Number(0))"), "{out}");

    // Container を作らなければ ContainerProcess も作らない
    let out = juiz_with(
        &conf,
        &[
            "--container",
            &container,
            "--container_process",
            &get,
            "-c",
            "example_container_get",
            "-e",
            "--print",
        ],
    );
    assert!(!out.contains("Value("), "{out}");

    // ContainerProcess を指定しなければ Container だけ作る
    let out = juiz_with(
        &conf,
        &["--container", &container, "-c", "example_container", "-m"],
    );
    assert!(out.contains("type_name: \"example_container\""), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn component_module_execute() {
    let dir = temp_dir("module_cli_component");
    let conf = write_conf(&dir);
    // Process だけのコンポーネント
    let out = juiz_with(
        &conf,
        &[
            "--component",
            &plugin_file("native_pose_component"),
            "-e",
            "--print",
            "-m",
        ],
    );
    assert!(
        out.contains("ComponentManifest(\"native_pose_component\""),
        "{out}"
    );
    // C ABI で読むので、Native は境界で encode されて値で返る (docs/spec/rust-plugin-abi.md §10、
    // Native を運ぶのは Step 3 #331。そこで Native(rust:robot.geometry.Pose2D) に戻す)
    assert!(
        out.contains(r#"Value(Object {"position": Object {"x": Number(1.0), "y": Number(0.0)}, "theta": Number(0.0)})"#),
        "{out}"
    );

    // Container を含むコンポーネント (C++)
    let out = juiz_with(
        &conf,
        &[
            "--component",
            &plugin_file("cpp_component"),
            "-l",
            "cpp",
            "-e",
            "--print",
        ],
    );
    // example_increment(3) = 4、example_cpp_container_get = 0、example_cpp_container_increment(1) = 1
    for v in ["Value(Number(4))", "Value(Number(0))", "Value(Number(1))"] {
        assert!(out.contains(v), "{v}: {out}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Windows: juiz.exe と別のディレクトリにある C++ のプラグインを、その隣に置いた juiz_sdk.dll で開ける (issue #273)。
///
/// juiz.exe のある target/debug にも juiz_sdk.dll があり、Windows は juiz.exe のディレクトリから DLL を探すので、
/// juiz.exe を別のディレクトリに写して起動する。PATH からも juiz_sdk.dll のあるディレクトリ
/// (cargo test が足す target/debug/deps など) を外す。プラグインの隣を探さないと juiz_sdk.dll が見つからない。
#[cfg(windows)]
#[test]
fn cpp_plugin_finds_juiz_sdk_dll_next_to_it() {
    use common::plugin_dir;
    use std::process::Command;

    let dir = temp_dir("module_cli_dll_search");
    let conf = write_conf(&dir);
    let bin = dir.join("bin");
    let plugins = dir.join("plugins");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&plugins).unwrap();
    let exe = bin.join("juiz.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_juiz"), &exe).unwrap();
    let plugin = plugins.join("cpp_component.dll");
    std::fs::copy(plugin_file("cpp_component"), &plugin).unwrap();
    std::fs::copy(
        plugin_dir().join("juiz_sdk.dll"),
        plugins.join("juiz_sdk.dll"),
    )
    .unwrap();

    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::env::split_paths(&path).filter(|p| !p.join("juiz_sdk.dll").exists()),
    )
    .unwrap();
    let out = Command::new(&exe)
        .env("PATH", path)
        .args([
            "-f",
            path_str(&conf),
            "--component",
            path_str(&plugin),
            "-l",
            "cpp",
            "-e",
            "--print",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.contains("Value(Number(4))"), "{stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_module_is_error() {
    let dir = temp_dir("module_cli_missing");
    let conf = write_conf(&dir);
    let missing = dir.join("no_such_plugin.so");
    let out = run(&["-f", path_str(&conf), "--process", path_str(&missing), "-1"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("Error"),
        "{out:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn daemon(dir: &Path, name: &str, args: &[&str]) -> Daemon {
    let conf = write_conf(dir);
    let mut all = vec!["-f", path_str(&conf)];
    all.extend_from_slice(args);
    Daemon::spawn(&all, dir.join(format!("{name}.log")))
}

/// -d では作って実行したあと、止められるまで待つ
#[test]
fn daemon_creates_and_executes_modules() {
    let dir = temp_dir("module_cli_daemon");
    let inc = plugin_file("increment_process");
    let mut d = daemon(
        &dir,
        "process",
        &["-d", "--process", &inc, "-1", "-e", "--print", "-m"],
    );
    d.wait_log("Value(Number(1))");
    d.stop();
    assert!(d.log_text().contains("type_name: \"increment_process\""));

    let container = plugin_file("example_container");
    let cinc = plugin_file("example_container_increment");
    let mut d = daemon(
        &dir,
        "container",
        &[
            "-d",
            "--container",
            &container,
            "--container_process",
            &cinc,
            "-1",
            "-e",
            "--print",
            "-m",
        ],
    );
    d.wait_log("Value(Number(1))");
    d.stop();
    assert!(d
        .log_text()
        .contains("type_name: \"example_container_increment\""));

    let mut d = daemon(
        &dir,
        "component",
        &[
            "-d",
            "--component",
            &plugin_file("cpp_component"),
            "-l",
            "cpp",
            "-1",
            "-e",
            "--print",
            "-m",
        ],
    );
    d.wait_log("Value(Number(4))");
    d.stop();
    assert!(d.log_text().contains("ComponentManifest(\"cpp_component\""));
    let _ = std::fs::remove_dir_all(&dir);
}

/// 出力の中の text の数
fn count(log: &str, text: &str) -> usize {
    log.matches(text).count()
}

/// -r では作ったものを周期で実行し続ける
#[test]
fn ratio_executes_periodically() {
    let dir = temp_dir("module_cli_ratio");
    let inc = plugin_file("increment_process");
    let d = daemon(
        &dir,
        "process",
        &["-r", "20", "--process", &inc, "-1", "--print"],
    );
    wait_count(&d, "Value(Number(1))", 3);
    drop(d);

    let container = plugin_file("example_container");
    let cinc = plugin_file("example_container_increment");
    let d = daemon(
        &dir,
        "container",
        &[
            "-r",
            "20",
            "--container",
            &container,
            "--container_process",
            &cinc,
            "-1",
            "--print",
        ],
    );
    // Container の値が 1 ずつ増えていく
    d.wait_log("Value(Number(3))");
    drop(d);

    // コンポーネントは最初の 1 回だけ作り、2 周目以降も同じものを実行する
    let d = daemon(
        &dir,
        "component",
        &[
            "-r",
            "20",
            "--component",
            &plugin_file("native_pose_component"),
            "-1",
            "--print",
        ],
    );
    wait_count(&d, "Value(Number(0))", 3);
    drop(d);
    let log = std::fs::read_to_string(dir.join("component.log")).unwrap();
    assert!(!log.contains("already registered"), "{log}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn wait_count(d: &Daemon, text: &str, n: usize) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while count(&d.log_text(), text) < n {
        if std::time::Instant::now() > deadline {
            panic!("'{text}' did not appear {n} times. log:\n{}", d.log_text());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
