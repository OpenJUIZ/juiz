//! 機械ごとの登録のテスト (docs/spec/naming.md §13)。登録の置き場所はテストごとに別の一時ディレクトリにする

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::record::write_new;
use super::*;
use crate::prelude::*;

/// 生きていない pid (どの OS でも割り当てられない大きさ)
const DEAD_PID: u32 = 2_000_000_000;

fn temp_names_dir(label: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "juiz-naming-test-{}-{}-{label}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// URL ごとに決めた答えを返す偽の問い合わせ。決めていない URL は応答しない
#[derive(Default)]
struct FakeProbe {
    answers: HashMap<String, Probe>,
}

impl FakeProbe {
    fn with(mut self, url: &str, p: Probe) -> Self {
        self.answers.insert(url.to_owned(), p);
        self
    }
}

impl ProfileProbe for FakeProbe {
    fn probe(&self, _broker: &RegisteredBroker, url: &str) -> Probe {
        self.answers.get(url).cloned().unwrap_or(Probe::NoResponse)
    }
}

fn kinds() -> Vec<String> {
    vec!["ipc".into(), "http".into()]
}

fn request(name: &str, namespace: &str, uuid: &str) -> RegisterRequest {
    RegisterRequest {
        name: name.into(),
        namespace: namespace.into(),
        uuid: uuid.into(),
        package: None,
        brokers: vec![
            RegisteredBroker::new("ipc", name),
            RegisteredBroker::new("http", "0.0.0.0:8001"),
        ],
        unique: false,
    }
}

/// 別の juiz の登録をファイルに直接書く
fn write_other(
    dir: &std::path::Path,
    name: &str,
    namespace: &str,
    uuid: &str,
    pid: u32,
    http: &str,
) {
    let reg = Registration {
        format: FORMAT,
        name: name.into(),
        namespace: namespace.into(),
        uuid: uuid.into(),
        pid,
        host: host_name(),
        juiz_version: "0.0.0".into(),
        started_at: "2026-11-25T00:00:00Z".into(),
        package: Some(jvalue!({"name": name, "version": "0.4.0", "kind": "base"})),
        brokers: vec![RegisteredBroker::new("http", http)],
    };
    write_new(&registration_path(dir, namespace, name).unwrap(), &reg).unwrap();
}

#[test]
fn register_writes_and_unregister_removes() {
    let dir = temp_names_dir("register");
    let names = NameRegistry::at(&dir);
    let reg = names
        .register(
            &request("mobile_sim", "robot1", "u-1"),
            &FakeProbe::default(),
            &kinds(),
        )
        .unwrap();
    let path = dir.join("robot1").join("mobile_sim.json");
    assert_eq!(reg.path(), path);
    assert!(reg.conflict.is_none());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    let ReadResult::Ok(r) = read_registration(&path) else {
        panic!("cannot read {path:?}");
    };
    assert_eq!(r.registered_name(), "robot1/mobile_sim");
    assert_eq!(r.pid, std::process::id());
    assert_eq!(r.uuid, "u-1");
    assert_eq!(r.brokers[1].port, Some(8001));
    assert!(reg.unregister());
    assert!(!path.exists());
    // 空になったネームスペースのディレクトリも消える
    assert!(!dir.join("robot1").exists());
    assert!(dir.exists());
}

#[test]
fn drop_removes_registration() {
    let dir = temp_names_dir("drop");
    let names = NameRegistry::at(&dir);
    let path = {
        let reg = names
            .register(&request("cam", "", "u-1"), &FakeProbe::default(), &kinds())
            .unwrap();
        reg.path().to_path_buf()
    };
    assert_eq!(path, dir.join("cam.json"));
    assert!(!path.exists());
}

#[test]
fn unregister_keeps_others_registration() {
    let dir = temp_names_dir("others");
    let names = NameRegistry::at(&dir);
    let reg = names
        .register(&request("cam", "", "u-1"), &FakeProbe::default(), &kinds())
        .unwrap();
    // 自分の登録が死んだとみなされ、別の juiz が同じ名前で登録し直した
    std::fs::remove_file(reg.path()).unwrap();
    write_other(&dir, "cam", "", "u-2", std::process::id(), "127.0.0.1:8002");
    let path = reg.path().to_path_buf();
    assert!(!reg.unregister());
    assert!(path.exists());
}

#[test]
fn dead_pid_registration_is_removed() {
    let dir = temp_names_dir("dead");
    let names = NameRegistry::at(&dir);
    write_other(&dir, "mobile_sim", "", "u-old", DEAD_PID, "0.0.0.0:8001");
    let path = dir.join("mobile_sim.json");
    // 一覧は pid の確かめだけで消す
    assert!(names
        .list(false, &FakeProbe::default(), &kinds())
        .is_empty());
    assert!(!path.exists());

    // 引くときも消す
    write_other(&dir, "mobile_sim", "", "u-old", DEAD_PID, "0.0.0.0:8001");
    let q = NameQuery::parse("mobile_sim").unwrap();
    assert!(names
        .candidates(&q, &FakeProbe::default(), &kinds())
        .is_empty());
    assert!(!path.exists());

    // 登録するときは消して同じ名前を使う
    write_other(&dir, "mobile_sim", "", "u-old", DEAD_PID, "0.0.0.0:8001");
    let reg = names
        .register(
            &request("mobile_sim", "", "u-new"),
            &FakeProbe::default(),
            &kinds(),
        )
        .unwrap();
    assert_eq!(reg.registration.name, "mobile_sim");
    assert!(reg.conflict.is_none());
}

#[test]
fn reused_pid_with_other_uuid_is_removed() {
    let dir = temp_names_dir("uuid");
    let names = NameRegistry::at(&dir);
    // pid は生きている (このテスト自身) が、ポートの先は別の juiz
    write_other(
        &dir,
        "mobile_sim",
        "",
        "u-old",
        std::process::id(),
        "0.0.0.0:8001",
    );
    let probe = FakeProbe::default().with("http://127.0.0.1:8001", Probe::Uuid("u-other".into()));
    let q = NameQuery::parse("mobile_sim").unwrap();
    assert!(names.candidates(&q, &probe, &kinds()).is_empty());
    assert!(!dir.join("mobile_sim.json").exists());
}

#[test]
fn no_response_is_not_a_candidate_but_kept() {
    let dir = temp_names_dir("noresp");
    let names = NameRegistry::at(&dir);
    write_other(
        &dir,
        "mobile_sim",
        "",
        "u-1",
        std::process::id(),
        "0.0.0.0:8001",
    );
    let q = NameQuery::parse("mobile_sim").unwrap();
    assert!(names
        .candidates(&q, &FakeProbe::default(), &kinds())
        .is_empty());
    assert!(dir.join("mobile_sim.json").exists());
    // --check の一覧には応答しない印を付けて出す
    let listed = names.list(true, &FakeProbe::default(), &kinds());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].alive, Some(false));
    assert_eq!(listed[0].to_json()["alive"], jvalue!(false));
    assert_eq!(listed[0].to_json()["machine"], jvalue!("this"));
    // --check なしは pid だけを見る
    let listed = names.list(false, &FakeProbe::default(), &kinds());
    assert_eq!(listed[0].alive, None);
    assert!(listed[0].to_json().get("alive").is_none());
}

