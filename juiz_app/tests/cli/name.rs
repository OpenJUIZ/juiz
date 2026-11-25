//! この機械の名前の一覧 (docs/spec/naming.md §3〜§7・§9.3・§13、issue #616)
//!
//! - `juiz -f sys.conf -d` で動き続ける juiz は `<JUIZ_IPC_DIR>/names/<ns>/<名前>.json` に登録され、止めると消える
//! - 同じ conf の 2 つ目は `-2` を付けて登録され、HTTP のポートは取り直した後の番号が載る
//! - `option.naming.unique: true` なら 2 つ目は起動を止める
//! - `juiz name list` / `juiz name resolve` で引ける。異常終了で残った登録は引くときに消える
//!
//! 置き場所はテストごとの一時ディレクトリ (JUIZ_IPC_DIR) にする。
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::common::{juiz, path_str, temp_dir, Daemon};
use juiz_core::prelude::*;

const PORT: u16 = 8150;

fn name_cmd(ipc_dir: &Path, args: &[&str]) -> std::process::Output {
    juiz()
        .env("JUIZ_IPC_DIR", ipc_dir)
        .arg("name")
        .args(args)
        .output()
        .unwrap()
}

fn list_json(ipc_dir: &Path) -> Vec<Value> {
    let o = name_cmd(ipc_dir, &["list", "--json"]);
    assert!(o.status.success(), "{o:?}");
    serde_json::from_slice::<Vec<Value>>(&o.stdout).unwrap()
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

fn spawn(dir: &Path, ipc_dir: &Path, conf: &Path, log: &str) -> Daemon {
    let mut cmd: Command = juiz();
    cmd.current_dir(dir)
        .env("JUIZ_IPC_DIR", ipc_dir)
        .args(["-f", path_str(conf), "-d"]);
    Daemon::spawn_command(cmd, dir.join(log))
}

#[test]
fn register_list_resolve_and_unregister() {
    let dir = temp_dir("name_cli");
    let ipc_dir = dir.join("ipc");
    let conf = dir.join("sys.conf");
    std::fs::write(
        &conf,
        format!(
            "name: mysys\nnamespace: robot1\noption:\n  http_broker:\n    start: true\n    port: {PORT}\n"
        ),
    )
    .unwrap();

    let mut first = spawn(&dir, &ipc_dir, &conf, "first.log");
    let file1 = ipc_dir.join("names").join("robot1").join("mysys.json");
    wait_until("the first registration", || file1.is_file());

    let mut second = spawn(&dir, &ipc_dir, &conf, "second.log");
    let file2 = ipc_dir.join("names").join("robot1").join("mysys-2.json");
    wait_until("the second registration", || file2.is_file());
    second.wait_log("this juiz is registered as robot1/mysys-2");
    assert!(
        second
            .log_text()
            .contains("warning: robot1/mysys is already registered by another juiz (pid "),
        "{}",
        second.log_text()
    );

    // 一覧: 2 つ目は取り直した後のポートで載る
    let listed = list_json(&ipc_dir);
    assert_eq!(listed.len(), 2, "{listed:?}");
    let by_name = |n: &str| {
        listed
            .iter()
            .find(|v| v["name"] == jvalue!(n))
            .unwrap()
            .clone()
    };
    let a = by_name("mysys");
    let b = by_name("mysys-2");
    assert_eq!(a["namespace"], jvalue!("robot1"));
    assert_eq!(a["machine"], jvalue!("this"));
    assert_eq!(a["format"], jvalue!(1));
    assert_eq!(a["brokers"][0]["port"], jvalue!(PORT));
    assert_eq!(b["brokers"][0]["port"], jvalue!(PORT + 1));
    assert_ne!(a["uuid"], b["uuid"]);

    // --check: どちらも応答する
    let o = name_cmd(&ipc_dir, &["list", "--check", "--json"]);
    let checked: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    assert!(
        checked.iter().all(|v| v["alive"] == jvalue!(true)),
        "{checked:?}"
    );

    // resolve: 選んだブローカーの URL を 1 行で
    let o = name_cmd(&ipc_dir, &["resolve", "robot1/mysys"]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim(),
        format!("http://127.0.0.1:{PORT}")
    );
    let o = name_cmd(&ipc_dir, &["resolve", "mysys-2"]);
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim(),
        format!("http://127.0.0.1:{}", PORT + 1)
    );
    let o = name_cmd(&ipc_dir, &["resolve", "robot2/mysys"]);
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr)
            .contains("robot2/mysys is not registered on this machine."),
        "{o:?}"
    );
    let o = name_cmd(&ipc_dir, &["resolve", "mysys", "--all", "--json"]);
    let all: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(all.len(), 1);

    // 止めると自分の登録だけが消える (Windows の stop は kill なので、引くときに消える)
    second.stop();
    #[cfg(unix)]
    assert!(!file2.exists());
    assert_eq!(list_json(&ipc_dir).len(), 1);
    assert!(!file2.exists());
    assert!(file1.is_file());

    // unique: true の conf は、同じ名前が生きていれば起動を止める
    let unique = dir.join("unique.conf");
    std::fs::write(
        &unique,
        format!(
            "name: mysys\nnamespace: robot1\noption:\n  naming:\n    unique: true\n  http_broker:\n    start: true\n    port: {PORT}\n"
        ),
    )
    .unwrap();
    let o = juiz()
        .current_dir(&dir)
        .env("JUIZ_IPC_DIR", &ipc_dir)
        .args(["-f", path_str(&unique), "-d"])
        .output()
        .unwrap();
    assert!(!o.status.success(), "{o:?}");
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("robot1/mysys is already registered by another juiz (pid "),
        "{out}"
    );
    assert!(!file2.exists());

    // --no-register なら登録しない
    let mut quiet = {
        let mut cmd: Command = juiz();
        cmd.current_dir(&dir).env("JUIZ_IPC_DIR", &ipc_dir).args([
            "-f",
            path_str(&conf),
            "-d",
            "--no-register",
        ]);
        Daemon::spawn_command(cmd, dir.join("quiet.log"))
    };
    quiet.wait_log("joined the juiz");
    assert_eq!(list_json(&ipc_dir).len(), 1);
    quiet.stop();

    // 異常終了で残った登録は、引くときに消える
    #[cfg(unix)]
    {
        let pid = first.pid();
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
        first.wait_exit();
        assert!(file1.is_file());
        assert!(list_json(&ipc_dir).is_empty());
        assert!(!file1.exists());
    }
    #[cfg(not(unix))]
    {
        first.stop();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
