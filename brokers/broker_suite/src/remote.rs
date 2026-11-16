//! 別の System の qmp・Zenoh ブローカーに繋ぐテストの本体。qmp と Zenoh で同じものを走らせる
//!
//! 各関数は 1 つのテストの中身で、ブローカーの違いは [`RemoteBroker`] で渡す。
//! qmp・Zenoh のテストは `#[test]` の関数からこれを呼ぶだけにする。
//! ポートは qmp の番号に [`RemoteBroker::port_offset`] を足したものを使う。
//!
//! プラグイン (increment_process, example_container, timer_ec と qmp_broker / zenoh_broker) と
//! juiz コマンドは `cargo build` で target/debug にビルドされている前提 (CI の test-*.yml と同じ)。

use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use juiz_core::models::{sha256_hex, ContainerModelData};
use juiz_core::prelude::*;
use juiz_core::CRUDBrokerProxy;
use juiz_sdk::geometry::{Pose, Quaternion, Vector3};
use juiz_sdk::manifests::{CompositeManifest, PoseListenerManifest, TransformManifest};
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::types::{DataType, RecordField, RecordType, TypeSchema};

use crate::api::{self, Expect};
use crate::data;
use crate::fixture::{plugin_path, rust_system_manifest, target_dir, RUST_ONLY};
use crate::support::*;

/// テストするブローカー
pub struct RemoteBroker {
    /// ブローカーの型名 (識別子の broker_type_name、conf の brokers の type_name)
    pub type_name: &'static str,
    /// プラグインの名前 (conf の plugins.broker_factories のキー)
    pub plugin_name: &'static str,
    /// qmp のテストのポートに足す数。qmp は 0、Zenoh は 200
    pub port_offset: i64,
    /// プラグインを読まずに使うための組み込みの登録 (qmp_broker::register など)
    pub register: fn(),
    /// テストから直接作る CRUD の BrokerProxy (QuicBrokerProxy::new など)。引数は `{"name": <住所>}`
    pub new_crud_proxy: fn(&Value) -> JuizResult<Box<dyn CRUDBrokerProxy>>,
}

impl RemoteBroker {
    pub fn port(&self, qmp_port: i64) -> i64 {
        qmp_port + self.port_offset
    }

    /// プラグインを読み込む設定。組み込みを使うときは空にする
    fn broker_factories(&self, use_plugin: bool) -> Value {
        let mut v = jvalue!({});
        if use_plugin {
            v[self.plugin_name] = jvalue!({ "path": plugin_path() });
        }
        v
    }

    fn broker_entry(&self, port: i64) -> Value {
        jvalue!({ "type_name": self.type_name, "name": format!("127.0.0.1:{port}"), "host": "127.0.0.1", "port": port })
    }

    /// このブローカーを持ち、[`RUST_ONLY`] の Process / Container / EC を読み込んだ System の設定
    pub fn server_manifest(
        &self,
        name: &str,
        port: i64,
        http_port: i64,
        use_plugin: bool,
    ) -> Value {
        rust_system_manifest(
            name,
            http_port,
            jvalue!([self.broker_entry(port)]),
            self.broker_factories(use_plugin),
        )
    }

    /// このブローカーの BrokerProxy だけを使う (ブローカーを持たない) System の設定
    pub fn client_manifest(&self, name: &str, use_plugin: bool) -> Value {
        jvalue!({
            "name": name,
            "option": {
                "http_broker": { "start": false }
            },
            "plugins": {
                "broker_factories": self.broker_factories(use_plugin)
            }
        })
    }

    pub fn spawn_server(&self, port: i64, http_port: i64, use_plugin: bool) -> ServerProcess {
        self.spawn_server_with_log(port, http_port, use_plugin, None)
    }