#[test]
fn alive_registration_is_resolved() {
    let dir = temp_names_dir("alive");
    let names = NameRegistry::at(&dir);
    write_other(
        &dir,
        "mobile_sim",
        "robot1",
        "u-1",
        std::process::id(),
        "0.0.0.0:8001",
    );
    let probe = FakeProbe::default().with("http://127.0.0.1:8001", Probe::Uuid("u-1".into()));
    let c = names
        .resolve(&NameQuery::parse("mobile_sim").unwrap(), &probe, &kinds())
        .unwrap();
    assert_eq!(c.registration.registered_name(), "robot1/mobile_sim");
    let (b, url) = c.select_broker(None, &kinds()).unwrap();
    assert_eq!(b.type_name, "http");
    assert_eq!(url, "http://127.0.0.1:8001");
    assert_eq!(
        c.describe(),
        format!(
            "robot1/mobile_sim  0.4.0  pid {}  (this machine)",
            std::process::id()
        )
    );
    // 別のネームスペースでは見つからない
    let err = names
        .resolve(
            &NameQuery::parse("robot2/mobile_sim").unwrap(),
            &probe,
            &kinds(),
        )
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "robot2/mobile_sim is not registered on this machine."
    );
}

#[test]
fn same_name_gets_a_number() {
    let dir = temp_names_dir("dup");
    let names = NameRegistry::at(&dir);
    let probe = FakeProbe::default().with("ipc://mobile_sim", Probe::Uuid("u-1".into()));
    let first = names
        .register(&request("mobile_sim", "", "u-1"), &probe, &kinds())
        .unwrap();
    let second = names
        .register(&request("mobile_sim", "", "u-2"), &probe, &kinds())
        .unwrap();
    assert_eq!(second.registration.name, "mobile_sim-2");
    let c = second.conflict.clone().unwrap();
    assert_eq!(c.registered_name, "mobile_sim");
    assert_eq!(c.pid, std::process::id());
    assert_eq!(second.path(), dir.join("mobile_sim-2.json"));

    // unique なら止まる
    let mut req = request("mobile_sim", "", "u-3");
    req.unique = true;
    let err = names.register(&req, &probe, &kinds()).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "mobile_sim is already registered by another juiz (pid {})",
            std::process::id()
        )
    );

    // 2 つ目を解除しても 1 つ目は残る
    drop(second);
    assert!(first.path().exists());
    assert!(!dir.join("mobile_sim-2.json").exists());
}

