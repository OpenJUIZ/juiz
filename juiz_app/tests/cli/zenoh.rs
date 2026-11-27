//! Zenoh のブローカーが入っていないときの案内のテスト (docs/spec/broker-plugin-abi.md §6.4)
//!
//! Zenoh は juiz の外 (OpenJUIZ/juiz_zenoh_broker) に移り、juiz には組み込まない (#640)。
//! 置き場所 (JUIZ_BROKER_HOME) が空の juiz で Zenoh を使おうとすると、
//! `juiz broker install zenoh` の案内が出ることを確かめる。
//! Zenoh そのもののテストは juiz_zenoh_broker のリポジトリにある。
//!
//! ポートは 5252 (以前の zenoh_broker のテストの続きの番号)。相手は居なくてよい。

use crate::common::{juiz, path_str, temp_dir};

const MISSING_PORT: u16 = 5252;

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
        .env("JUIZ_BROKER_HOME", dir.join("no_brokers"))
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
    // 置き場所 (JUIZ_BROKER_HOME) は空なので、juiz broker install を案内する (broker-plugin-abi.md §6.4)
    assert!(text.contains("juiz broker install zenoh"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zenoh_server_option_missing_shows_hint() {
    // -s zenoh://... でも BrokerProxy を作るところで同じ案内が出る (相手は居なくてよい)
    let host = format!("zenoh://127.0.0.1:{MISSING_PORT}");
    let home = std::env::temp_dir().join(format!("juiz_zenoh_no_brokers_{}", std::process::id()));
    let out = juiz()
        .env("RUST_BACKTRACE", "0")
        .env("JUIZ_BROKER_HOME", &home)
        .args(["-s", &host, "process", "list"])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // 終了コードは今は 0 のまま (process list がエラーを表示して終わる) なので見ない
    assert!(text.contains("juiz broker install zenoh"), "{text}");
}