    /// juiz コマンドで [`RemoteBroker::server_manifest`] のサーバーを起動する。
    /// log を渡すと、サーバーのログ (RUST_LOG=info) をそのファイルに書く
    pub fn spawn_server_with_log(
        &self,
        port: i64,
        http_port: i64,
        use_plugin: bool,
        log: Option<&Path>,
    ) -> ServerProcess {
        let juiz = target_dir().join(format!("juiz{}", std::env::consts::EXE_SUFFIX));
        assert!(
            juiz.exists(),
            "{juiz:?} not found. Run `cargo build` before this test."
        );
        let name = format!("{}_broker_test_server", self.type_name);
        let conf = std::env::temp_dir().join(format!("{name}_{}_{port}.conf", std::process::id()));
        // JSON は YAML としても読める
        std::fs::write(
            &conf,
            self.server_manifest(&name, port, http_port, use_plugin)
                .to_string(),
        )
        .unwrap();
        let mut command = Command::new(juiz);
        command.arg("-f").arg(&conf).arg("-d").stdout(Stdio::null());
        match log {
            Some(path) => {
                let file = std::fs::File::create(path).expect("failed to create server log");
                command.env("RUST_LOG", "info").stderr(file)
            }
            None => command.stderr(Stdio::null()),
        };
        let child = command.spawn().expect("failed to start juiz server");
        ServerProcess(child)
    }

    /// System 自身のブローカーへの BrokerProxy
    fn proxy(&self, system: &System, broker_name: &str) -> JuizResult<BrokerProxyPtr> {
        system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy(self.type_name, broker_name, true)
    }

    /// 起動中のサーバーに繋がるまで待つ。起動前は接続がタイムアウトしてエラーになる
    fn wait_proxy(
        &self,
        system: &System,
        server: &mut ServerProcess,
        broker_name: &str,
    ) -> JuizResult<BrokerProxyPtr> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = server.0.try_wait()? {
                panic!("juiz server exited ({status})");
            }
            match self.proxy(system, broker_name) {
                Ok(p) => return Ok(p),
                Err(e) => {
                    assert!(
                        Instant::now() < deadline,
                        "can not connect to server: {e:?}"
                    );
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    }

    fn expect<'a>(&'a self, broker_name: &'a str) -> Expect<'a> {
        Expect {
            broker_type_name: self.type_name,
            broker_name: Some(broker_name),
        }
    }

    /// ブローカーは起動後、実際の住所を名前にする
    fn assert_broker_listed(&self, system: &System, port: i64) -> JuizResult<()> {
        let brokers = system.core_broker().lock()?.broker_list(false)?;
        let suffix = format!("/broker/127.0.0.1:{port}::{}", self.type_name);
        assert!(
            brokers.iter().any(|id| id.ends_with(&suffix)),
            "{brokers:?}"
        );
        Ok(())
    }
}

type BrokerProxyPtr = std::sync::Arc<std::sync::RwLock<dyn BrokerProxy>>;

/// テストが失敗しても juiz サーバーのプロセスを残さない
pub struct ServerProcess(pub Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// ---- ブローカー API (issue #522 の共通スイート) ----

/// 同じプロセス内: System 自身のブローカーに BrokerProxy で繋ぎ、
/// クライアント → サーバー → CRUDBroker → CoreBroker の経路で [`api::check_all`] を走らせる
pub fn broker_api_in_same_process(b: &RemoteBroker) -> JuizResult<()> {
    let port = b.port(5012);
    let broker_name = format!("localhost:{port}");
    let name = format!("{}_broker_api_test", b.type_name);
    System::new(b.server_manifest(&name, port, b.port(8032), true))?
        .setup()?
        .run_and_do_once(|system| {
            b.assert_broker_listed(system, port)?;
            let proxy = b.proxy(system, &broker_name)?;
            let mut bp = juiz_write(&proxy)?;
            api::check_all(&mut *bp, &RUST_ONLY, &b.expect(&broker_name))
        })
}

/// 別プロセス: juiz コマンドでこのブローカーを持つサーバーを起動し、
/// テスト側の System から BrokerProxy で [`api::check_all`] を走らせる
pub fn broker_api_between_processes(b: &RemoteBroker) -> JuizResult<()> {
    let port = b.port(5013);
    let mut server = b.spawn_server(port, b.port(8033), true);
    let broker_name = format!("127.0.0.1:{port}");
    let name = format!("{}_broker_test_client", b.type_name);
    System::new(b.client_manifest(&name, true))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = b.wait_proxy(system, &mut server, &broker_name)?;
            let mut bp = juiz_write(&proxy)?;
            api::check_all(&mut *bp, &RUST_ONLY, &b.expect(&broker_name))
        })
}