#[test]
fn ambiguous_names_are_errors() {
    let dir = temp_names_dir("ambiguous");
    let names = NameRegistry::at(&dir);
    write_other(
        &dir,
        "mobile_sim",
        "robot1",
        "u-1",
        std::process::id(),
        "0.0.0.0:8001",
    );
    write_other(
        &dir,
        "mobile_sim",
        "robot2",
        "u-2",
        std::process::id(),
        "0.0.0.0:8002",
    );
    let probe = FakeProbe::default()
        .with("http://127.0.0.1:8001", Probe::Uuid("u-1".into()))
        .with("http://127.0.0.1:8002", Probe::Uuid("u-2".into()));
    let err = names
        .resolve(&NameQuery::parse("mobile_sim").unwrap(), &probe, &kinds())
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "mobile_sim is ambiguous: robot1/mobile_sim (this machine), robot2/mobile_sim (this machine). Write the namespace or the machine (ex., robot1/mobile_sim)."
    );
    let c = names
        .resolve(
            &NameQuery::parse("robot2/mobile_sim").unwrap(),
            &probe,
            &kinds(),
        )
        .unwrap();
    assert_eq!(c.registration.uuid, "u-2");
}

#[test]
fn unreadable_and_unknown_format_are_skipped() {
    let dir = temp_names_dir("broken");
    let names = NameRegistry::at(&dir);
    // 書いている途中 (作られたばかりで読めない) は飛ばすが消さない
    std::fs::write(dir.join("half.json"), "{\"format\": 1, \"na").unwrap();
    // 知らない版は飛ばす
    std::fs::write(
        dir.join("future.json"),
        "{\"format\": 2, \"name\": \"future\"}",
    )
    .unwrap();
    assert!(names
        .list(false, &FakeProbe::default(), &kinds())
        .is_empty());
    assert!(dir.join("half.json").exists());
    assert!(dir.join("future.json").exists());
    // 知らないキーは読み飛ばす
    let text = format!(
        r#"{{"format":1,"name":"x","namespace":"","uuid":"u","pid":{},"host":"h","juiz_version":"0","started_at":"t","brokers":[],"later_key":1}}"#,
        std::process::id()
    );
    std::fs::write(dir.join("x.json"), text).unwrap();
    assert_eq!(names.list(false, &FakeProbe::default(), &kinds()).len(), 1);
}

