//! juiz_app の feature `zenoh-broker` のテスト (docs/spec/zenoh-broker.md §8.2、issue #491)
//!
//! - feature あり: プラグインを読まずに、conf の `type_name: zenoh` と `-s zenoh://...` だけで使えること。
//!   Zenoh はビルドが重いので、CI では Zenoh だけの別ジョブ (test-linux.yml の zenoh) で
//!   `cargo test -p juiz_app --features zenoh-broker --test zenoh_cli_test` として走らせる。
//! - feature なし: Zenoh のブローカーを使おうとしたときのエラーに、feature とプラグインの案内が付くこと。
//!   こちらは既定の feature のテスト (scripts/test.sh) で走る。
//!
//! ポートは zenoh_broker のテスト (5212〜5250) の続きの 5251〜5252。
mod common;

use common::{juiz, path_str, temp_dir};

#[cfg(feature = "zenoh-broker")]
use common::{plugin_dir_str, Daemon};
#[cfg(feature = "zenoh-broker")]
use std::time::{Duration, Instant};

#[cfg(feature = "zenoh-broker")]
const BUILTIN_PORT: u16 = 5251;
#[cfg(not(feature = "zenoh-broker"))]
const MISSING_PORT: u16 = 5252;

#[cfg(feature = "zenoh-broker")]
#[test]
fn zenoh_broker_builtin() {
    let dir = temp_dir("zenoh_builtin_cli");
    let p = plugin_dir_str();
    let server_conf = dir.join("server.conf");
    // plugins.broker_factories に zenoh_broker を書かない
    std::fs::write(
        &server_conf,
        format!(
            r#"name: zenoh_builtin_server
option:
  http_broker:
    start: false
plugins:
  process_factories:
    increment_process: {{ path: "{p}" }}
brokers:
  - {{ type_name: zenoh, host: 127.0.0.1, port: {BUILTIN_PORT} }}
processes:
  - {{ type_name: increment_process, name: inc0 }}
"#
        ),
    )
    .unwrap();
    let mut server = Daemon::spawn(
        &["-f", path_str(&server_conf), "-d"],
        dir.join("server.log"),
    );
    let host = format!("zenoh://127.0.0.1:{BUILTIN_PORT}");
    let inc0 = format!("{host}/process/inc0::increment_process");

    // クライアント側も conf なしで、組み込みの Zenoh の BrokerProxy で呼ぶ
    let deadline = Instant::now() + Duration::from_secs(60);
    let out = loop {
        let out = juiz()
            .env("RUST_LOG", "info")
            .args(["-s", &host, "process", "call", &inc0, r#"{"arg1": 2}"#, "-p"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() && stdout.contains("Value(3)") {
            break out;
        }
        assert!(
            Instant::now() < deadline,
            "process call did not succeed: {out:?}\nserver log:\n{}",
            server.log_text()
        );
        std::thread::sleep(Duration::from_millis(500));
    };
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        log.contains("BrokerFactory(type_name=zenoh) (builtin) Loaded"),
        "{log}"
    );
    assert!(
        server.stop() || cfg!(windows),
        "server log:\n{}",
        server.log_text()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(feature = "zenoh-broker"))]
#[test]
fn zenoh_broker_missing_shows_hint() {
    let dir = temp_dir("zenoh_missing_cli");
    let conf = dir.join("server.conf");
    std::fs::write(
        &conf,
        format!(
            r#"name: zenoh_missing
option:
  http_broker:
    start: false
brokers:
  - {{ type_name: zenoh, host: 127.0.0.1, port: {MISSING_PORT} }}
"#
        ),
    )
    .unwrap();
    // ブローカーを作れずに止まる。そのときの表示に案内が出る。止まらなければ 60 秒で失敗にする
    let log = dir.join("server.log");
    let log_file = std::fs::File::create(&log).unwrap();
    let mut child = juiz()
        .args(["-f", path_str(&conf), "-d"])
        .stdout(log_file.try_clone().unwrap())
        .stderr(log_file)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "juiz did not stop:\n{}",
                std::fs::read_to_string(&log).unwrap_or_default()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!status.success(), "{text}");
    assert!(text.contains("--features zenoh-broker"), "{text}");
    assert!(
        text.contains("plugins.broker_factories.zenoh_broker"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(feature = "zenoh-broker"))]
#[test]
fn zenoh_server_option_missing_shows_hint() {
    // -s zenoh://... でも BrokerProxy を作るところで同じ案内が出る (相手は居なくてよい)
    let host = format!("zenoh://127.0.0.1:{MISSING_PORT}");
    let out = juiz()
        .env("RUST_BACKTRACE", "0")
        .args(["-s", &host, "process", "list"])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // 終了コードは今は 0 のまま (process list がエラーを表示して終わる) なので見ない
    assert!(text.contains("--features zenoh-broker"), "{text}");
}
