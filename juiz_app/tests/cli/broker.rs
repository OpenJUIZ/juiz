//! `juiz broker install / list / path / remove` と、入れたブローカーを conf の type_name と
//! `-s <種類>://` だけで使うこと (docs/spec/broker-plugin-abi.md §6.4・§10、issue #638)
//!
//! 入れるのは試験用の C ABI のブローカー tcptest (brokers/tcp_test_broker、ワークスペースの cargo build で
//! target/debug にできる cdylib)。§10.2 の並びのアーカイブにして `--from` で入れる。置き場所は
//! JUIZ_BROKER_HOME で一時ディレクトリにする。GitHub には取りに行かない。
//! 取得元の選び方・照合などの細かい場合は juiz_app/src/broker.rs の単体テストで確かめている。
//!
//! ポートは tcp_test_broker のテスト (5440〜5446) の続きの 5447。

use crate::common::{juiz, path_str, plugin_dir_str, plugin_file, temp_dir, Daemon};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const PORT: u16 = 5447;

/// target/debug の tcp_test_broker を、§10.2 のアーカイブ (tar.gz、中身は版のディレクトリの下と同じ並び) にする
fn make_archive(dir: &Path, version: &str, target: &str) -> PathBuf {
    let lib_src = PathBuf::from(plugin_file("tcp_test_broker"));
    assert!(
        lib_src.is_file(),
        "{} is not built. Run 'cargo build' in the workspace first",
        lib_src.display()
    );
    let lib_name = lib_src.file_name().unwrap().to_str().unwrap().to_owned();
    let lib = std::fs::read(&lib_src).unwrap();
    let json = format!(
        r#"{{
  "format": 1,
  "type_name": "tcptest",
  "plugin_name": "juiz_tcp_test_broker",
  "version": "{version}",
  "abi_version": 1,
  "target": "{target}",
  "library": "lib/{lib_name}"
}}
"#
    );
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(gz);
    for (p, c, mode) in [
        ("juiz-broker.json".to_owned(), json.as_bytes(), 0o644),
        (format!("lib/{lib_name}"), lib.as_slice(), 0o755),
    ] {
        let mut h = tar::Header::new_gnu();
        h.set_size(c.len() as u64);
        h.set_mode(mode);
        h.set_cksum();
        b.append_data(&mut h, p, c).unwrap();
    }
    let data = b.into_inner().unwrap().finish().unwrap();
    let file = format!("juiz_broker-tcptest-{version}-abi1-{target}.tar.gz");
    let path = dir.join(&file);
    std::fs::write(&path, &data).unwrap();
    let sum: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(dir.join(format!("{file}.sha256")), format!("{sum}  {file}\n")).unwrap();
    path
}

fn text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn install_and_use_a_broker_plugin() {
    let dir = temp_dir("broker_install_cli");
    let home = dir.join("brokers");
    // juiz_app のテストと juiz は同じターゲットでビルドされる
    let target = juiz_core::HOST_TARGET;
    let archive = make_archive(&dir, "0.1.0", target);
    let run = |args: &[&str]| {
        juiz()
            .env("JUIZ_BROKER_HOME", &home)
            .env("RUST_BACKTRACE", "0")
            .args(args)
            .output()
            .unwrap()
    };

    // 入れる前は案内が出る
    let out = run(&["broker", "path", "tcptest"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("juiz broker install tcptest"), "{}", text(&out));

    let out = run(&["broker", "install", "tcptest", "--from", path_str(&archive)]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let installed = home.join("tcptest").join("0.1.0");
    assert!(
        t.contains("installed tcptest broker 0.1.0 (juiz_tcp_test_broker, broker ABI 1) to "),
        "{t}"
    );
    assert!(t.contains("Use it with 'type_name: tcptest' in the conf"), "{t}");
    // 同じ版はもう一度入れない
    let out = run(&["broker", "install", "tcptest", "--from", path_str(&archive)]);
    assert!(out.status.success());
    assert!(text(&out).contains("already installed"), "{}", text(&out));

    let out = run(&["broker", "path", "tcptest"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        std::fs::canonicalize(String::from_utf8_lossy(&out.stdout).trim()).unwrap(),
        std::fs::canonicalize(&installed).unwrap()
    );
    let out = run(&["broker", "list"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.lines().any(|l| l.starts_with("http") && l.contains("builtin")), "{t}");
    assert!(
        t.lines().any(|l| l.starts_with("tcptest") && l.contains("0.1.0") && l.contains("used")),
        "{t}"
    );

    // conf に type_name: tcptest を書くだけで、置き場所から読んで立てる (plugins.broker_factories は書かない)
    let p = plugin_dir_str();
    let conf = dir.join("server.conf");
    std::fs::write(
        &conf,
        format!(
            r#"name: broker_install_server
option:
  http_broker:
    start: false
plugins:
  process_factories:
    increment_process: {{ path: "{p}" }}
brokers:
  - {{ type_name: tcptest, host: 127.0.0.1, port: {PORT} }}
processes:
  - {{ type_name: increment_process, name: inc0 }}
"#
        ),
    )
    .unwrap();
    let mut cmd = juiz();
    cmd.env("JUIZ_BROKER_HOME", &home)
        .env("RUST_LOG", "info")
        .args(["-f", path_str(&conf), "-d"]);
    let mut server = Daemon::spawn_command(cmd, dir.join("server.log"));
    // クライアントも conf なしで、-s tcptest://… の頭だけで置き場所から読む
    let host = format!("tcptest://127.0.0.1:{PORT}");
    let inc0 = format!("{host}/process/inc0::increment_process");
    let deadline = Instant::now() + Duration::from_secs(60);
    let out = loop {
        let out = run(&["-s", &host, "process", "call", &inc0, r#"{"arg1": 2}"#, "-p"]);
        if out.status.success() && String::from_utf8_lossy(&out.stdout).contains("Value(3)") {
            break out;
        }
        assert!(
            Instant::now() < deadline,
            "process call did not succeed: {}\nserver log:\n{}",
            text(&out),
            server.log_text()
        );
        std::thread::sleep(Duration::from_millis(500));
    };
    let _ = out;
    let log = server.log_text();
    assert!(
        log.contains("BrokerFactory(type_name=tcptest) (installed juiz_tcp_test_broker 0.1.0, C ABI) Loaded"),
        "{log}"
    );
    assert!(server.stop() || cfg!(windows), "server log:\n{}", server.log_text());

    // remove
    let out = run(&["broker", "remove", "tcptest"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("removed "), "{}", text(&out));
    assert!(!home.join("tcptest").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
