//! CLI が qmp で問い合わせて終わったとき、サーバーが切断を ERROR にしないことのテスト (#276)
//!
//! juiz コマンドで qmp ブローカーを持つサーバーを起動し、juiz コマンドの CLI で
//! `process list` を送る。CLI は終わるときに接続を明示的に閉じるので、サーバーは
//! アイドルタイムアウト (クライアントの既定は 30 秒) を待たずに「閉じられた」と記録する。
//!
//! juiz コマンドは cargo build で ../../target/debug にビルドされている前提 (CI の test.yml と同じ)。

mod common;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::*;

const SERVER_PORT: i64 = 5024;

#[test]
fn cli_disconnect_is_not_logged_as_error() {
    let log = std::env::temp_dir().join(format!(
        "qmp_disconnect_test_server_{}.log",
        std::process::id()
    ));
    let mut server = spawn_server_with_log(SERVER_PORT, 8040, false, Some(&log));
    let juiz = target_dir().join(format!("juiz{}", std::env::consts::EXE_SUFFIX));
    // 系の conf (./juiz.conf) を読まないよう、空の作業ディレクトリで CLI を動かす
    let work = std::env::temp_dir().join(format!("qmp_disconnect_test_{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();

    // サーバーの起動を待つ。起動前は接続がタイムアウトして CLI が失敗する
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            panic!("juiz server exited ({status})");
        }
        let out = Command::new(&juiz)
            .current_dir(&work)
            .arg("-s")
            .arg(format!("qmp://127.0.0.1:{SERVER_PORT}"))
            .arg("process")
            .arg("list")
            .stdin(Stdio::null())
            .output()
            .expect("failed to run juiz");
        let stdout = String::from_utf8_lossy(&out.stdout);
        if out.status.success() && stdout.contains("inc0") {
            break;
        }
        assert!(Instant::now() < deadline, "process list failed: {stdout}");
        std::thread::sleep(Duration::from_millis(200));
    }

    // 接続を閉じた知らせが届けば、アイドルタイムアウトより十分早く記録される
    let deadline = Instant::now() + Duration::from_secs(10);
    let text = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("closed by peer") || Instant::now() > deadline {
            break text;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    drop(server);
    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_dir_all(&work);
    assert!(text.contains("closed by peer"), "server log:\n{text}");
    assert!(!text.contains("accept_bi() error"), "server log:\n{text}");
}