#[test]
fn select_broker_order_and_kind() {
    let reg = Registration {
        format: FORMAT,
        name: "mobile_sim".into(),
        namespace: "".into(),
        uuid: "u".into(),
        pid: 1,
        host: "h".into(),
        juiz_version: "0".into(),
        started_at: "t".into(),
        package: None,
        brokers: vec![
            RegisteredBroker::new("http", "0.0.0.0:8001"),
            RegisteredBroker::new("zenoh", "[::]:7448"),
            RegisteredBroker::new("ipc", "mobile_sim"),
        ],
    };
    let c = Candidate::local(reg);
    let all: Vec<String> = LOCAL_ORDER.iter().map(|s| s.to_string()).collect();
    assert_eq!(c.select_broker(None, &all).unwrap().1, "ipc://mobile_sim");
    let no_ipc: Vec<String> = vec!["zenoh".into(), "http".into()];
    assert_eq!(
        c.select_broker(None, &no_ipc).unwrap().1,
        "zenoh://127.0.0.1:7448"
    );
    assert_eq!(
        c.select_broker(Some("http"), &all).unwrap().1,
        "http://127.0.0.1:8001"
    );
    assert_eq!(
        c.select_broker(Some("qmp"), &all).unwrap_err().to_string(),
        "mobile_sim has no qmp broker (it has: http, zenoh, ipc)"
    );
}

#[test]
fn query_parse_and_match() {
    let q = NameQuery::parse("mobile_sim").unwrap();
    assert_eq!((q.namespace.clone(), q.host.clone()), (None, None));
    let q = NameQuery::parse("robot1/mobile_sim").unwrap();
    assert_eq!(q.namespace.as_deref(), Some("robot1"));
    let q = NameQuery::parse("/mobile_sim").unwrap();
    assert_eq!(q.namespace.as_deref(), Some(""));
    assert_eq!(q.to_string(), "/mobile_sim");
    let q = NameQuery::parse("a/b/mobile_sim@robot1-pc").unwrap();
    assert_eq!(q.namespace.as_deref(), Some("a/b"));
    assert_eq!(q.host.as_deref(), Some("robot1-pc"));
    assert_eq!(q.to_string(), "a/b/mobile_sim@robot1-pc");
    for ng in ["", "a b", "x@", "x@a@b", ".x", "robot 1/x", "x:8000"] {
        assert!(NameQuery::parse(ng).is_err(), "{ng}");
    }
}

#[test]
fn local_url_replaces_any_address() {
    for (t, n, url) in [
        ("http", "0.0.0.0:8001", "http://127.0.0.1:8001"),
        ("http", "localhost:8001", "http://localhost:8001"),
        ("qmp", "[::]:9000", "qmp://127.0.0.1:9000"),
        ("zenoh", ":7448", "zenoh://127.0.0.1:7448"),
        ("ipc", "cam0", "ipc://cam0"),
    ] {
        assert_eq!(RegisteredBroker::new(t, n).local_url(), url, "{t} {n}");
    }
    assert_eq!(RegisteredBroker::new("ipc", "cam0").port, None);
    assert_eq!(
        RegisteredBroker::new("qmp", "0.0.0.0:9000").port,
        Some(9000)
    );
}

#[test]
fn options_and_names_from_manifest() {
    let m = jvalue!({"name": "sys", "namespace": "/robot1/", "option": {"naming": {"unique": true, "zzz": 1}}});
    assert_eq!(
        naming_options(&m),
        NamingOptions {
            register: true,
            unique: true,
            advertise: false
        }
    );
    assert_eq!(name_of_manifest(&m).as_deref(), Some("sys"));
    assert_eq!(namespace_of_manifest(&m), "robot1");
    let mut m =
        jvalue!({"name": "sys", "package": {"name": "pkg", "version": "1.0.0", "kind": "app"}});
    assert_eq!(name_of_manifest(&m).as_deref(), Some("pkg"));
    set_register_option(&mut m, false);
    assert!(!naming_options(&m).register);
    assert!(!naming_options(&m).advertise);
    set_advertise_option(&mut m, true);
    assert!(naming_options(&m).advertise);
    assert!(!naming_options(&m).register);
    assert_eq!(name_of_manifest(&jvalue!({})), None);
}

