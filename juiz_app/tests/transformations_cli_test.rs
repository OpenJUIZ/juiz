//! Transformations の CLI (issue #370、仕様 docs/spec/transformations.md §13.2) のテスト
//!
//! conf に Anchor と Transform を持つ juiz を `-d` で起動し、`container pose`・`transform`・`anchor`・
//! `container list --long` を叩く。数値は §3.5 の例 (map → c0 は (1, 0, 0) と z 軸まわり 90°、c0 → c1 は (0.1, 0, 0.3))。
//!
//! プラグイン (example_container) は scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{path_str, plugin_dir_str, run, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PORT: u16 = 8072;

fn host() -> String {
    format!("http://127.0.0.1:{PORT}")
}

fn write_robot_conf(dir: &Path) -> PathBuf {
    let robot = dir.join("robot.conf");
    let p = plugin_dir_str();
    std::fs::write(
        &robot,
        format!(
            r#"name: robot
option:
  http_broker:
    start: true
    port: {PORT}
plugins:
  container_factories:
    example_container:
      path: "{p}"
containers:
  - {{ type_name: static_anchor, name: map }}
  - {{ type_name: periodic_anchor, name: odom, rate: 20.0 }}
  - {{ type_name: example_container, name: c0 }}
  - {{ type_name: example_container, name: c1 }}
  - {{ type_name: example_container, name: c2 }}
transforms:
  - source: map
    destination: c0
    translation: {{x: 1.0, y: 0.0, z: 0.0}}
    rotation: {{x: 0.0, y: 0.0, z: 0.7071067811865476, w: 0.7071067811865476}}
  - source: c0
    destination: c1
    translation: {{x: 0.1, y: 0.0, z: 0.3}}
"#
        ),
    )
    .unwrap();
    robot
}

struct Client {
    conf: PathBuf,
}

impl Client {
    fn run(&self, args: &[&str]) -> String {
        let server = host();
        let mut all = vec!["-f", path_str(&self.conf), "-s", &server];
        all.extend_from_slice(args);
        let out = run(&all);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }
}

fn json_of(out: &str) -> juiz_sdk::serde_json::Value {
    juiz_sdk::serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("not a JSON ({e}): {out}"))
}

fn close(v: &juiz_sdk::serde_json::Value, expected: f64) -> bool {
    (v.as_f64().unwrap() - expected).abs() < 1e-9
}

#[test]
fn transformations_subcommands() {
    let dir = temp_dir("transformations_cli_test");
    let robot_conf = write_robot_conf(&dir);
    let client = Client {
        conf: dir.join("client.conf"),
    };
    std::fs::write(&client.conf, "name: client\n").unwrap();
    let mut robot = Daemon::spawn(&["-f", path_str(&robot_conf), "-d"], dir.join("robot.log"));
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let out = run(&[
            "-f",
            path_str(&client.conf),
            "-s",
            &host(),
            "anchor",
            "list",
        ]);
        if String::from_utf8_lossy(&out.stdout).contains("map::static_anchor") {
            break;
        }
        if Instant::now() > deadline {
            panic!("juiz did not start. log:\n{}", robot.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    // 読む
    let p = json_of(&client.run(&["container", "pose", "c1", "--reference", "map"]));
    assert_eq!(p["header"]["frame_id"], "map");
    let pos = &p["pose"]["position"];
    assert!(
        close(&pos["x"], 1.0) && close(&pos["y"], 0.1) && close(&pos["z"], 0.3),
        "{p}"
    );

    // 書く (親 c0 → c1 の Transform が変わる)
    let p = json_of(&client.run(&[
        "container",
        "pose",
        "c1",
        "-r",
        "map",
        "--set",
        r#"{"position": {"x": 1.0, "y": 0.2, "z": 0.3}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.7071067811865476, "w": 0.7071067811865476}}"#,
    ]));
    assert!(close(&p["pose"]["position"]["y"], 0.2), "{p}");
    let info = client.run(&["transform", "info", "c0", "c1"]);
    let t = json_of(&info);
    assert!(close(&t["transform"]["translation"]["x"], 0.2), "{info}");

    // 一覧
    let list = json_of(&client.run(&["transform", "list"]));
    assert_eq!(list.as_array().unwrap().len(), 2, "{list}");
    let long = client.run(&["transform", "list", "--long"]);
    assert!(
        long.contains("core://core/container/map::static_anchor -> core://core/container/c0::example_container  translation (1, 0, 0)  rotation (0, 0, 0.7071067811865476, 0.7071067811865476)"),
        "{long}"
    );

    // 作る・変える・消す
    let out = client.run(&[
        "transform",
        "create",
        "c1",
        "c2",
        "--translation",
        "0,0,-0.5",
    ]);
    assert!(out.starts_with("created: "), "{out}");
    let out = client.run(&["transform", "update", "c1", "c2", "--rotation", "0,0,1,0"]);
    assert!(out.contains("rotation (0, 0, 1, 0)"), "{out}");
    let tree = client.run(&["anchor", "tree", "map"]);
    let lines: Vec<&str> = tree.lines().collect();
    assert_eq!(lines.len(), 4, "{tree}");
    assert!(lines[0].starts_with("map  position (0, 0, 0)"), "{tree}");
    assert!(lines[1].starts_with("└─ c0  position (1, 0, 0)"), "{tree}");
    assert!(lines[3].starts_with("      └─ c2  "), "{tree}");
    let tid = "transform://core://core/container/c1::example_container|core://core/container/c2::example_container";
    let out = client.run(&["transform", "delete", tid]);
    assert_eq!(out.trim(), format!("deleted: {tid}"));

    // 木の規則を破る Transform・見つからない Transform はエラーを表示する
    let out = client.run(&["transform", "create", "c1", "map"]);
    assert!(out.contains("Error"), "{out}");
    let out = client.run(&["transform", "delete", "c1", "c2"]);
    assert!(
        out.contains("Error") && out.contains("is not found"),
        "{out}"
    );
    let out = client.run(&["transform", "create", "c1", "c2", "--translation", "1,2"]);
    assert!(out.contains("Error"), "{out}");

    // Anchor
    let anchors = client.run(&["anchor", "list"]);
    assert!(anchors.contains("odom::periodic_anchor"), "{anchors}");
    let out = client.run(&["anchor", "update", "map"]);
    assert_eq!(
        out.trim(),
        "updated: core://core/container/map::static_anchor"
    );
    let tree = json_of(&client.run(&["anchor", "tree", "map", "--json"]));
    assert_eq!(
        tree["identifier"],
        "core://core/container/map::static_anchor"
    );

    // container list --long は Anchor の種類と rate を出す
    let table = client.run(&["container", "list", "-f", path_str(&client.conf), "--long"]);
    assert!(table.contains("anchor: periodic (20 Hz)"), "{table}");
    assert!(table.contains("anchor: static"), "{table}");

    assert!(robot.stop() || cfg!(not(unix)), "{}", robot.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}
