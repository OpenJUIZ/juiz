//! LAN の自動発見 mDNS (docs/spec/naming.md §11、issue #619)
//!
//! - feature あり (`cargo test -p juiz_app --features naming-mdns --test cli -- mdns::`、CI は test-linux.yml の
//!   naming-mdns ジョブ): `--advertise` の juiz を、登録ファイルの置き場所が別の juiz コマンドから名前で引けること。
//!   同じ機械の中でも、置き場所 (JUIZ_IPC_DIR) を分ければ「別の機械」と同じ経路 (mDNS) を通る。
//!   マルチキャストが自分に戻ってくる網 (ふつうの Linux のランナー) が要る。
//! - feature なし: `--advertise` と `--lan` が警告を出して無視されること (既定の feature のテストで走る)。
//!
//! ポートは 8170〜8171。
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::common::{juiz, path_str, temp_dir, Daemon};

const PORT: u16 = 8170;

fn spawn(dir: &Path, ipc_dir: &Path, conf: &Path, extra: &[&str], log: &str) -> Daemon {
    let mut cmd: Command = juiz();
    cmd.current_dir(dir)
        .env("JUIZ_IPC_DIR", ipc_dir)
        .args(["-f", path_str(conf), "-d"])
        .args(extra);
    Daemon::spawn_command(cmd, dir.join(log))
}

fn write_conf(dir: &Path, port: u16) -> std::path::PathBuf {
    let conf = dir.join("sys.conf");
    std::fs::write(
        &conf,
        format!(
            "name: lan_sys\nnamespace: robot9\noption:\n  http_broker:\n    start: true\n    port: {port}\n"
        ),
    )
    .unwrap();
    conf
}

fn ipc(dir: &Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::create_dir_all(&p).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    p
}

#[cfg(feature = "naming-mdns")]
#[test]
fn advertised_juiz_is_found_by_name_on_the_lan() {
    use juiz_core::prelude::*;

    let dir = temp_dir("mdns_cli");
    let ipc_a = ipc(&dir, "ipc_a");
    let ipc_b = ipc(&dir, "ipc_b");
    let conf = write_conf(&dir, PORT);
    let mut d = spawn(&dir, &ipc_a, &conf, &["--advertise"], "a.log");
    d.wait_log("is advertised on the LAN (mDNS)");

    // 置き場所 b からはこの機械の登録が見えないので、LAN (mDNS) で見つける
    let out = |args: &[&str]| {
        juiz()
            .env("JUIZ_IPC_DIR", &ipc_b)
            .args(args)
            .output()
            .unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let url = loop {
        let o = out(&["name", "resolve", "robot9/lan_sys"]);
        if o.status.success() {
            break String::from_utf8_lossy(&o.stdout).trim().to_owned();
        }
        if Instant::now() > deadline {
            panic!("not found on the LAN: {o:?}\n{}", d.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    assert!(url.starts_with("http://"), "{url}");
    assert!(url.ends_with(&format!(":{PORT}")), "{url}");
    assert!(
        !url.contains("127.0.0.1") && !url.contains("0.0.0.0"),
        "{url}"
    );

    // 一覧 --lan には LAN の印が付く。置き場所 a からは同じ System が「この機械」に 1 つだけ出る
    let o = out(&["name", "list", "--lan", "--check", "--json"]);
    assert!(o.status.success(), "{o:?}");
    let v: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    let mine: Vec<&Value> = v.iter().filter(|e| e["name"] == "lan_sys").collect();
    assert_eq!(mine.len(), 1, "{v:?}");
    assert_eq!(mine[0]["found"], "lan");
    assert_eq!(mine[0]["alive"], true);
    let o = juiz()
        .env("JUIZ_IPC_DIR", &ipc_a)
        .args(["name", "list", "--lan", "--json"])
        .output()
        .unwrap();
    let v: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    let mine: Vec<&Value> = v.iter().filter(|e| e["name"] == "lan_sys").collect();
    assert_eq!(mine.len(), 1, "{v:?}");
    assert_eq!(mine[0]["machine"], "this");

    // -s の名前も LAN で引ける
    let o = out(&["-s", "robot9/lan_sys", "process", "list"]);
    assert!(o.status.success(), "{o:?}");

    // 止めたら goodbye を送るので、引いても見つからない
    assert!(d.stop());
    let o = out(&["name", "resolve", "robot9/lan_sys"]);
    assert!(!o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr)
            .contains("robot9/lan_sys is not registered on this machine or the LAN."),
        "{o:?}"
    );
}

#[cfg(not(feature = "naming-mdns"))]
#[test]
fn advertise_and_lan_are_ignored_without_the_feature() {
    let dir = temp_dir("mdns_cli_off");
    let ipc_a = ipc(&dir, "ipc_a");
    let conf = write_conf(&dir, PORT + 1);
    let mut d = spawn(&dir, &ipc_a, &conf, &["--advertise"], "a.log");
    d.wait_log("is not advertised on the LAN: this juiz is built without the feature naming-mdns");
    // 名前の一覧への登録はそのまま
    let o = juiz()
        .env("JUIZ_IPC_DIR", &ipc_a)
        .args(["name", "list", "--lan"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains(
            "warning: --lan is ignored: this juiz is built without the feature naming-mdns"
        ),
        "{o:?}"
    );
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("robot9/lan_sys"),
        "{o:?}"
    );
    assert!(d.stop());
}