/// 画像・数の配列・Topic の push を、生バイトのフレーム (docs/spec/large-data.md §3) で送る (#234)。
/// Rust の関数で Process を登録するので同じプロセス内だけ。それでもクライアントとサーバーの間は
/// フレームのバイト列で受け渡す
pub fn sends_images_and_arrays(b: &RemoteBroker) -> JuizResult<()> {
    (b.register)();
    let port = b.port(5023);
    let broker_name = format!("127.0.0.1:{port}");
    let topic = format!("{}_image_topic", b.type_name);
    System::new(jvalue!({
        "name": format!("{}_broker_data_test", b.type_name),
        "option": { "http_broker": { "start": false } },
        "brokers": [ b.broker_entry(port) ]
    }))?
    .setup()?
    .run_and_do_once(|system| {
        let (_publisher, sink) = data::setup_data_processes_with_topic(system, &topic)?;
        let proxy = b.proxy(system, &broker_name)?;
        let mut bp = juiz_write(&proxy)?;
        data::check_remote_error(&mut *bp)?;
        data::check_images(&mut *bp)?;
        data::check_arrays(&mut *bp)?;
        data::check_topic_push_image(&mut *bp, &topic, &sink)?;
        // エラーの後も同じ接続で呼べる
        data::check_remote_error(&mut *bp)?;
        Ok(())
    })
}

// ---- 組み込みのブローカー (issue #63) ----
//
// 組み込みの登録が他のテストに影響しないよう、これらは別のテストバイナリから呼ぶ。

fn register_once(b: &RemoteBroker) {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(b.register);
}

/// プラグインを読み込まない設定でも、組み込みのブローカー / ブローカープロキシが使える
pub fn builtin_in_same_process(b: &RemoteBroker) -> JuizResult<()> {
    register_once(b);
    let port = b.port(5014);
    let broker_name = format!("localhost:{port}");
    let name = format!("{}_builtin_test", b.type_name);
    System::new(b.server_manifest(&name, port, b.port(8034), false))?
        .setup()?
        .run_and_do_once(|system| {
            b.assert_broker_listed(system, port)?;
            let proxy = b.proxy(system, &broker_name)?;
            let mut bp = juiz_write(&proxy)?;
            api::check_processes(&mut *bp, &RUST_ONLY, &b.expect(&broker_name))?;
            api::check_containers(&mut *bp, &RUST_ONLY)?;
            api::check_connections(&mut *bp, &RUST_ONLY)?;
            Ok(())
        })
}

/// プラグインを読み込まない設定で juiz コマンド (ブローカーを組み込んだ juiz_app) を起動し、
/// 組み込みの BrokerProxy で Operation を呼ぶ
pub fn builtin_between_processes(b: &RemoteBroker) -> JuizResult<()> {
    register_once(b);
    let port = b.port(5015);
    let mut server = b.spawn_server(port, b.port(8035), false);
    let broker_name = format!("127.0.0.1:{port}");
    let name = format!("{}_builtin_test_client", b.type_name);
    System::new(b.client_manifest(&name, false))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = b.wait_proxy(system, &mut server, &broker_name)?;
            let mut bp = juiz_write(&proxy)?;
            api::check_processes(&mut *bp, &RUST_ONLY, &b.expect(&broker_name))?;
            api::check_containers(&mut *bp, &RUST_ONLY)?;
            Ok(())
        })
}

/// 組み込みとプラグインの両方があるときは、二重登録のエラーにならず、プラグインのほうを使う
pub fn builtin_with_plugin(b: &RemoteBroker) -> JuizResult<()> {
    register_once(b);
    let port = b.port(5016);
    let broker_name = format!("localhost:{port}");
    let name = format!("{}_builtin_plugin_test", b.type_name);
    System::new(b.server_manifest(&name, port, b.port(8036), true))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = b.proxy(system, &broker_name)?;
            let mut bp = juiz_write(&proxy)?;
            api::check_processes(&mut *bp, &RUST_ONLY, &b.expect(&broker_name))
        })
}

// ---- ネームスペース (issue #150) ----
//
// ブローカーは組み込み (register) で使う。テストから直接作る CRUD の BrokerProxy と
// System が使うブローカーを同じコードにするため (プラグインの DLL と混ぜると Windows で落ちた)。

