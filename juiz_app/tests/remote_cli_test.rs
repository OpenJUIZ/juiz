//! 起動中の juiz を別の juiz コマンドから操作するサブコマンドのテスト (issue #222)
//!
//! Process・Container・ContainerProcess・EC を持つ juiz を `-d` で起動し、
//! `process` / `container` / `container-process` / `connection` / `ec` の各サブコマンドを叩く。
//!
//! プラグイン (increment_process, talker, listener, example_container とその ContainerProcess, timer_ec) は
//! scripts/test.sh で ../target/debug にビルドされている前提。
mod common;

use common::{path_str, plugin_dir_str, run, temp_dir, Daemon};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

const PORT: u16 = 8070;

fn host() -> String {
    format!("http://127.0.0.1:{PORT}")
}

fn write_robot_conf(dir: &Path) -> PathBuf {
    let robot = dir.join("robot.conf");
    let p = plugin_dir_str();
    std::fs::write(
        &robot,
        format!(
            r#"name: robot
option:
  http_broker:
    start: true
    port: {PORT}
plugins:
  process_factories:
    increment_process: {{ path: "{p}" }}
    talker: {{ path: "{p}" }}
    listener: {{ path: "{p}" }}
  container_factories:
    example_container:
      path: "{p}"
      processes:
        example_container_increment: {{ path: "{p}" }}
        example_container_get: {{ path: "{p}" }}
  ec_factories:
    timer_ec: {{ path: "{p}" }}
processes:
  - {{ type_name: increment_process, name: inc0 }}
  - {{ type_name: talker, name: talker0 }}
  - {{ type_name: listener, name: listener0 }}
containers:
  - type_name: example_container
    name: c0
    processes:
      - {{ type_name: example_container_increment, name: c0inc }}
      - {{ type_name: example_container_get, name: c0get }}
ecs:
  - {{ type_name: TimerEC, name: timer0, rate: 10.0 }}
"#
        ),
    )
    .unwrap();
    robot
}

/// コマンド側の空の juiz。-s で起動中の juiz を指す
struct Client {
    conf: PathBuf,
}

