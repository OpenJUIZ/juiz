//! juiz_core の feature が違うブローカーのプラグインを読んでも落ちないことのテスト (issue #450)
//!
//! ワークスペースの `juiz_core` は `default-features = false` なので、`cargo build -p juiz_qmp_broker` だけで
//! 作った `libqmp_broker.so` の中の juiz_core は feature なしになる。juiz コマンドは ipc-broker などの
//! 既定の feature 付きなので、トレイトのメソッドの並び (vtable) が feature で変わると、プラグインが作った
//! BrokerProxy をホストが呼んだときに違う番地へ飛んで落ちる (0.2.7 までは `topic_push_shared` がそうだった)。
//!
//! scripts/test.sh が `cargo build -p juiz_qmp_broker --target-dir target/qmp-plugin-alone` で作った
//! プラグインと、../target/debug の increment_process を使う。
mod common;

use common::{juiz, path_str, plugin_dir_str, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const QMP_PORT: u16 = 5041;

/// feature なしの juiz_core で作った qmp のプラグインがあるディレクトリ
fn alone_plugin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/qmp-plugin-alone/debug")
}

fn alone_plugin_dir_str() -> String {
    let dir = alone_plugin_dir();
    {
        use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
        let file = dir.join(format!("{DLL_PREFIX}qmp_broker{DLL_SUFFIX}"));
        assert!(
            file.exists(),
            "{} がありません。scripts/test.sh の準備 (cargo build -p juiz_qmp_broker --target-dir target/qmp-plugin-alone) を先に済ませてください",
            file.display()
        );
    }
    dir.display().to_string().replace('\\', "/")
}

fn write_confs(dir: &Path) -> (PathBuf, PathBuf) {
    let qmp = alone_plugin_dir_str();
    let p = plugin_dir_str();
    let server = dir.join("server.conf");
    std::fs::write(
        &server,
        format!(
            r#"name: broker_plugin_features_server
option:
  http_broker:
    start: false
plugins:
  broker_factories:
    qmp_broker: {{ path: "{qmp}" }}
  process_factories:
    increment_process: {{ path: "{p}" }}
brokers:
  - {{ type_name: qmp, name: "127.0.0.1:{QMP_PORT}", host: 127.0.0.1, port: {QMP_PORT} }}
processes:
  - {{ type_name: increment_process, name: inc0 }}
"#
        ),
    )
    .unwrap();
    let client = dir.join("client.conf");
    std::fs::write(
        &client,
        format!(
            r#"name: broker_plugin_features_client
option:
  http_broker:
    start: false
plugins:
  broker_factories:
    qmp_broker: {{ path: "{qmp}" }}
"#
        ),
    )
    .unwrap();
    (server, client)
}

#[test]
fn broker_plugin_built_without_core_features() {
    let dir = temp_dir("broker_plugin_features");
    let (server_conf, client_conf) = write_confs(&dir);
    let mut server = Daemon::spawn(
        &["-f", path_str(&server_conf), "-d"],
        dir.join("server.log"),
    );
    let host = format!("qmp://127.0.0.1:{QMP_PORT}");
    let inc0 = format!("{host}/process/inc0::increment_process");

    // サーバーが qmp で応答するまで待ってから、プラグインの BrokerProxy 越しに Process を呼ぶ
    let deadline = Instant::now() + Duration::from_secs(60);
    let out = loop {
        let out = juiz()
            .env("RUST_LOG", "info")
            .args([
                "-f",
                path_str(&client_conf),
                "-s",
                &host,
                "process",
                "call",
                &inc0,
                r#"{"arg1": 2}"#,
                "-p",
            ])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() && stdout.contains("Value(3)") {
            break out;
        }
        // 落ちた (シグナルで終わった) ならすぐ失敗にする
        assert!(
            out.status.code().is_some(),
            "juiz crashed: {out:?}\nserver log:\n{}",
            server.log_text()
        );
        assert!(
            Instant::now() < deadline,
            "process call did not succeed: {out:?}\nserver log:\n{}",
            server.log_text()
        );
        std::thread::sleep(Duration::from_millis(500));
    };
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    // 組み込みの qmp ではなく、feature なしで作ったプラグインを読んでいる
    assert!(
        log.contains("BrokerFactory (name=qmp_broker) Loaded"),
        "{log}"
    );
    assert!(
        server.stop() || cfg!(windows),
        "server log:\n{}",
        server.log_text()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