fn find_full_name<'a>(ids: &'a [ProcessIdentifier], full_name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.full_name() == full_name)
        .unwrap_or_else(|| panic!("{full_name} not found in {ids:?}"))
}

/// CRUD の BrokerProxy で list に namespace を渡し、`<namespace>/<name>` を並べ替えて返す
fn list_in_namespace(
    proxy: &dyn CRUDBrokerProxy,
    class_name: &str,
    namespace: &str,
) -> JuizResult<Vec<String>> {
    let mut param: HashMap<String, String> = HashMap::new();
    param.insert("namespace".to_owned(), namespace.to_owned());
    let v = proxy.read(class_name, "list", param)?.extract_value()?;
    let mut names: Vec<String> = v
        .as_array()
        .unwrap_or_else(|| panic!("{class_name} list is not an array: {v}"))
        .iter()
        .map(|v| match v.as_str() {
            Some(id) => {
                let s: IdentifierStruct = id.to_owned().try_into().expect("invalid identifier");
                s.full_name()
            }
            None => {
                let id: ProcessIdentifier =
                    serde_json::from_value(v.clone()).expect("invalid identifier");
                id.full_name()
            }
        })
        .collect();
    names.sort();
    Ok(names)
}

/// サブシステム越しにネームスペース付きの Process / ContainerProcess を呼べ、
/// list の param `namespace` でそのネームスペースとその下だけに絞り込める
pub fn namespaced_objects_over_subsystem(b: &RemoteBroker) -> JuizResult<()> {
    (b.register)();
    let port = b.port(5017);
    let master_port = b.port(5018);

    // このブローカーを持ち、robot1 (既定)・robot1/arm・robot2・ルートに Process を置いた System
    let mut sub = b.server_manifest(
        &format!("{}_namespace_sub", b.type_name),
        port,
        b.port(8037),
        false,
    );
    sub["namespace"] = jvalue!("robot1");
    sub["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "inc0" },
        { "type_name": "increment_process", "name": "inc1", "namespace": "arm" },
        { "type_name": "increment_process", "name": "inc0", "namespace": "/robot2" },
        { "type_name": "increment_process", "name": "inc0", "namespace": "/" }
    ]);
    sub["containers"] = jvalue!([
        { "type_name": "example_container", "name": "c0", "initial_value": 100,
          "processes": [ { "type_name": "example_container_increment", "name": "c0inc" } ] }
    ]);

    // sub をサブシステムとして登録する System。
    // 登録するとき相手に自分のブローカーを教えるので、自分も同じ種類のブローカーを持つ
    let mut master = b.client_manifest(&format!("{}_namespace_master", b.type_name), false);
    master["brokers"] = jvalue!([b.broker_entry(master_port)]);
    master["subsystems"] = jvalue!([
        { "type_name": b.type_name, "name": format!("127.0.0.1:{port}") }
    ]);

    System::new(sub)?.setup()?.run_and_do_once(|_sub| {
        System::new(master)?.setup()?.run_and_do_once(|master| {
            check_call_over_subsystem(b, master)?;
            check_list_filter(b, port)?;
            Ok(())
        })
    })
}

fn check_call_over_subsystem(b: &RemoteBroker, master: &System) -> JuizResult<()> {
    let core = master.core_broker();
    let ids = core.lock()?.process_list(true, None)?;
    // サブシステムの Process は <type>://<住所>/process/<namespace>/<name>::<type> で見える
    let r1 = find_full_name(&ids, "robot1/inc0");
    assert_eq!(r1.broker_type_name, b.type_name, "{r1:?}");
    assert_eq!(r1.namespace, "robot1");
    assert!(
        r1.to_string()
            .ends_with("/process/robot1/inc0::increment_process"),
        "{r1}"
    );
    for (full_name, arg) in [
        ("robot1/inc0", 10),
        ("robot1/arm/inc1", 20),
        ("robot2/inc0", 30),
        ("inc0", 40),
    ] {
        let id = find_full_name(&ids, full_name);
        assert_eq!(
            int_of(core.lock()?.process_call(id, args("arg1", arg))),
            arg + 1,
            "{id}"
        );
    }

    let cps = core.lock()?.container_process_list(true, None)?;
    let c0inc = find_full_name(&cps, "robot1/c0inc");
    assert_eq!(
        int_of(core.lock()?.container_process_call(c0inc, args("arg1", 1))),
        101
    );
    assert_eq!(
        int_of(core.lock()?.any_process_call(c0inc, args("arg1", 2))),
        103
    );
    Ok(())
}

