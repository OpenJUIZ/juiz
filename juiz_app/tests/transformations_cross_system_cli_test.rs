//! System をまたぐ Transform (issue #415、仕様 docs/spec/transformations.md §19) を、juiz を別プロセスで
//! いくつも起動して確かめる。
//!
//! - 例 A (HTTP): S1 に `map`、S2 に `odom → base0`。S2 の conf の `map → odom` の始点は S1 の `map`
//!   (`http://…/container/map::static_anchor`)。S2 で `base0` を `map` から読み、S1 からも S2 の `base0` を読む。
//! - 例 A (QUIC): S3 の conf の始点を qmp の ID にして同じ確認。
//! - 例 B: S2 と S3 がどちらも `map` の写しを持つので、それぞれの `map` から見た位置を合成できる。
//! - 親側からの作成: S2 は S1 をマスターシステムにしている。S1 で終点を S2 の Container にして作ると S2 にでき、
//!   S1 から ID で読む・消すこともできる (§19.2 の 5・7)。
//! - 親を止めると、S2 の写しは消え (`remote: lost`、timeout の後か、親が止まるときに S2 から外れたとき)、
//!   `map` から読むとその理由が出る。
//! - conf で回す: S5 (親) の conf の `transforms:` の終点を S4 (サブシステム) の Container にすると、S4 にできる (§19.2 の 8)。
//!
//! 数値は §3.5 の例 (map → odom は (1, 0, 0) と z 軸まわり 90°、odom → base0 は (0.1, 0, 0.3)、
//! map から見た base0 は (1, 0.1, 0.3))。
//! プラグイン (example_container) は scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{path_str, plugin_dir_str, run, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const S1_HTTP: u16 = 8091;
const S1_QMP: u16 = 5031;
const S2_HTTP: u16 = 8092;
const S3_HTTP: u16 = 8093;
const S4_HTTP: u16 = 8094;
const S5_HTTP: u16 = 8095;

const Z90: &str = "{x: 0.0, y: 0.0, z: 0.7071067811865476, w: 0.7071067811865476}";

