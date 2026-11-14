//! Container の 3D モデルの CLI (issue #382、仕様 docs/spec/container-model.md §9) のテスト
//!
//! conf にモデルを持つ Container を書いた juiz を `-d --ipc` で起動し、HTTP (生のバイト列の経路) と
//! IPC (JSON の経路) の両方で `container model`・`container list --long` を叩く。
//! `model check` は System を起動せずに動くことも確かめる。
//!
//! プラグイン (example_container) は scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{path_str, plugin_dir_str, run, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PORT: u16 = 8089;
const IPC: &str = "model_cli_test";

fn host() -> String {
    format!("http://127.0.0.1:{PORT}")
}

/// glTF の「上」1 m の箱だけを持つ .glb (JSON チャンクだけ)
fn glb() -> Vec<u8> {
    let json = br#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"accessors":[{"min":[0,0,0],"max":[0.5,1,0.25]}]}"#;
    let mut j = json.to_vec();
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((20 + j.len()) as u32).to_le_bytes());
    v.extend_from_slice(&(j.len() as u32).to_le_bytes());
    v.extend_from_slice(b"JSON");
    v.extend(j);
    v
}

fn write_robot_conf(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir.join("models")).unwrap();
    std::fs::write(dir.join("models/pantilt.glb"), glb()).unwrap();
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
  - type_name: example_container
    name: pantilt0
    model:
      path: models/pantilt.glb
  - {{ type_name: example_container, name: plain0 }}
"#
        ),
    )
    .unwrap();
    robot
}

fn json_of(out: &str) -> juiz_sdk::serde_json::Value {
    juiz_sdk::serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("not a JSON ({e}): {out}"))
}