fn check_list_filter(b: &RemoteBroker, port: i64) -> JuizResult<()> {
    let proxy = (b.new_crud_proxy)(&jvalue!({ "name": format!("127.0.0.1:{port}") }))?;
    let list =
        |class_name: &str, namespace: &str| list_in_namespace(&*proxy, class_name, namespace);
    assert_eq!(
        list("process", "robot1")?,
        vec!["robot1/arm/inc1", "robot1/inc0"]
    );
    assert_eq!(list("process", "/robot1/arm")?, vec!["robot1/arm/inc1"]);
    assert_eq!(list("process", "robot2")?, vec!["robot2/inc0"]);
    assert!(list("process", "robot")?.is_empty());
    assert_eq!(list("process", "")?.len(), 4);
    assert_eq!(list("container", "robot1")?, vec!["robot1/c0"]);
    assert!(list("container", "robot2")?.is_empty());
    assert_eq!(list("container_process", "robot1")?, vec!["robot1/c0inc"]);
    assert_eq!(
        list("any_process", "robot1")?,
        vec!["robot1/arm/inc1", "robot1/c0inc", "robot1/inc0"]
    );
    assert_eq!(list("execution_context", "robot1")?, vec!["robot1/timer0"]);
    assert!(list("execution_context", "robot2")?.is_empty());
    // 不正なネームスペースはエラー
    assert!(list("process", "a|b").is_err());
    Ok(())
}

// ---- Composition (issue #170, #171) ----

fn composite(v: Value) -> CompositeManifest {
    serde_json::from_value(v).expect("invalid CompositeManifest in test")
}

/// Composite Process の profile に `composition`、CompositeContainer の profile に `slots` が出る
/// (docs/spec/composition.md の §8.2)。HTTP は juiz_core の composition::display
pub fn composite_profile(b: &RemoteBroker) -> JuizResult<()> {
    let port = b.port(5019);
    let broker_name = format!("localhost:{port}");
    let mut m = b.server_manifest(
        &format!("{}_composite_test", b.type_name),
        port,
        b.port(8034),
        true,
    );
    m["composites"] = jvalue!([
        {
            "type_name": "inc_twice",
            "body": {
                "process": "increment_process",
                "args": { "arg1": { "process": "increment_process" } }
            }
        },
        {
            "type_name": "add_twice",
            "containers": { "c": "example_container" },
            "body": {
                "process": "example_container_increment",
                "container": "c",
                "args": { "arg1": { "process": "increment_process", "args": { "arg1": { "expose": "x" } } } }
            }
        }
    ]);
    m["processes"]
        .as_array_mut()
        .unwrap()
        .push(jvalue!({ "type_name": "inc_twice", "name": "t0" }));
    m["composite_containers"] = jvalue!([
        { "name": "cc0", "slots": { "c": "c0" },
          "processes": [ { "type_name": "add_twice", "name": "a0" } ] }
    ]);
    let ty = b.type_name;
    System::new(m)?.setup()?.run_and_do_once(|system| {
        let proxy = b.proxy(system, &broker_name)?;
        let bp = juiz_borrow(&proxy)?;

        let t0: ProcessIdentifier =
            format!("{ty}://{broker_name}/process/t0::inc_twice").try_into()?;
        let composition = bp
            .process_profile(&t0)?
            .manifest
            .composition
            .expect("composition");
        assert_eq!(composition.body.process, "increment_process");
        assert!(composition.body.args.contains_key("arg1"));
        // 呼び出しもブローカー越しに通る
        assert_eq!(int_of(bp.process_call(&t0, args("arg1", 1))), 3);

        let a0: ProcessIdentifier =
            format!("{ty}://{broker_name}/container_process/a0::add_twice:cc0").try_into()?;
        let composition = bp
            .container_process_profile(&a0)?
            .manifest
            .composition
            .expect("composition");
        assert_eq!(
            composition.containers.get("c").map(String::as_str),
            Some("example_container")
        );
        assert_eq!(composition.body.container.as_deref(), Some("c"));

        let cc0: ContainerIdentifier =
            format!("{ty}://{broker_name}/container/cc0::composite_container").try_into()?;
        let slots = bp.container_profile(&cc0)?.slots.expect("slots");
        assert_eq!(
            slots.get("c").map(String::as_str),
            Some("core://core/container/c0::example_container")
        );
        // 普通の Container には slots を出さない
        let c0: ContainerIdentifier =
            format!("{ty}://{broker_name}/container/c0::example_container").try_into()?;
        assert!(bp.container_profile(&c0)?.slots.is_none());
        Ok(())
    })
}

