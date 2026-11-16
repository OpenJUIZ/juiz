//! 同じ conf を 2 つのネームスペースで起動し、それぞれ call できることを確かめる (issue #151)
//!
//! - `juiz -f robot.conf --namespace robot1 -d` と `--namespace robot2 -d` を同じポートで起動する
//!   (2 台目は 1 台目のサブシステムになり、次のポートで待ち受ける)
//! - `juiz process list --namespace robot2` で絞り込み、`--long` でネームスペースの列を見る
//! - `juiz process call` でネームスペース付き ID を呼ぶ
//!
//! プラグイン increment_process をワークスペースの cargo build で作っておく必要がある (scripts/test.sh)。
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const MASTER_PORT: u16 = 8050;
const SUB_PORT: u16 = 8051;

/// 起動した juiz をテストの終わりに止める
struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn juiz() -> Command {
    Command::cargo_bin("juiz").unwrap()
}

/// ワークスペースの cargo build でできるプラグインの場所 (他のテストの "../target/debug" と同じ)。
/// cargo llvm-cov ではテストの実行ファイルが別の target にできるが、プラグインはここを使う
fn plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("juiz_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// robot.conf (Process を 1 つ持つ juiz) と client.conf (コマンド側の空の juiz)
fn write_confs(dir: &Path) -> (PathBuf, PathBuf) {
    let robot = dir.join("robot.conf");
    std::fs::write(
        &robot,
        format!(
            r#"name: robot
option:
  http_broker:
    start: true
    port: {MASTER_PORT}
plugins:
  process_factories:
    increment_process:
      path: "{}"
processes:
  - type_name: increment_process
    name: inc0
"#,
            plugin_dir().display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let client = dir.join("client.conf");
    std::fs::write(&client, "name: client\n").unwrap();
    (robot, client)
}

fn start(robot: &Path, namespace: &str) -> Daemon {
    let log = robot.with_file_name(format!("{}.log", namespace.trim_start_matches('/')));
    let log = std::fs::File::create(log).unwrap();
    Daemon(
        juiz()
            .arg("-f")
            .arg(robot)
            .args(["--namespace", namespace, "-d"])
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    )
}

fn run(client: &Path, args: &[&str]) -> String {
    let out = juiz()
        .arg("-f")
        .arg(client)
        .args(["-s", &format!("http://127.0.0.1:{MASTER_PORT}")])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn process_list(client: &Path, extra: &[&str]) -> String {
    let client_path = client.to_str().unwrap();
    let mut args = vec!["process", "list", "-f", client_path];
    args.extend_from_slice(extra);
    run(client, &args)
}

fn wait_for(client: &Path, id: &str, log: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !process_list(client, &[]).contains(id) {
        if Instant::now() > deadline {
            let log = std::fs::read_to_string(client.with_file_name(log)).unwrap_or_default();
            panic!("{id} did not appear. log:\n{log}");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn robot_id(port: u16, namespace: &str) -> String {
    format!("http://127.0.0.1:{port}/process/{namespace}/inc0::increment_process")
}

#[test]
fn same_conf_runs_in_two_namespaces() {
    let dir = temp_dir("namespace_cli_test");
    let (robot, client) = write_confs(&dir);
    let r1 = robot_id(MASTER_PORT, "robot1");
    let r2 = robot_id(SUB_PORT, "robot2");

    let _robot1 = start(&robot, "robot1");
    // 1 台目が待ち受けてから 2 台目を起動する (2 台目は 1 台目のサブシステムになる)
    wait_for(&client, &r1, "robot1.log");
    let _robot2 = start(&robot, "/robot2");
    // 2 台目の登録の手続き中にも 1 台目へ list を送り続ける (issue #159 で詰まらなくなったことの確認を兼ねる)
    wait_for(&client, &r2, "robot2.log");

    // 絞り込み: robot2 だけ
    let robot2_only = process_list(&client, &["--namespace", "robot2"]);
    assert_eq!(robot2_only.trim(), format!("[\"{r2}\"]"));

    // 表: ネームスペースの列が出る
    let table = process_list(&client, &["--namespace", "robot1", "--long"]);
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines.len(), 2, "{table}");
    assert!(lines[0].starts_with("NAMESPACE"), "{table}");
    assert!(lines[1].starts_with("/robot1 "), "{table}");
    assert!(lines[1].ends_with(&r1), "{table}");

    // それぞれ call できる
    let out = run(&client, &["process", "call", &r1, r#"{"arg1": 1}"#, "-p"]);
    assert!(out.contains("Value(2)"), "{out}");
    let out = run(&client, &["process", "call", &r2, r#"{"arg1": 10}"#, "-p"]);
    assert!(out.contains("Value(11)"), "{out}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_namespace_option_is_error() {
    let dir = temp_dir("namespace_cli_test_invalid");
    let (robot, _) = write_confs(&dir);
    juiz()
        .arg("-f")
        .arg(&robot)
        .args(["--namespace", "robot1:x"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("robot1:x"));
    let _ = std::fs::remove_dir_all(&dir);
}