#[test]
fn container_model_subcommands() {
    let dir = temp_dir("model_cli_test");
    let robot_conf = write_robot_conf(&dir);
    let client_conf = dir.join("client.conf");
    std::fs::write(&client_conf, "name: client\n").unwrap();
    let mut robot = Daemon::spawn(
        &["-f", path_str(&robot_conf), "-d", "--ipc", IPC],
        dir.join("robot.log"),
    );
    let client = |server: &str, args: &[&str]| -> String {
        let mut all = vec!["-f", path_str(&client_conf), "-s", server];
        all.extend_from_slice(args);
        let out = run(&all);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let out = run(&[
            "-f",
            path_str(&client_conf),
            "-s",
            &host(),
            "container",
            "list",
        ]);
        if String::from_utf8_lossy(&out.stdout).contains("pantilt0::example_container") {
            break;
        }
        if Instant::now() > deadline {
            panic!("juiz did not start. log:\n{}", robot.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let glb = glb();
    let sha = juiz_core::models::sha256_hex(&glb);
    let ipc = format!("ipc://{IPC}");

    for server in [host(), ipc.clone()] {
        // -o が無ければモデル情報 (§9.1)
        let info = json_of(&client(&server, &["container", "model", "pantilt0"]));
        assert_eq!(info["format"], "glb", "{server}: {info}");
        assert_eq!(info["size"], glb.len() as u64);
        assert_eq!(info["sha256"], sha.as_str());
        assert_eq!(info["file_name"], "pantilt.glb");
        // モデルの無い Container は null
        let none = client(&server, &["container", "model", "plain0", "--info"]);
        assert_eq!(none.trim(), "null", "{server}");

        // -o がディレクトリなら <file_name> で書く
        let out_dir = dir.join(format!(
            "out_{}",
            if server.starts_with("ipc") {
                "ipc"
            } else {
                "http"
            }
        ));
        std::fs::create_dir_all(&out_dir).unwrap();
        let out = client(
            &server,
            &["container", "model", "pantilt0", "-o", path_str(&out_dir)],
        );
        assert!(
            out.contains("wrote: ") && out.contains("pantilt.glb"),
            "{server}: {out}"
        );
        assert_eq!(std::fs::read(out_dir.join("pantilt.glb")).unwrap(), glb);
        // -o と --info なら両方
        let file = out_dir.join("copy.glb");
        let out = client(
            &server,
            &[
                "container",
                "model",
                "pantilt0",
                "--info",
                "-o",
                path_str(&file),
            ],
        );
        assert!(
            out.contains("\"sha256\"") && out.contains("wrote: "),
            "{server}: {out}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), glb);
        // モデルが無ければエラーを表示し、ファイルは作らない
        let missing = out_dir.join("plain.glb");
        let out = client(
            &server,
            &["container", "model", "plain0", "-o", path_str(&missing)],
        );
        assert!(out.contains("Error"), "{server}: {out}");
        assert!(!missing.exists());

        // list --long の Container の行の下に形式と大きさ (§7)
        let table = client(
            &server,
            &["container", "list", "-f", path_str(&client_conf), "--long"],
        );
        assert!(
            table.contains(&format!("model: glb {} B (pantilt.glb)", glb.len())),
            "{server}: {table}"
        );
        assert_eq!(table.matches("model: ").count(), 1, "{server}: {table}");
    }

    assert!(robot.stop() || cfg!(not(unix)), "{}", robot.log_text());

    // model check は System を起動しない (conf も読まない)
    let out = run(&["model", "check", path_str(&dir.join("models/pantilt.glb"))]);
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains(": glb (model/gltf-binary)"), "{s}");
    assert!(s.contains("size: 0.5 x 0.25 x 1 m"), "{s}");
    assert!(s.trim_end().ends_with("OK"), "{s}");
    // 警告だけなら 0、エラーなら 1
    let stl = dir.join("big.stl");
    let mut v = vec![0u8; 80];
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&[0u8; 12]);
    for p in [[0.0f32, 0.0, 0.0], [200.0, 0.0, 0.0], [0.0, 50.0, 10.0]] {
        for x in p {
            v.extend_from_slice(&x.to_le_bytes());
        }
    }
    v.extend_from_slice(&[0u8; 2]);
    std::fs::write(&stl, v).unwrap();
    let out = run(&["model", "check", path_str(&stl)]);
    assert!(out.status.success(), "{out:?}");
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("warning: ") && s.contains("scale: 0.001"), "{s}");
    let out = run(&["model", "check", path_str(&stl), "--scale", "0.001"]);
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .trim_end()
            .ends_with("OK"),
        "{out:?}"
    );
    let bad = dir.join("bad.glb");
    std::fs::write(&bad, b"not a glb").unwrap();
    let out = run(&["model", "check", path_str(&bad)]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).starts_with("error: "),
        "{out:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

const MAP_PORT: u16 = 8098;
const ROBOT_PORT: u16 = 8099;

/// ロボットの System から、名前 `map` で地図の System の map (写し、transformations.md §19.5) のモデルを引ける
#[test]
fn container_model_of_replica_by_name() {
    let dir = temp_dir("model_cli_replica_test");
    std::fs::create_dir_all(dir.join("models")).unwrap();
    std::fs::write(dir.join("models/floor.glb"), glb()).unwrap();
    let map_conf = dir.join("map.conf");
    std::fs::write(
        &map_conf,
        format!(
            r#"name: map_system
option:
  http_broker:
    start: true
    port: {MAP_PORT}
containers:
  - type_name: static_anchor
    name: map
    model:
      path: models/floor.glb
"#
        ),
    )
    .unwrap();
    let p = plugin_dir_str();
    let robot_conf = dir.join("robot.conf");
    std::fs::write(
        &robot_conf,
        format!(
            r#"name: robot_system
option:
  http_broker:
    start: true
    port: {ROBOT_PORT}
plugins:
  container_factories:
    example_container:
      path: "{p}"
containers:
  - {{ type_name: example_container, name: base0 }}
transforms:
  - source: http://127.0.0.1:{MAP_PORT}/container/map::static_anchor
    destination: base0
    translation: {{x: 1.0, y: 0.0, z: 0.0}}
    rate: 20.0
"#
        ),
    )
    .unwrap();
    let client_conf = dir.join("client.conf");
    std::fs::write(&client_conf, "name: client\n").unwrap();
    let mut map = Daemon::spawn(&["-f", path_str(&map_conf), "-d"], dir.join("map.log"));
    let mut robot = Daemon::spawn(&["-f", path_str(&robot_conf), "-d"], dir.join("robot.log"));
    let robot_host = format!("http://127.0.0.1:{ROBOT_PORT}");
    let client = |args: &[&str]| {
        let mut all = vec!["-f", path_str(&client_conf), "-s", &robot_host];
        all.extend_from_slice(args);
        run(&all)
    };
    // 写しができるまで待つ (pose の基準に名前 map が使えるようになるまで)
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let out = client(&["container", "pose", "base0", "--reference", "map"]);
        if String::from_utf8_lossy(&out.stdout)
            .trim_start()
            .starts_with('{')
        {
            break;
        }
        if Instant::now() > deadline {
            panic!(
                "replica of map was not made. robot log:\n{}\nmap log:\n{}",
                robot.log_text(),
                map.log_text()
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let glb = glb();
    let out = client(&["container", "model", "map", "--info"]);
    assert!(out.status.success(), "{out:?}");
    let info = json_of(&String::from_utf8(out.stdout).unwrap());
    assert_eq!(
        info["sha256"],
        juiz_core::models::sha256_hex(&glb).as_str(),
        "{info}"
    );
    assert_eq!(info["file_name"], "floor.glb");
    let file = dir.join("floor_copy.glb");
    let out = client(&["container", "model", "map", "-o", path_str(&file)]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(std::fs::read(&file).unwrap(), glb);

    assert!(robot.stop() || cfg!(not(unix)), "{}", robot.log_text());
    assert!(map.stop() || cfg!(not(unix)), "{}", map.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}