/// 実行中に composite/create・list・profile・destroy と composite_container/create を呼べる
/// (docs/spec/composition.md の §9)。HTTP は juiz_core の composition::api
pub fn composite_api(b: &RemoteBroker) -> JuizResult<()> {
    let port = b.port(5021);
    let broker_name = format!("localhost:{port}");
    let manifest = b.server_manifest(
        &format!("{}_composite_api_test", b.type_name),
        port,
        b.port(8038),
        true,
    );
    let ty = b.type_name;
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        let proxy = b.proxy(system, &broker_name)?;
        let mut bp = juiz_write(&proxy)?;

        let m = bp.composite_create(&composite(jvalue!({
            "type_name": "inc_twice",
            "body": {
                "process": "increment_process",
                "args": { "arg1": { "process": "increment_process" } }
            }
        })))?;
        assert!(m.composition.is_some());
        assert_eq!(bp.composite_list()?, vec!["inc_twice"]);
        assert_eq!(bp.composite_profile("inc_twice")?.type_name, "inc_twice");

        bp.process_create(&ProcessManifest::new("inc_twice").name("t0"))?;
        let t0: ProcessIdentifier =
            format!("{ty}://{broker_name}/process/t0::inc_twice").try_into()?;
        assert_eq!(int_of(bp.process_call(&t0, args("arg1", 1))), 3);

        // 使われている Composite は消せず、理由が返る
        let e = format!("{:#}", bp.composite_destroy("inc_twice").unwrap_err());
        assert!(e.contains("t0"), "{e}");

        bp.composite_create(&composite(jvalue!({
            "type_name": "add_twice",
            "containers": { "c": "example_container" },
            "body": {
                "process": "example_container_increment",
                "container": "c",
                "args": { "arg1": { "process": "inc_twice" } }
            }
        })))?;
        let cc = bp.composite_container_create(&jvalue!({
            "name": "cc0",
            "slots": { "c": "c0" },
            "processes": [ { "type_name": "add_twice", "name": "a0" } ]
        }))?;
        assert_eq!(
            cc.slots.expect("slots").get("c").map(String::as_str),
            Some("core://core/container/c0::example_container")
        );
        let a0: ProcessIdentifier =
            format!("{ty}://{broker_name}/container_process/a0::add_twice:cc0").try_into()?;
        let before = int_of(bp.container_process_call(&a0, args("arg1", 1)));
        let after = int_of(bp.container_process_call(&a0, args("arg1", 1)));
        assert_eq!(after - before, 3);

        let cc0: ContainerIdentifier =
            format!("{ty}://{broker_name}/container/cc0::composite_container").try_into()?;
        bp.container_destroy(&cc0)?;
        bp.composite_destroy("add_twice")?;
        bp.process_destroy(&t0)?;
        bp.composite_destroy("inc_twice")?;
        assert!(bp.composite_list()?.is_empty());
        Ok(())
    })
}

// ---- データ型 (issue #193) ----

fn twist() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Twist")
            .field(RecordField::new("linear", DataType::Double))
            .field(RecordField::new("angular", DataType::Double)),
    ))
    .unwrap()
}

fn make_twist(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!({"linear": 1.0, "angular": 0.0}).into())
}