#[test]
fn register_system_skips_invalid_names() {
    // 規則に合わない名前は警告を出して登録しない (起動は止めない)
    let m = jvalue!({"name": "my system"});
    let r = register_system(&m, "u", vec![], &FakeProbe::default(), &kinds()).unwrap();
    assert!(r.is_none());
    let m = jvalue!({"name": "sys", "option": {"naming": {"register": false}}});
    let r = register_system(&m, "u", vec![], &FakeProbe::default(), &kinds()).unwrap();
    assert!(r.is_none());
}

#[test]
fn rfc3339_format() {
    let s = record::now_rfc3339();
    assert_eq!(s.len(), 20, "{s}");
    assert!(s.ends_with('Z') && s.as_bytes()[10] == b'T', "{s}");
}

#[test]
fn name_spec_parses_urls_and_server_names() {
    // name://<名前>[@<機械>][?ns=…&type=…] (§9.1)
    let s = NameSpec::parse_url("name://mobile_sim").unwrap().unwrap();
    assert_eq!(s.query, NameQuery::parse("mobile_sim").unwrap());
    assert_eq!(s.kind, None);
    let s = NameSpec::parse_url("name://mobile_sim?ns=robot1&type=http")
        .unwrap()
        .unwrap();
    assert_eq!(s.query.namespace.as_deref(), Some("robot1"));
    assert_eq!(s.kind.as_deref(), Some("http"));
    let s = NameSpec::parse_url("name://mobile_sim?ns=/").unwrap().unwrap();
    assert_eq!(s.query.namespace.as_deref(), Some(""));
    let s = NameSpec::parse_url("name://mobile_sim@robot1-pc/").unwrap().unwrap();
    assert_eq!(s.query.host.as_deref(), Some("robot1-pc"));
    assert!(NameSpec::parse_url("http://127.0.0.1:8000").unwrap().is_none());
    // ネームスペースは / で書かない。知らないクエリはエラー
    let e = NameSpec::parse_url("name://robot1/mobile_sim").unwrap_err();
    assert!(e.to_string().contains("ns=robot1"), "{e}");
    let e = NameSpec::parse_url("name://mobile_sim?port=1").unwrap_err();
    assert!(e.to_string().contains("unknown query 'port=1'"), "{e}");
    assert!(NameSpec::parse_url("name://mobile_sim?ns=a b").is_err());

    // -s の値 (§9.2): :// も : も無ければ名前
    let s = NameSpec::parse_server("robot1/mobile_sim").unwrap().unwrap();
    assert_eq!(s.query, NameQuery::parse("robot1/mobile_sim").unwrap());
    assert!(NameSpec::parse_server("name://mobile_sim").unwrap().is_some());
    assert!(NameSpec::parse_server("http://127.0.0.1:8000").unwrap().is_none());
    assert!(NameSpec::parse_server("localhost:8000").unwrap().is_none());
    assert!(NameSpec::parse_server("ipc://juiz").unwrap().is_none());
    assert!(NameSpec::parse_server("my system").is_err());

    assert_eq!(
        split_broker_url("http://127.0.0.1:8001/"),
        ("http".to_owned(), "127.0.0.1:8001".to_owned())
    );
}

#[test]
fn resolved_selects_broker_by_kind() {
    let dir = temp_names_dir("resolved");
    let reg = NameRegistry::at(&dir);
    let probe = FakeProbe::default().with("ipc://sys", Probe::Uuid("u1".into()));
    let r = reg.register(&request("sys", "", "u1"), &probe, &kinds()).unwrap();
    let q = NameQuery::parse("sys").unwrap();
    let c = reg.resolve(&q, &probe, &kinds()).unwrap();
    let res = Resolved::new(c.clone(), None, &kinds()).unwrap();
    assert_eq!(res.url, "ipc://sys");
    assert_eq!(res.type_and_name(), ("ipc".to_owned(), "sys".to_owned()));
    let res = Resolved::new(c.clone(), Some("http"), &kinds()).unwrap();
    assert_eq!(res.url, "http://127.0.0.1:8001");
    let e = Resolved::new(c, Some("zenoh"), &kinds()).unwrap_err();
    assert_eq!(e.to_string(), "sys has no zenoh broker (it has: ipc, http)");
    r.unregister();
    let _ = std::fs::remove_dir_all(&dir);
}