impl Client {
    fn run(&self, args: &[&str]) -> String {
        let out = self.run_raw(args);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn run_raw(&self, args: &[&str]) -> Output {
        let server = host();
        let mut all = vec!["-f", path_str(&self.conf), "-s", &server];
        all.extend_from_slice(args);
        run(&all)
    }

    /// 一覧系のサブコマンド (-f でコマンド側の conf を渡すもの)
    fn list(&self, sub: &str, extra: &[&str]) -> String {
        let conf = path_str(&self.conf).to_owned();
        let mut args = vec![sub, "list", "-f", &conf];
        args.extend_from_slice(extra);
        self.run(&args)
    }
}

fn wait_for(client: &Client, robot: &Daemon, id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !client.list("process", &[]).contains(id) {
        if Instant::now() > deadline {
            panic!("{id} did not appear. log:\n{}", robot.log_text());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
fn remote_subcommands() {
    let dir = temp_dir("remote_cli_test");
    let robot_conf = write_robot_conf(&dir);
    let client = Client {
        conf: dir.join("client.conf"),
    };
    std::fs::write(&client.conf, "name: client\n").unwrap();
    let mut robot = Daemon::spawn(&["-f", path_str(&robot_conf), "-d"], dir.join("robot.log"));
    let h = host();
    let inc0 = format!("{h}/process/inc0::increment_process");
    wait_for(&client, &robot, &inc0);

    process(&client, &h, &inc0);
    container_process(&client, &h);
    connection(&client, &h);
    execution_context(&client, &h);
    composite(&client, &dir);
    // Container を消すのは最後 (ContainerProcess も消える)
    container(&client, &h);

    assert!(robot.stop() || cfg!(not(unix)), "{}", robot.log_text());
    let _ = std::fs::remove_dir_all(&dir);
}

fn process(client: &Client, h: &str, inc0: &str) {
    let list = client.list("process", &[]);
    for name in ["inc0", "talker0", "listener0"] {
        assert!(list.contains(&format!("/process/{name}::")), "{list}");
    }
    // -a は ContainerProcess も含める
    let list = client.list("process", &["-a"]);
    assert!(
        list.contains("c0inc::example_container_increment"),
        "{list}"
    );

    let info = client.run(&["process", "info", inc0]);
    assert!(info.contains("inc0::increment_process"), "{info}");
    let out = client.run(&["process", "call", inc0, r#"{"arg1": 2}"#, "-p"]);
    assert!(out.contains("Value(3)"), "{out}");
    // -p が無ければ結果は出さない
    let out = client.run(&["process", "call", inc0, r#"{"arg1": 2}"#]);
    assert!(!out.contains("Value("), "{out}");

    // 無い Process はエラーを表示する
    let missing = format!("{h}/process/zz::increment_process");
    let out = client.run(&["process", "info", &missing]);
    assert!(out.contains("Error"), "{out}");
    let out = client.run(&["process", "call", &missing, "{}"]);
    assert!(out.contains("Error"), "{out}");
}

fn container(client: &Client, h: &str) {
    let c0 = format!("{h}/container/c0::example_container");
    let list = client.list("container", &[]);
    assert_eq!(list.trim(), format!("[\"{c0}\"]"));
    let table = client.list("container", &["--long"]);
    assert!(table.starts_with("NAMESPACE"), "{table}");
    assert!(table.contains(&c0), "{table}");
    assert!(client.list("container", &["--namespace", "robot9"]).trim() == "[]");

    let info = client.run(&["container", "info", &c0]);
    assert!(
        info.contains("ContainerProfile(\"example_container\""),
        "{info}"
    );
    assert!(
        info.contains("c0inc::example_container_increment"),
        "{info}"
    );
    let out = client.run(&[
        "container",
        "info",
        &format!("{h}/container/zz::example_container"),
    ]);
    assert!(out.contains("Error"), "{out}");

    // コマンド側のローカルにない core:// の Container は消せない
    let out = client.run(&[
        "container",
        "destroy",
        "core://core/container/c0::example_container",
    ]);
    assert!(out.contains("Error"), "{out}");

    let out = client.run(&["container", "destroy", &c0]);
    assert!(out.contains("destroyed: "), "{out}");
    assert!(out.contains("c0::example_container"), "{out}");
    assert_eq!(client.list("container", &[]).trim(), "[]");
    assert_eq!(client.list("container-process", &[]).trim(), "[]");
}

fn container_process(client: &Client, h: &str) {
    let cinc = format!("{h}/container_process/c0inc::example_container_increment:c0");
    let cget = format!("{h}/container_process/c0get::example_container_get:c0");
    let list = client.list("container-process", &[]);
    assert!(list.contains(&cinc) && list.contains(&cget), "{list}");
    let table = client.list("container-process", &["--long"]);
    assert!(table.starts_with("NAMESPACE"), "{table}");

    let info = client.run(&["container-process", "info", &cinc]);
    assert!(
        info.contains("c0inc::example_container_increment"),
        "{info}"
    );

    // increment で Container の値を変え、get で読む
    let out = client.run(&["container-process", "call", &cinc, r#"{"arg1": 5}"#, "-p"]);
    assert!(out.contains("Value(5)"), "{out}");
    let out = client.run(&["container-process", "call", &cget, "{}", "-p"]);
    assert!(out.contains("Value(5)"), "{out}");
    let out = client.run(&["container-process", "call", &cget, "{}"]);
    assert!(!out.contains("Value("), "{out}");

    // 無い ContainerProcess はエラー (container-process は process と違い、終了コードも 1 になる)
    let missing = format!("{h}/container_process/zz::example_container_get:c0");
    for args in [
        vec!["container-process", "info", &missing],
        vec!["container-process", "call", &missing, "{}"],
    ] {
        let out = client.run_raw(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("Error"),
            "{out:?}"
        );
    }
}

fn connection(client: &Client, h: &str) {
    assert_eq!(client.run(&["connection", "list"]).trim(), "[]");
    let talker = format!("{h}/process/talker0::talker");
    let listener = format!("{h}/process/listener0::listener");
    let out = client.run(&["connection", "create", &talker, "arg1", &listener]);
    assert!(!out.contains("Error"), "{out}");
    // 既定は delete にそのまま渡せる ID の JSON 配列。同じ接続は 1 つだけ出る
    let ids = connection_ids(client);
    assert_eq!(ids.len(), 1, "{ids:?}");
    assert!(ids[0].starts_with("connection://"), "{ids:?}");
    assert!(ids[0].contains("talker0::talker|arg1|"), "{ids:?}");
    let long = client.run(&["connection", "list", "--long"]);
    assert!(long.contains("-(arg1)->"), "{long}");
    assert!(long.contains("listener0::listener"), "{long}");

    // list の ID で消す
    let out = client.run(&["connection", "delete", &ids[0]]);
    assert!(!out.contains("Error"), "{out}");
    assert!(connection_ids(client).is_empty());
    // 消した接続をもう一度消すとエラーを表示する
    let out = client.run(&["connection", "delete", &ids[0]]);
    assert!(out.contains("Error"), "{out}");

    // create と同じ <src> <arg> <dst> でも消せる
    client.run(&["connection", "create", &talker, "arg1", &listener]);
    assert_eq!(connection_ids(client).len(), 1);
    let out = client.run(&["connection", "delete", &talker, "arg1", &listener]);
    assert!(!out.contains("Error"), "{out}");
    assert!(connection_ids(client).is_empty());

    // 種類の誤りはエラーを表示する
    let out = client.run(&[
        "connection",
        "create",
        "-t",
        "no_such_type",
        &talker,
        "arg1",
        &listener,
    ]);
    assert!(out.contains("Error"), "{out}");

    // ID として読めない引数はエラーを表示する
    let out = client.run(&["connection", "delete", "no_such_connection"]);
    assert!(out.contains("Error"), "{out}");
    let out = client.run(&["connection", "delete", &talker, "arg1"]);
    assert!(out.contains("Error"), "{out}");
}

fn connection_ids(client: &Client) -> Vec<String> {
    let list = client.run(&["connection", "list"]);
    juiz_sdk::serde_json::from_str(list.trim()).unwrap_or_else(|e| panic!("{e}: {list}"))
}

fn execution_context(client: &Client, h: &str) {
    let ec = format!("{h}/execution_context/timer0::TimerEC");
    let list = client.list("ec", &[]);
    assert_eq!(list.trim(), format!("[\"{ec}\"]"));
    let table = client.list("ec", &["--long"]);
    assert!(
        table.contains("timer0") && table.contains("TimerEC"),
        "{table}"
    );

    let conf = path_str(&client.conf).to_owned();
    let out = client.run(&["ec", "start", &ec, "-f", &conf]);
    assert!(!out.contains("Error"), "{out}");
    let out = client.run(&["ec", "stop", &ec, "-f", &conf]);
    assert!(!out.contains("Error"), "{out}");
    // core://core/... はコマンド側に無ければ -s のサーバーの EC として扱う
    let local = "core://core/execution_context/timer0::TimerEC";
    let out = client.run(&["ec", "start", local, "-f", &conf]);
    assert!(!out.contains("Error"), "{out}");
    let out = client.run(&["ec", "stop", local, "-f", &conf]);
    assert!(!out.contains("Error"), "{out}");

    // 無い EC はエラーを表示する
    let out = client.run(&[
        "ec",
        "stop",
        "core://core/execution_context/zz::TimerEC",
        "-f",
        &conf,
    ]);
    assert!(out.contains("Error"), "{out}");
    let out = client.run(&["ec", "start", "no_such_identifier", "-f", &conf]);
    assert!(out.contains("Error"), "{out}");
}

fn composite(client: &Client, dir: &Path) {
    assert_eq!(client.run(&["composite", "list"]).trim(), "[]");
    let defs = dir.join("composites.yaml");
    std::fs::write(
        &defs,
        r#"composites:
  - type_name: inc_twice
    body:
      process: increment_process
      args:
        arg1: { process: increment_process, args: { arg1: { expose: x } } }
  - type_name: add_chain
    containers: { a: example_container, b: example_container }
    body:
      process: example_container_increment
      container: b
      args:
        arg1: { process: example_container_increment, container: a }
composite_containers:
  - name: cc0
    slots: { a: c0, b: c0 }
    processes:
      - { type_name: add_chain, name: chain0 }
"#,
    )
    .unwrap();
    let out = client.run(&["composite", "create", path_str(&defs)]);
    assert!(out.contains("created: inc_twice (process)"), "{out}");
    assert!(
        out.contains("created: add_chain (container_process)"),
        "{out}"
    );
    assert!(out.contains("cc0"), "{out}");

    // 1 つだけの定義 (type_name が先頭にある形)
    let single = dir.join("inc_four.yaml");
    std::fs::write(
        &single,
        "type_name: inc_four\nbody: { process: inc_twice, args: { x: { process: inc_twice } } }\n",
    )
    .unwrap();
    let out = client.run(&["composite", "create", path_str(&single)]);
    assert!(out.contains("created: inc_four (process)"), "{out}");

    let list = client.run(&["composite", "list"]);
    for name in ["inc_twice", "add_chain", "inc_four"] {
        assert!(list.contains(name), "{list}");
    }
    let long = client.run(&["composite", "list", "--long"]);
    assert!(long.contains("add_chain  container_process"), "{long}");
    assert!(long.contains("increment_process"), "{long}");
    let info = client.run(&["composite", "info", "inc_twice"]);
    assert!(info.contains("\"type_name\": \"inc_twice\""), "{info}");

    // 作った CompositeContainer は container info でスロットが見える
    let cc0 = client
        .list("container", &[])
        .split('"')
        .find(|s| s.contains("/container/cc0::"))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("cc0 is not listed"));
    let info = client.run(&["container", "info", &cc0]);
    assert!(info.contains("c0"), "{info}");
    let out = client.run(&["container", "destroy", &cc0]);
    assert!(out.contains("destroyed: "), "{out}");

    for name in ["inc_four", "inc_twice", "add_chain"] {
        let out = client.run(&["composite", "destroy", name]);
        assert!(out.contains(&format!("destroyed: {name}")), "{out}");
    }
    assert_eq!(client.run(&["composite", "list"]).trim(), "[]");

    // 無いもの・読めないファイルはエラーで終わる
    for args in [
        vec!["composite", "info", "no_such_composite"],
        vec!["composite", "destroy", "no_such_composite"],
        vec!["composite", "create", "no_such_file.yaml"],
    ] {
        let out = client.run_raw(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
    }
}
