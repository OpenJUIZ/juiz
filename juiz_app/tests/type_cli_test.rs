//! `juiz type list / show` (データ型 Step 8、issue #197)
//!
//! 型付きの Process を持つ juiz を `-d` で起動し、別の juiz コマンドから型の登録所を見る。
//! プラグイン native_pose_component (juiz_sdk/test_packages) をワークスペースの cargo build で作っておく必要がある
//! (scripts/test.sh)。
use assert_cmd::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const PORT: u16 = 8052;

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

/// ワークスペースの cargo build でできるプラグインの場所 (namespace_cli_test と同じ)
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

fn run(client: &Path, args: &[&str]) -> std::process::Output {
    juiz()
        .arg("-f")
        .arg(client)
        .args(["-s", &format!("http://127.0.0.1:{PORT}")])
        .args(args)
        .output()
        .unwrap()
}

fn run_ok(client: &Path, args: &[&str]) -> String {
    let out = run(client, args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn type_list_and_show() {
    let dir = temp_dir("type_cli_test");
    let robot = dir.join("robot.conf");
    std::fs::write(
        &robot,
        format!(
            r#"name: robot
option:
  http_broker:
    start: true
    port: {PORT}
plugins:
  components:
    native_pose_component:
      path: "{}"
"#,
            plugin_dir().display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let client = dir.join("client.conf");
    std::fs::write(&client, "name: client\n").unwrap();
    let log = std::fs::File::create(dir.join("robot.log")).unwrap();
    let _robot = Daemon(
        juiz()
            .arg("-f")
            .arg(&robot)
            .arg("-d")
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );

    // 起動を待つ
    let deadline = Instant::now() + Duration::from_secs(60);
    let list = loop {
        let out = run(&client, &["type", "list"]);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() && stdout.contains("robot.geometry.Pose2D@sha256:") {
            break stdout;
        }
        if Instant::now() > deadline {
            let log = std::fs::read_to_string(dir.join("robot.log")).unwrap_or_default();
            panic!("types did not appear. out: {out:?}\nlog:\n{log}");
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    // 識別子の JSON 配列
    let ids: Vec<String> = juiz_sdk::serde_json::from_str(list.trim()).unwrap();
    assert!(
        ids.iter()
            .any(|i| i.starts_with("robot.geometry.Point2D@sha256:")),
        "{ids:?}"
    );

    // 表: 使っている Process の型名が出る
    let table = run_ok(&client, &["type", "list", "--long"]);
    let lines: Vec<&str> = table.lines().collect();
    assert!(lines[0].starts_with("NAME"), "{table}");
    let pose = lines
        .iter()
        .find(|l| l.starts_with("robot.geometry.Pose2D "))
        .unwrap_or_else(|| panic!("{table}"));
    assert!(pose.contains(" record "), "{table}");
    assert!(pose.contains("native_pose_move"), "{table}");

    // JIDL で表示 (完全名だけで引ける)
    let shown = run_ok(&client, &["type", "show", "robot.geometry.Pose2D"]);
    assert!(shown.contains("namespace robot.geometry;"), "{shown}");
    assert!(
        shown.contains("record Pose2D {\n  Point2D position;\n  double theta = 0.0;\n}"),
        "{shown}"
    );
    assert!(shown.contains("record Point2D {"), "{shown}");
    assert!(shown.contains("// used by: "), "{shown}");

    // .avsc
    let avro: juiz_sdk::serde_json::Value = juiz_sdk::serde_json::from_str(&run_ok(
        &client,
        &["type", "show", "robot.geometry.Pose2D", "--avro"],
    ))
    .unwrap();
    assert_eq!(avro["name"], "Pose2D");
    assert_eq!(avro["namespace"], "robot.geometry");

    // 知らない型はエラー
    let out = run(&client, &["type", "show", "robot.geometry.Nothing"]);
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("not registered") || all.contains("not found"),
        "{all}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
