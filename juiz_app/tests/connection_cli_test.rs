//! 別の juiz にある Process への接続を `connection delete` で消すテスト (issue #479)
//!
//! チュートリアル第 6 章と同じく、listener の juiz と talker の juiz を別々に起動し、
//! talker → listener の push 接続を作ってから、talker 側の juiz に `-s` で delete を送る。
//! HTTP の BrokerProxy は接続 ID の両端を core:// に書き換えて送るので、talker 側には
//! listener も core://core/process/listener0::listener として届く。それを探して
//! ERROR を出さないことを確かめる。
//!
//! プラグイン (talker, listener) は scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{path_str, plugin_dir_str, run, temp_dir, Daemon};
use std::time::{Duration, Instant};

const LISTENER_PORT: u16 = 8086;
const TALKER_PORT: u16 = 8087;

fn host(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn conf(name: &str, port: u16, plugin: &str, process: &str) -> String {
    let p = plugin_dir_str();
    format!(
        r#"name: {name}
option:
  http_broker:
    start: true
    port: {port}
plugins:
  process_factories:
    {plugin}: {{ path: "{p}" }}
processes:
  - {{ type_name: {plugin}, name: {process} }}
"#
    )
}

fn run_ok_with(conf: &str, args: &[&str]) -> String {
    let mut all = vec!["-f", conf];
    all.extend_from_slice(args);
    let out = run(&all);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn wait_for(conf: &str, server: &str, id: &str, daemon: &Daemon) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !run_ok_with(conf, &["-s", server, "process", "list"]).contains(id) {
        if Instant::now() > deadline {
            panic!("{id} did not appear. log:\n{}", daemon.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
fn delete_connection_to_other_system() {
    let dir = temp_dir("connection_cli_test");
    let listener_conf = dir.join("listener.conf");
    let talker_conf = dir.join("talker.conf");
    let client_conf = dir.join("client.conf");
    std::fs::write(
        &listener_conf,
        conf("listener_system", LISTENER_PORT, "listener", "listener0"),
    )
    .unwrap();
    std::fs::write(
        &talker_conf,
        conf("talker_system", TALKER_PORT, "talker", "talker0"),
    )
    .unwrap();
    std::fs::write(&client_conf, "name: client\n").unwrap();
    let client = path_str(&client_conf);

    let listener_host = host(LISTENER_PORT);
    let talker_host = host(TALKER_PORT);
    let listener_daemon = Daemon::spawn(
        &["-f", path_str(&listener_conf), "-d"],
        dir.join("listener.log"),
    );
    let talker_daemon = Daemon::spawn(
        &["-f", path_str(&talker_conf), "-d"],
        dir.join("talker.log"),
    );
    let listener = format!("{listener_host}/process/listener0::listener");
    let talker = format!("{talker_host}/process/talker0::talker");
    wait_for(client, &listener_host, &listener, &listener_daemon);
    wait_for(client, &talker_host, &talker, &talker_daemon);

    let out = run_ok_with(
        client,
        &[
            "connection",
            "create",
            "-t",
            "push",
            &talker,
            "arg1",
            &listener,
        ],
    );
    assert!(!out.contains("Error"), "{out}");
    let list = run_ok_with(client, &["-s", &talker_host, "connection", "list"]);
    let ids: Vec<String> = juiz_sdk::serde_json::from_str(list.trim()).unwrap();
    assert_eq!(ids.len(), 1, "{ids:?}");
    assert!(ids[0].ends_with(&format!("|arg1|{listener}")), "{ids:?}");

    // チュートリアルと同じく、talker 側の juiz に list の ID で delete を送る
    let out = run_ok_with(
        client,
        &["-s", &talker_host, "connection", "delete", &ids[0]],
    );
    assert!(!out.contains("Error"), "{out}");
    let list = run_ok_with(client, &["-s", &talker_host, "connection", "list"]);
    assert_eq!(list.trim(), "[]");

    let log = talker_daemon.log_text();
    assert!(!log.contains("ERROR"), "talker log:\n{log}");
    let log = listener_daemon.log_text();
    assert!(!log.contains("ERROR"), "listener log:\n{log}");
}
