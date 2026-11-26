//! 知っている juiz に聞く (peers、docs/spec/naming.md §12、issue #620)
//!
//! 同じ機械の中でも、登録ファイルの置き場所 (JUIZ_IPC_DIR) を分ければ「別の機械」と同じ経路を通る。
//!
//! - `juiz name resolve --peer <URL>`・`JUIZ_NAMING_PEERS`・CLI の `-s <名前>` で、peer の機械の登録を引ける
//! - `juiz name list --peer <URL> --json` には `found: "peer"` が付く
//! - 答えない peer は警告にして、ほかの所で探し続ける
//! - conf の `option.naming.peers` を書いた System は、`subsystems` の名前も API `naming/resolve` も peer で引く
//! - peer が別の機械に http でつないでいる System も、peer に聞いて引ける (`naming/list?connected=true`)
//!
//! ポートは 8180〜8183。
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use super::common::{juiz, path_str, temp_dir, Daemon};
use juiz_core::prelude::*;

const PEER_PORT: u16 = 8180;
const FAR_PORT: u16 = 8181;
const ASKER_PORT: u16 = 8182;

fn ipc(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::create_dir_all(&p).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    p
}

fn spawn(dir: &Path, ipc_dir: &Path, conf: &Path, log: &str) -> Daemon {
    let mut cmd: Command = juiz();
    cmd.current_dir(dir)
        .env("JUIZ_IPC_DIR", ipc_dir)
        .env_remove("JUIZ_NAMING_PEERS")
        .args(["-f", path_str(conf), "-d"]);
    Daemon::spawn_command(cmd, dir.join(log))
}

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !f() {
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// HTTP/1.0 の GET で JSON を取る
fn http_get_json(port: u16, path: &str) -> Value {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    assert!(
        head.starts_with("HTTP/1.") && head.contains(" 200 "),
        "{text}"
    );
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {body}"))
}

/// この機械の、ループバックでない IPv4 の住所 (無ければ None)。UDP の connect は何も送らない
fn non_loopback_ipv4() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let a = s.local_addr().ok()?.ip();
    (!a.is_loopback() && !a.is_unspecified()).then(|| a.to_string())
}

