//! 名前で接続先を指す (docs/spec/naming.md §9.1・§9.2・§9.4、issue #618)
//!
//! - `juiz -s <名前> …` は、この機械の登録で引いたブローカーにつなぐ (同じ機械は ipc が先)
//! - `-s name://<名前>?type=http` で種類を指定できる。登録が無ければエラー
//! - conf の `subsystems` に `type_name: name` で書いた相手につながる (一覧には実際の URL が出る)
//! - API `naming/list`・`naming/resolve` で登録を引ける
//!
//! 置き場所はテストごとの一時ディレクトリ (JUIZ_IPC_DIR) にする。
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use super::common::{juiz, path_str, temp_dir, Daemon};
use juiz_core::prelude::*;

const TARGET_PORT: u16 = 8160;
const CHILD_PORT: u16 = 8161;

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !f() {
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn spawn(dir: &Path, ipc_dir: &Path, conf: &Path, log: &str) -> Daemon {
    let mut cmd: Command = juiz();
    cmd.current_dir(dir)
        .env("JUIZ_IPC_DIR", ipc_dir)
        .args(["-f", path_str(conf), "-d"]);
    Daemon::spawn_command(cmd, dir.join(log))
}

/// `juiz -s <server> process list` を、名前の解決のログが見えるようにして走らせる
fn process_list(dir: &Path, ipc_dir: &Path, server: &str) -> Output {
    juiz()
        .current_dir(dir)
        .env("JUIZ_IPC_DIR", ipc_dir)
        .env("RUST_LOG", "juiz_app=info")
        .args(["-s", server, "process", "list"])
        .output()
        .unwrap()
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

#[test]
fn point_to_targets_by_name() {
    let dir = temp_dir("name_target_cli");
    let ipc_dir = dir.join("ipc");
    let target_conf = dir.join("target.conf");
    std::fs::write(
        &target_conf,
        format!(
            "name: target\nnamespace: robot1\nbrokers:\n  - {{ type_name: ipc, name: target_ipc }}\noption:\n  http_broker:\n    start: true\n    port: {TARGET_PORT}\n"
        ),
    )
    .unwrap();
    let mut target = spawn(&dir, &ipc_dir, &target_conf, "target.log");
    let file = ipc_dir.join("names").join("robot1").join("target.json");
    wait_until("the registration of target", || file.is_file());

    // -s <名前>: 同じ機械では ipc が先 (§7.3)
    for server in [
        "target",
        "robot1/target",
        "name://target",
        "name://target?ns=robot1",
    ] {
        let o = process_list(&dir, &ipc_dir, server);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(o.status.success(), "{server}: {o:?}");
        assert!(
            err.contains(&format!("{server} is resolved to ipc://target_ipc")),
            "{server}: {err}"
        );
    }
    // 種類の指定
    let o = process_list(&dir, &ipc_dir, "name://target?type=http");
    assert!(o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains(&format!(
            "name://target?type=http is resolved to http://127.0.0.1:{TARGET_PORT}"
        )),
        "{o:?}"
    );
    // 登録の無い名前・無い種類はエラー
    let o = process_list(&dir, &ipc_dir, "robot2/target");
    assert!(!o.status.success(), "{o:?}");
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("robot2/target is not registered on this machine."),
        "{out}"
    );
    let o = process_list(&dir, &ipc_dir, "name://target?type=qmp");
    assert!(!o.status.success(), "{o:?}");
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    // 種類の並びは登録した順 (ブローカーの一覧の順は決まっていない)
    assert!(
        out.contains("robot1/target has no qmp broker (it has: http, ipc)")
            || out.contains("robot1/target has no qmp broker (it has: ipc, http)"),
        "{out}"
    );
    // URL と host:port は今までどおり
    let o = process_list(&dir, &ipc_dir, &format!("127.0.0.1:{TARGET_PORT}"));
    assert!(
        !String::from_utf8_lossy(&o.stderr).contains("is resolved to"),
        "{o:?}"
    );

    // API naming/list・naming/resolve (§9.4)
    let listed = http_get_json(TARGET_PORT, "/api/naming/list");
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["name"], jvalue!("target"));
    assert_eq!(listed[0]["machine"], jvalue!("this"));
    let checked = http_get_json(TARGET_PORT, "/api/naming/list?check=true");
    assert_eq!(checked[0]["alive"], jvalue!(true), "{checked}");
    let found = http_get_json(TARGET_PORT, "/api/naming/resolve?name=robot1/target");
    assert_eq!(found.as_array().unwrap().len(), 1, "{found}");
    let none = http_get_json(TARGET_PORT, "/api/naming/resolve?name=nothing");
    assert_eq!(none, jvalue!([]));

    // conf の subsystems に名前で書く (§9.1)。一覧には実際の URL が出る
    let child_conf = dir.join("child.conf");
    std::fs::write(
        &child_conf,
        format!(
            "name: child\nsubsystems:\n  - {{ type_name: name, name: \"target?type=http\" }}\noption:\n  http_broker:\n    start: true\n    port: {CHILD_PORT}\n"
        ),
    )
    .unwrap();
    let mut child = spawn(&dir, &ipc_dir, &child_conf, "child.log");
    wait_until("the registration of child", || {
        ipc_dir.join("names").join("child.json").is_file()
    });
    let profile = http_get_json(CHILD_PORT, "/api/system/profile");
    let subsystems = profile["subsystems"].as_array().unwrap();
    assert_eq!(subsystems.len(), 1, "{profile}\n{}", child.log_text());
    let text = subsystems[0].to_string();
    assert_eq!(subsystems[0]["type_name"], jvalue!("http"), "{text}");
    assert_eq!(
        subsystems[0]["name"],
        jvalue!(format!("127.0.0.1:{TARGET_PORT}")),
        "{text}"
    );
    assert!(!text.contains("name://"), "{text}");

    child.stop();
    target.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