fn host(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn s1_map_http() -> String {
    format!("http://127.0.0.1:{S1_HTTP}/container/map::static_anchor")
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// 地図の System (Anchor `map` だけ)。http と qmp で受ける
fn s1_conf() -> String {
    format!(
        r#"name: s1
option:
  http_broker:
    start: true
    port: {S1_HTTP}
brokers:
  - type_name: qmp
    name: qmp0
    port: {S1_QMP}
containers:
  - {{ type_name: static_anchor, name: map }}
"#
    )
}

/// ロボットの System。始点 `source` (別 System の map) → odom → base0。`extra` は conf の末尾に足す
fn robot_conf(name: &str, port: u16, source: &str, extra: &str) -> String {
    let p = plugin_dir_str();
    format!(
        r#"name: {name}
option:
  http_broker:
    start: true
    port: {port}
plugins:
  container_factories:
    example_container:
      path: "{p}"
containers:
  - {{ type_name: example_container, name: odom }}
  - {{ type_name: example_container, name: base0 }}
  - {{ type_name: example_container, name: c2 }}
transforms:
  - source: {source}
    destination: odom
    translation: {{x: 1.0, y: 0.0, z: 0.0}}
    rotation: {Z90}
    rate: 20.0
    timeout: 2.0
  - source: odom
    destination: base0
    translation: {{x: 0.1, y: 0.0, z: 0.3}}
{extra}"#
    )
}

struct Client {
    conf: PathBuf,
}

impl Client {
    fn output(&self, port: u16, args: &[&str]) -> String {
        let server = host(port);
        let mut all = vec!["-f", path_str(&self.conf), "-s", &server];
        all.extend_from_slice(args);
        let out = run(&all);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    /// 出力が `ok` を満たすまで (最長 30 秒) 繰り返す
    fn wait_for(&self, port: u16, args: &[&str], ok: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        let server = host(port);
        let mut all = vec!["-f", path_str(&self.conf), "-s", &server];
        all.extend_from_slice(args);
        loop {
            // 起動を待つ間は失敗してもよい
            let out = String::from_utf8_lossy(&run(&all).stdout).into_owned();
            if ok(&out) {
                return out;
            }
            if Instant::now() > deadline {
                panic!("{args:?} on {port} did not get the expected output. last output:\n{out}");
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }
}

fn json_of(out: &str) -> juiz_sdk::serde_json::Value {
    juiz_sdk::serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("not a JSON ({e}): {out}"))
}

fn close(v: &juiz_sdk::serde_json::Value, expected: f64) -> bool {
    v.as_f64().is_some_and(|x| (x - expected).abs() < 1e-9)
}

fn position_is(p: &juiz_sdk::serde_json::Value, x: f64, y: f64, z: f64) -> bool {
    let pos = &p["pose"]["position"];
    close(&pos["x"], x) && close(&pos["y"], y) && close(&pos["z"], z)
}

fn start(conf: &Path, log: PathBuf) -> Daemon {
    Daemon::spawn(&["-f", path_str(conf), "-d"], log)
}

#[test]
fn transforms_across_processes() {
    let dir = temp_dir("transformations_cross_system_cli_test");
    let client = Client {
        conf: write(&dir, "client.conf", "name: client\n"),
    };
    let s1_conf = write(&dir, "s1.conf", &s1_conf());
    let mut s1 = start(&s1_conf, dir.join("s1.log"));
    client.wait_for(S1_HTTP, &["anchor", "list"], |o| o.contains("map::static_anchor"));

    // S2 は http で、S3 は qmp で S1 の map を取り込む。S2 は S1 をマスターシステムにする (親側からの作成に使う)
    let s2_conf = write(
        &dir,
        "s2.conf",
        &robot_conf(
            "s2",
            S2_HTTP,
            &s1_map_http(),
            &format!("mastersystem:\n  broker_type: http\n  broker_name: \"127.0.0.1:{S1_HTTP}\"\n"),
        ),
    );
    let s3_conf = write(
        &dir,
        "s3.conf",
        &robot_conf(
            "s3",
            S3_HTTP,
            &format!("qmp://127.0.0.1:{S1_QMP}/container/map::static_anchor"),
            "",
        ),
    );
    let mut s2 = start(&s2_conf, dir.join("s2.log"));
    let mut s3 = start(&s3_conf, dir.join("s3.log"));

    // 例 A: 子の System で、親の map から見た base0 を読む (名前 map は写しに解決する、§19.5)
    let read_base0 = ["container", "pose", "base0", "--reference", "map"];
    for (port, prefix) in [(S2_HTTP, "http"), (S3_HTTP, "qmp")] {
        let out = client.wait_for(port, &read_base0, |o| o.trim_start().starts_with('{'));
        let p = json_of(&out);
        assert!(position_is(&p, 1.0, 0.1, 0.3), "{prefix}: {p}");
        let frame = p["header"]["frame_id"].as_str().unwrap();
        assert!(
            frame.starts_with(&format!("{prefix}://127.0.0.1:"))
                && frame.ends_with("/container/map::static_anchor"),
            "{prefix}: {p}"
        );
        let long = client.output(port, &["transform", "list", "--long"]);
        assert!(long.contains("/container/map::static_anchor -> core://core/container/odom::example_container"), "{long}");
        assert!(long.contains("(remote: ok)"), "{long}");
    }
    // 例 B: S2 と S3 はどちらも map の写しを持つ。S3 の base0 を S2 の base0 から見ると、同じ値なので原点
    let b2 = json_of(&client.output(S2_HTTP, &read_base0));
    let b3 = json_of(&client.output(S3_HTTP, &read_base0));
    assert_eq!(b2["pose"], b3["pose"]);

    // PoseState (§19.4)
    let state = json_of(&client.output(S1_HTTP, &["container", "pose-state", "map"]));
    assert_eq!(state["identifier"], "core://core/container/map::static_anchor", "{state}");
    assert_eq!(state["changed"], true, "{state}");
    assert_eq!(state["entries"].as_array().unwrap().len(), 1, "{state}");

    // 親の System では、S2 と S3 が取りに来ている (remote_children、§19.10)
    let table = client.wait_for(
        S1_HTTP,
        &["container", "list", "-f", path_str(&client.conf), "--long"],
        |o| o.matches("remote child: ").count() == 2,
    );
    assert!(table.contains("core://core/container/odom::example_container"), "{table}");

    // S1 から S2 の base0 を map から見て読む (S2 に回す、§19.8)
    let s2_base0 = format!("{}/container/base0::example_container", host(S2_HTTP));
    let p = json_of(&client.output(S1_HTTP, &["container", "pose", &s2_base0, "-r", "map"]));
    assert!(position_is(&p, 1.0, 0.1, 0.3), "{p}");
    assert_eq!(p["header"]["frame_id"], "map", "{p}");
    // 逆に、S2 の base0 から見た S1 の map (R の System に頼んで逆にする)
    let p = json_of(&client.output(S1_HTTP, &["container", "pose", "map", "-r", &s2_base0]));
    assert!(position_is(&p, -0.1, 1.0, -0.3), "{p}");
    // S1 から S2 の base0 を書く (S2 の odom → base0 が変わる)
    let p = json_of(&client.output(
        S1_HTTP,
        &[
            "container",
            "pose",
            &s2_base0,
            "-r",
            "map",
            "--set",
            r#"{"position": {"x": 1.0, "y": 0.2, "z": 0.3}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.7071067811865476, "w": 0.7071067811865476}}"#,
        ],
    ));
    assert!(position_is(&p, 1.0, 0.2, 0.3), "{p}");
    let t = json_of(&client.output(S2_HTTP, &["transform", "info", "odom", "base0"]));
    assert!(close(&t["transform"]["translation"]["x"], 0.2), "{t}");

    // 親側から作る: 終点が S2 の Container なら S2 にできる (§19.2 の 5)
    let s2_c2 = format!("{}/container/c2::example_container", host(S2_HTTP));
    let out = client.output(
        S1_HTTP,
        &["transform", "create", "map", &s2_c2, "--translation", "0,0,1", "--rate", "5", "--timeout", "2"],
    );
    let created = format!(
        "created: {} -> core://core/container/c2::example_container  translation (0, 0, 1)",
        s1_map_http()
    );
    assert!(out.starts_with(&created), "{out}");
    assert!(client.output(S1_HTTP, &["transform", "list"]).trim() == "[]");
    let p = json_of(&client.wait_for(S2_HTTP, &["container", "pose", "c2", "-r", "map"], |o| {
        o.trim_start().starts_with('{')
    }));
    assert!(position_is(&p, 0.0, 0.0, 1.0), "{p}");
    // S1 から見た ID で読む・変える・消す (§19.2 の 7)
    let s1_view = format!("transform://core://core/container/map::static_anchor|{s2_c2}");
    let t = json_of(&client.output(S1_HTTP, &["transform", "info", &s1_view]));
    assert_eq!(t["remote"]["rate"], 5.0, "{t}");
    let out = client.output(S1_HTTP, &["transform", "update", &s1_view, "--translation", "0,0,2"]);
    assert!(out.contains("translation (0, 0, 2)"), "{out}");
    let out = client.output(S1_HTTP, &["transform", "delete", &s1_view]);
    assert!(out.starts_with("deleted: "), "{out}");
    let list = client.output(S2_HTTP, &["transform", "list"]);
    assert!(!list.contains("c2::example_container"), "{list}");
    // 両端とも別 System は作れない
    let out = client.output(S1_HTTP, &["transform", "create", &s2_base0, &s2_c2]);
    assert!(out.contains("Error") && out.contains("Cross-System Transform error"), "{out}");

    // 親を止めると、timeout (2 秒) の後に写しが消える (§19.6)。Transform は残る
    assert!(s1.stop() || cfg!(not(unix)), "{}", s1.log_text());
    client.wait_for(S2_HTTP, &["transform", "list", "--long"], |o| o.contains("(remote: lost)"));
    let out = client.output(S2_HTTP, &read_base0);
    // 止まるときに S1 が S2 に「外して」と頼めば remove_subsystem の理由になる (§19.6)
    assert!(
        out.contains("Error") && (out.contains("not reachable") || out.contains("was removed")),
        "{out}"
    );

    assert!(s2.stop() || cfg!(not(unix)), "{}", s2.log_text());
    assert!(s3.stop() || cfg!(not(unix)), "{}", s3.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn conf_transforms_are_forwarded_to_subsystem() {
    let dir = temp_dir("transformations_cross_system_conf_test");
    let client = Client {
        conf: write(&dir, "client.conf", "name: client\n"),
    };
    let p = plugin_dir_str();
    // 子 (サブシステム) を先に起動する
    let s4_conf = write(
        &dir,
        "s4.conf",
        &format!(
            r#"name: s4
option:
  http_broker:
    start: true
    port: {S4_HTTP}
plugins:
  container_factories:
    example_container:
      path: "{p}"
containers:
  - {{ type_name: example_container, name: odom }}
"#
        ),
    );
    let mut s4 = start(&s4_conf, dir.join("s4.log"));
    client.wait_for(S4_HTTP, &["container", "list"], |o| o.contains("odom"));
    // 親の conf の transforms: の終点を子の Container にする
    let s5_conf = write(
        &dir,
        "s5.conf",
        &format!(
            r#"name: s5
option:
  http_broker:
    start: true
    port: {S5_HTTP}
containers:
  - {{ type_name: static_anchor, name: map }}
subsystems:
  - type_name: http
    name: "127.0.0.1:{S4_HTTP}"
transforms:
  - source: map
    destination: {}/container/odom::example_container
    translation: {{x: 1.0, y: 0.0, z: 0.0}}
    rotation: {Z90}
"#,
            host(S4_HTTP)
        ),
    );
    let mut s5 = start(&s5_conf, dir.join("s5.log"));
    let long = client.wait_for(S4_HTTP, &["transform", "list", "--long"], |o| {
        o.contains("(remote: ok)")
    });
    assert!(
        long.contains(&format!(
            "/container/map::static_anchor -> core://core/container/odom::example_container  translation (1, 0, 0)"
        )),
        "{long}"
    );
    let p = json_of(&client.output(S4_HTTP, &["container", "pose", "odom", "-r", "map"]));
    assert!(position_is(&p, 1.0, 0.0, 0.0), "{p}");
    // 親には何も残らない
    assert_eq!(client.output(S5_HTTP, &["transform", "list"]).trim(), "[]");

    assert!(s5.stop() || cfg!(not(unix)), "{}", s5.log_text());
    assert!(s4.stop() || cfg!(not(unix)), "{}", s4.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}