/// 型の登録所の type/list・type/schema を引ける (docs/spec/datatype.md の §19)。
/// HTTP は juiz_core の datatype::type_check
pub fn type_api(b: &RemoteBroker) -> JuizResult<()> {
    let port = b.port(5022);
    let broker_name = format!("localhost:{port}");
    let manifest = b.server_manifest(
        &format!("{}_type_api_test", b.type_name),
        port,
        b.port(8039),
        true,
    );
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        let pf = process_factory_create(
            ProcessManifest::new("make_twist").set_output_schema(Some(twist()))?,
            make_twist,
        )?;
        system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .store_mut()
            .processes
            .register_factory("make_twist", pf)?;

        let proxy = b.proxy(system, &broker_name)?;
        let bp = juiz_borrow(&proxy)?;
        // Transformations の API の型 (juiz_std.geometry、issue #370) と Container のモデル情報の型
        // (juiz_std.model、issue #381) はいつも載るので除く
        let list: Vec<_> = bp
            .type_list()?
            .into_iter()
            .filter(|e| {
                !e.used_by
                    .iter()
                    .all(|u| u == "transformations" || u == "container_model")
            })
            .collect();
        assert_eq!(list.len(), 1, "{list:?}");
        assert_eq!(list[0].identifier, twist().identifier());
        assert_eq!(list[0].used_by, vec!["make_twist"]);
        assert_eq!(bp.type_schema(&twist().identifier())?, twist());
        assert_eq!(bp.type_schema("robot.geometry.Twist")?, twist());
        let e = format!("{:#}", bp.type_schema("robot.geometry.Pose2D").unwrap_err());
        assert!(e.contains("not found"), "{e}");
        Ok(())
    })
}

// ---- Transformations (issue #370) ----

const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

struct Part {}

fn record(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("pose")?.lock_as_value(|v| v.clone())?.into())
}

/// Anchor の作成・Transform の作成・ContainerPose の読み書き・木・Pose 通知の登録。
/// 数値は docs/spec/transformations.md §3.5 の例。HTTP とローカルブローカーは juiz_core の transformations::api
pub fn transformations_api(b: &RemoteBroker) -> JuizResult<()> {
    let port = b.port(5025);
    let broker_name = format!("localhost:{port}");
    let manifest = b.server_manifest(
        &format!("{}_transformations_api_test", b.type_name),
        port,
        b.port(8066),
        true,
    );
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        {
            let mut cb = system.core_broker().lock_mut()?;
            let w = cb.worker_mut();
            let juiz_sdk::factory::ContainerFactoryStruct(m, constructor) =
                juiz_sdk::factory::container_factory(ContainerManifest::new("part"), |_| {
                    Ok(Box::new(Part {}))
                });
            w.store_mut()
                .containers
                .register_factory("part", container_factory_create(m, constructor)?)?;
            for n in ["base0", "camera0"] {
                w.create_container_ref("part", n, Vec::<(&str, Value)>::new().into())?;
            }
            let pm = ProcessManifest::new("record").add_object_arg("pose", "", jvalue!({}));
            w.store_mut()
                .processes
                .register_factory("record", process_factory_create(pm, record)?)?;
            w.create_process_ref(&ProcessManifest::new("record").name("rec0"))?;
        }

        let proxy = b.proxy(system, &broker_name)?;
        let mut bp = juiz_write(&proxy)?;
        let z90 = Quaternion::new(0.0, 0.0, H, H);

        let profile = bp.container_create(
            &ContainerManifest::new("periodic_anchor").name("map"),
            CapsuleMap::new(),
        )?;
        assert_eq!(
            profile.anchor,
            Some(jvalue!({"kind": "periodic", "rate": 10.0}))
        );
        bp.transform_create(
            &TransformManifest::new("map", "base0")
                .translation(Vector3::new(1.0, 0.0, 0.0))
                .rotation(z90),
        )?;
        bp.transform_create(
            &TransformManifest::new("base0", "camera0").translation(Vector3::new(0.1, 0.0, 0.3)),
        )?;
        assert_eq!(bp.transform_list()?.len(), 2);
        assert_eq!(
            bp.anchor_list()?,
            vec!["core://core/container/map::periodic_anchor"]
        );

        let p = bp.container_pose("camera0", "map")?;
        assert!(
            p.pose
                .approx_eq(&Pose::new(Vector3::new(1.0, 0.1, 0.3), z90), 1e-9),
            "{p:?}"
        );
        let w = bp.container_set_pose(
            "camera0",
            "map",
            &Pose::new(Vector3::new(1.0, 0.2, 0.3), z90),
        )?;
        assert!(w
            .pose
            .approx_eq(&Pose::new(Vector3::new(1.0, 0.2, 0.3), z90), 1e-9));
        let tid =
            "transform://core://core/container/base0::part|core://core/container/camera0::part";
        let t = bp.transform_update(tid, Some(Vector3::new(0.1, 0.0, 0.3)), None)?;
        assert_eq!(t["transform"]["translation"]["x"], jvalue!(0.1));
        bp.anchor_update("map")?;
        let tree = bp.transformations_tree("map")?;
        assert_eq!(
            tree["children"][0]["children"][0]["identifier"],
            jvalue!("core://core/container/camera0::part")
        );

        let l = bp.pose_listener_create(&PoseListenerManifest::new("camera0", "rec0"))?;
        let lid = l["identifier"].as_str().unwrap().to_owned();
        assert_eq!(bp.pose_listener_list()?, vec![lid.clone()]);
        assert_eq!(bp.pose_listener_profile(&lid)?["arg"], jvalue!("pose"));
        bp.pose_listener_destroy(&lid)?;
        assert!(bp.pose_listener_list()?.is_empty());

        bp.transform_destroy(tid)?;
        let e = format!("{:#}", bp.container_pose("camera0", "map").unwrap_err());
        assert!(e.contains("No anchor contains"), "{e}");
        Ok(())
    })
}