#[test]
fn names_are_found_through_peers() {
    let dir = temp_dir("peers_cli");
    let ipc_peer = ipc(&dir, "ipc_peer");
    let ipc_me = ipc(&dir, "ipc_me");
    let ipc_far = ipc(&dir, "ipc_far");
    let far_ip = non_loopback_ipv4();

    // 「別の機械」の System far。peer がこれに http でつなぐ
    let far_conf = dir.join("far.conf");
    std::fs::write(
        &far_conf,
        format!("name: far_sys\noption:\n  http_broker:\n    start: true\n    port: {FAR_PORT}\n"),
    )
    .unwrap();
    let mut far = far_ip
        .as_ref()
        .map(|_| spawn(&dir, &ipc_far, &far_conf, "far.log"));
    if far.is_some() {
        wait_until("the registration of far_sys", || {
            ipc_far.join("names").join("far_sys.json").is_file()
        });
    }

    // peer になる System
    let mut peer_conf = format!(
        "name: peer_sys\nnamespace: robot8\noption:\n  http_broker:\n    start: true\n    port: {PEER_PORT}\n"
    );
    if let Some(ip) = &far_ip {
        peer_conf.push_str(&format!(
            "subsystems:\n  - {{ type_name: http, name: \"{ip}:{FAR_PORT}\" }}\n"
        ));
    }
    let peer_conf_path = dir.join("peer.conf");
    std::fs::write(&peer_conf_path, peer_conf).unwrap();
    let mut peer = spawn(&dir, &ipc_peer, &peer_conf_path, "peer.log");
    wait_until("the registration of peer_sys", || {
        ipc_peer
            .join("names")
            .join("robot8")
            .join("peer_sys.json")
            .is_file()
    });
    let peer_url = format!("http://127.0.0.1:{PEER_PORT}");

    let me = |args: &[&str], peers: Option<&str>| -> Output {
        let mut c = juiz();
        c.current_dir(&dir).env("JUIZ_IPC_DIR", &ipc_me);
        match peers {
            Some(p) => c.env("JUIZ_NAMING_PEERS", p),
            None => c.env_remove("JUIZ_NAMING_PEERS"),
        };
        c.args(args).output().unwrap()
    };

    // この機械 (置き場所 me) には無い
    let o = me(&["name", "resolve", "robot8/peer_sys"], None);
    assert!(!o.status.success(), "{o:?}");

    // --peer で引ける。peer の URL の host (127.0.0.1) のまま
    let o = me(
        &["name", "resolve", "robot8/peer_sys", "--peer", &peer_url],
        None,
    );
    assert!(o.status.success(), "{o:?}\n{}", peer.log_text());
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim(),
        format!("http://127.0.0.1:{PEER_PORT}")
    );
    // 環境変数でも。ポートを省いた書き方 (127.0.0.1:8180) も読む
    let o = me(
        &["name", "resolve", "robot8/peer_sys"],
        Some(&format!("127.0.0.1:{PEER_PORT}")),
    );
    assert!(o.status.success(), "{o:?}");

    // 一覧 --peer には peer の印が付く。ipc は出さない
    let o = me(&["name", "list", "--peer", &peer_url, "--json"], None);
    assert!(o.status.success(), "{o:?}");
    let v: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    let e = v
        .iter()
        .find(|e| e["name"] == "peer_sys")
        .unwrap_or_else(|| panic!("{v:?}"));
    assert_eq!(e["found"], "peer");
    assert!(
        e["brokers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["type_name"] != "ipc"),
        "{e}"
    );
    // 既定の一覧はこの機械だけ (環境変数があっても)
    let o = me(&["name", "list", "--json"], Some(&peer_url));
    let v: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v.iter().all(|e| e["name"] != "peer_sys"), "{v:?}");

    // -s の名前も peer で引ける
    let o = me(
        &["-s", "robot8/peer_sys", "process", "list"],
        Some(&peer_url),
    );
    assert!(o.status.success(), "{o:?}");

    // 答えない peer は警告にして、ほかの peer で探す
    let o = me(
        &[
            "name",
            "resolve",
            "robot8/peer_sys",
            "--peer",
            "http://127.0.0.1:1",
            "--peer",
            &peer_url,
        ],
        None,
    );
    assert!(o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr)
            .contains("warning: peer http://127.0.0.1:1 did not answer"),
        "{o:?}"
    );
    let o = me(
        &["name", "resolve", "nothing", "--peer", "http://127.0.0.1:1"],
        None,
    );
    assert!(!o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("or the peers."),
        "{o:?}"
    );
    // http でない peer は引数の誤り
    let o = me(
        &["name", "resolve", "x", "--peer", "qmp://127.0.0.1:9001"],
        None,
    );
    assert!(!o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("is not an http URL"),
        "{o:?}"
    );

    // peer が別の機械に http でつないだ System も、peer に聞いて引ける (§12.4)
    if let Some(ip) = &far_ip {
        let o = me(&["name", "resolve", "far_sys", "--peer", &peer_url], None);
        assert!(o.status.success(), "{o:?}\n{}", peer.log_text());
        assert_eq!(
            String::from_utf8_lossy(&o.stdout).trim(),
            format!("http://{ip}:{FAR_PORT}")
        );
        let v = http_get_json(PEER_PORT, "/api/naming/list?connected=true");
        let e = v
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "far_sys")
            .unwrap_or_else(|| panic!("{v}"));
        assert_eq!(e["found"], "connected");
        // connected を付けなければ出さない
        let v = http_get_json(PEER_PORT, "/api/naming/list");
        assert!(
            v.as_array().unwrap().iter().all(|e| e["name"] != "far_sys"),
            "{v}"
        );
    } else {
        eprintln!("skipped the connected System part: no non-loopback IPv4 address");
    }

    // conf の option.naming.peers を書いた System: subsystems の名前と API naming/resolve を peer で引く
    let asker_conf = dir.join("asker.conf");
    std::fs::write(
        &asker_conf,
        format!(
            "name: asker\nsubsystems:\n  - {{ type_name: name, name: \"peer_sys?ns=robot8\" }}\noption:\n  naming:\n    peers: [\"{peer_url}\"]\n  http_broker:\n    start: true\n    port: {ASKER_PORT}\n"
        ),
    )
    .unwrap();
    let mut asker = spawn(&dir, &ipc_me, &asker_conf, "asker.log");
    wait_until("the registration of asker", || {
        ipc_me.join("names").join("asker.json").is_file()
    });
    let found = http_get_json(ASKER_PORT, "/api/naming/resolve?name=robot8/peer_sys");
    let found = found.as_array().unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["found"], "peer");
    let list = http_get_json(ASKER_PORT, "/api/naming/list?lan=true");
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "peer_sys" && e["found"] == "peer"),
        "{list}"
    );
    // subsystems に実際の URL でつながっている
    let profile = http_get_json(ASKER_PORT, "/api/system/profile");
    let subs = profile["subsystems"].to_string();
    assert!(
        subs.contains(&format!("127.0.0.1:{PEER_PORT}")),
        "{subs}\n{}",
        asker.log_text()
    );

    // Windows では合図で止められず kill するので stop() は false (common::Daemon::stop_with)
    assert!(asker.stop() || cfg!(not(unix)), "{}", asker.log_text());
    assert!(peer.stop() || cfg!(not(unix)), "{}", peer.log_text());
    if let Some(f) = far.as_mut() {
        assert!(f.stop() || cfg!(not(unix)), "{}", f.log_text());
    }
}