// ---- Container の 3D モデル (issue #381) ----

/// テストの間だけある作業ディレクトリ (models/ を作っておく)
pub struct TempDir(pub std::path::PathBuf);

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        let p = std::env::temp_dir().join(format!("juiz_{name}_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(p.join("models")).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// payload_len バイトの中身を持つ .glb のバイト列
pub fn glb_bytes(payload_len: usize) -> Vec<u8> {
    let mut v = b"glTF".to_vec();
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&((12 + payload_len) as u32).to_le_bytes());
    v.extend((0..payload_len).map(|i| (i % 251) as u8));
    v
}

impl RemoteBroker {
    /// models/pantilt.glb をモデルに持つ pantilt0 と、モデルのない plain を置いた System の設定
    pub fn manifest_with_models(&self, name: &str, port: i64, http_port: i64, http: bool) -> Value {
        let mut m = self.server_manifest(name, port, http_port, true);
        m["option"]["http_broker"]["start"] = jvalue!(http);
        m["containers"] = jvalue!([
            { "type_name": "static_anchor", "name": "pantilt0", "model": { "path": "models/pantilt.glb" } },
            { "type_name": "static_anchor", "name": "plain" },
        ]);
        m
    }
}

/// `container/model_info`・`container/model` (`{info, data: <base64>}`)・`container/profile` の `model`
/// (docs/spec/container-model.md §6.3)。HTTP とローカルブローカーは juiz_core の container_model::core
pub fn container_model(b: &RemoteBroker) -> JuizResult<()> {
    let dir = TempDir::new(&format!("{}_container_model", b.type_name));
    let glb = glb_bytes(1000);
    std::fs::write(dir.0.join("models/pantilt.glb"), &glb)?;
    let port = b.port(5026);
    let broker_name = format!("localhost:{port}");
    let manifest = b.manifest_with_models(
        &format!("{}_container_model_test", b.type_name),
        port,
        b.port(8069),
        false,
    );
    System::new(manifest)?
        .set_working_dir(&dir.0)
        .setup()?
        .run_and_do_once(|system| {
            let proxy = b.proxy(system, &broker_name)?;
            let bp = juiz_borrow(&proxy)?;
            let info = bp.container_model_info("pantilt0")?;
            assert_eq!(info["format"], "glb");
            assert_eq!(info["sha256"], sha256_hex(&glb));
            assert_eq!(info["file_name"], "pantilt.glb");
            let d = ContainerModelData::from_value(&bp.container_model("pantilt0")?)?;
            assert_eq!(d.data, glb);
            assert_eq!(d.info, info);
            assert_eq!(bp.container_model_info("plain")?, Value::Null);
            let e = bp.container_model("plain").unwrap_err();
            assert!(format!("{e:#}").contains("has no model"), "{e:#}");
            let id = ContainerIdentifier::try_from(
                "core://core/container/pantilt0::static_anchor".to_owned(),
            )?;
            assert_eq!(bp.container_profile(&id)?.model, Some(info));
            Ok(())
        })
}
