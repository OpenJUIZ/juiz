//! メッセンジャー系ブローカーの BrokerProxy (MessengerBrokerProxy) と ProcessProxy のテスト (issue #221)
//!
//! local_broker_api_test が通していない API (call / execute、コンテナプロセス、Topic、Composite、型、
//! プラグインの読み込みなど) をローカルブローカー越しに呼ぶ。
//! ProcessProxy は、BrokerProxy を包んで他の System のプロセスを手元のプロセスのように扱う部品で、
//! ローカルブローカーと HTTP ブローカーの両方で call / profile / push_by / p_apply などを確かめる。

use std::sync::{Arc, Mutex};

use crate::broker_api;

use broker_api::*;
use juiz_core::prelude::*;
use juiz_sdk::{manifests::CompositeManifest, process_identifier::ProcessIdentifier};

const PORT: i64 = 8025;

fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

fn local_proxy(system: &System) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
    system
        .core_broker()
        .lock()?
        .worker()
        .broker_proxy("local", "local", true)
}

fn http_proxy(system: &System) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
    system.core_broker().lock()?.worker().broker_proxy(
        "http",
        format!("localhost:{PORT}").as_str(),
        true,
    )
}

fn check_system(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let profile = bp.system_profile_full_recursive()?;
    assert!(profile.is_object(), "{profile:?}");
    // recursive のときは各 System の接続も入る
    assert!(profile["connections"].is_array(), "{profile:?}");
    Ok(())
}

fn check_process_call_execute(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    for (name, _type_name) in PROCESSES {
        let id = find_process(&ids, name);
        assert_eq!(int_of(bp.process_call(id, args("arg1", 10))), 11, "{name}");
        bp.process_p_apply(id, "arg1", jvalue!(2).into())?;
        assert_eq!(int_of(bp.process_execute(id)), 3, "{name}");
        let spec = bp.process_openapi_spec(id)?;
        assert!(spec.is_object(), "{name}: {spec:?}");
    }
    Ok(())
}

fn check_container_processes(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.container_process_list(false, None)?;
    for (container_name, add_name, arg_name, get_name) in CONTAINERS {
        let add = find_process(&ids, add_name);
        let get = find_process(&ids, get_name);
        let before = int_of(bp.container_process_call(get, CapsuleMap::new()));
        assert_eq!(
            int_of(bp.container_process_call(add, args(arg_name, 4))),
            before + 4,
            "{container_name}"
        );
        bp.container_process_p_apply(add, arg_name, jvalue!(1).into())?;
        assert_eq!(
            int_of(bp.container_process_execute(add)),
            before + 5,
            "{container_name}"
        );
        assert!(bp.container_process_openapi_spec(get)?.is_object());
    }
    Ok(())
}

fn check_ec_start_stop(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ecs = bp.ec_list(false)?;
    let id = ecs["__value__"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .find(|id| id.contains("timer0"))
        .expect("timer0 not listed");
    let wait_for = |bp: &mut dyn BrokerProxy, expected: &str| -> JuizResult<bool> {
        for _ in 0..200 {
            if bp.ec_get_state(&id)?["__value__"] == jvalue!(expected) {
                return Ok(true);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(false)
    };
    bp.ec_start(&id)?;
    assert!(wait_for(bp, "STARTED")?);
    bp.ec_stop(&id)?;
    assert!(wait_for(bp, "STOPPED")?);
    Ok(())
}

fn check_topics(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let uuid = Some(uuid::Uuid::new_v4());
    assert_eq!(
        bp.topic_request_subscribe("topic0", uuid)?,
        jvalue!({"subscribe": false})
    );
    assert_eq!(
        bp.topic_request_publish("topic0", uuid)?,
        jvalue!({"publish": false})
    );
    // 無い Topic への push はエラーになる
    assert!(bp.topic_push("topic0", jvalue!(1).into(), uuid).is_err());
    Ok(())
}

/// サーバ側で失敗したら、タイムアウト (3 秒) を待たずにサーバ側のエラーが返る (issue #231)
fn check_remote_error(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let missing: ProcessIdentifier =
        "core://core/process/missing0::increment_process".try_into()?;
    let started = std::time::Instant::now();
    let err = bp
        .process_profile_full(&missing)
        .expect_err("profile of missing process must fail");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "error took {:?}",
        started.elapsed()
    );
    match err.downcast_ref::<JuizError>() {
        Some(JuizError::BrokerProxyRemoteError {
            class_name,
            function_name,
            message,
        }) => {
            assert_eq!(
                (class_name.as_str(), function_name.as_str()),
                ("process", "profile_full")
            );
            assert!(message.contains("missing0"), "{message}");
        }
        _ => panic!("unexpected error: {err:?}"),
    }

    let started = std::time::Instant::now();
    assert!(bp
        .topic_push("topic_missing", jvalue!(1).into(), None)
        .is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(1));

    // エラーのあとも同じ BrokerProxy で続けて呼べる
    assert!(!bp.process_list(false, None)?.is_empty());
    Ok(())
}

fn check_is_in_charge_for_process(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let id = find_process(&ids, "inc0");
    assert!(bp.is_in_charge_for_process(&id.to_string())?);
    assert!(!bp
        .is_in_charge_for_process(&"core://core/process/missing0::increment_process".to_owned())?);
    Ok(())
}

fn check_connection_by_try_connect_to(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let src = find_process(&ids, "inc0").clone();
    let dst = find_process(&ids, "cppinc0").clone();
    let cm = ConnectionManifest::new(ConnectionType::Pull, src, "arg1".to_owned(), dst);
    let connected = bp.process_try_connect_to(&cm)?;
    assert_eq!(connected.arg_name, "arg1");
    let cons = bp.connection_list(false)?;
    assert_eq!(cons.len(), 1, "{cons:?}");
    bp.connection_destroy(&cons[0])?;
    assert!(bp.connection_list(false)?.is_empty());
    Ok(())
}

fn composite(v: Value) -> CompositeManifest {
    serde_json::from_value(v).expect("invalid CompositeManifest in test")
}

fn check_composite(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let inc_twice = composite(jvalue!({
        "type_name": "inc_twice",
        "body": {
            "process": "increment_process",
            "args": { "arg1": { "process": "increment_process", "args": { "arg1": { "expose": "x" } } } }
        }
    }));
    assert_eq!(bp.composite_create(&inc_twice)?.type_name, "inc_twice");
    assert_eq!(bp.composite_list()?, vec!["inc_twice"]);
    assert_eq!(
        bp.composite_profile_full("inc_twice")?
            .composition
            .expect("composition")
            .body
            .process,
        "increment_process"
    );

    bp.process_create(&ProcessManifest::new("inc_twice").name("i0"))?;
    let i0: ProcessIdentifier = "core://core/process/i0::inc_twice".try_into()?;
    assert_eq!(int_of(bp.process_call(&i0, args("x", 3))), 5);
    bp.process_destroy(&i0)?;
    assert_eq!(bp.composite_destroy("inc_twice")?.type_name, "inc_twice");
    assert!(bp.composite_list()?.is_empty());

    // スロットに既存のコンテナを入れた CompositeContainer を作り、その中の Composite を呼ぶ
    bp.composite_create(&composite(jvalue!({
        "type_name": "add_chain",
        "containers": { "a": "example_container", "b": "example_container" },
        "body": {
            "process": "example_container_increment",
            "container": "b",
            "args": { "arg1": { "process": "example_container_increment", "container": "a" } }
        }
    })))?;
    let container = bp.composite_container_create(&jvalue!({
        "name": "cc0",
        "slots": { "a": "c0", "b": "c0" },
        "processes": [ { "type_name": "add_chain", "name": "chain0" } ]
    }))?;
    assert_eq!(container.name, "cc0");
    let chain0: ProcessIdentifier =
        "core://core/container_process/chain0::add_chain:cc0".try_into()?;
    assert!(int_of(bp.container_process_call(&chain0, args("arg1", 1))) > 0);
    let mut names = vec![];
    for id in bp.container_list(false, None)? {
        names.push(bp.container_profile_full(&id)?.name);
    }
    assert!(names.iter().any(|n| n == "cc0"), "{names:?}");
    Ok(())
}

fn check_types(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let list = bp.type_list()?;
    if let Some(entry) = list.first() {
        let schema = bp.type_schema(&entry.identifier)?;
        assert_eq!(schema.identifier(), entry.identifier);
    }
    Ok(())
}

fn check_load_plugins(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    // EC の API と同じく、Value を返す読み込みの API も __value__ で包んだまま返す
    let p = bp.system_load_process("rust".to_owned(), "../target/debug/libtalker.so".to_owned())?;
    assert_eq!(p["__value__"]["type_name"], jvalue!("talker"), "{p:?}");
    let p = bp.system_load_process(
        "python".to_owned(),
        "tests/python/py_bench_identity.py".to_owned(),
    )?;
    assert_eq!(
        p["__value__"]["type_name"],
        jvalue!("py_bench_identity"),
        "{p:?}"
    );
    let c = bp.system_load_container("python".to_owned(), "tests/python/py_counter.py".to_owned());
    // 同じ型名の Factory は二重に登録できない
    assert!(c.is_err() || c?["__value__"]["type_name"] == jvalue!("py_counter"));
    let m = bp.system_load_component(
        "rust".to_owned(),
        "../target/debug/libnative_pose_component.so".to_owned(),
    )?;
    assert_eq!(m.type_name, "native_pose_component");
    Ok(())
}

/// ProcessProxy は BrokerProxy を通して相手のプロセスを操作する
fn check_process_proxy(bp: Arc<Mutex<dyn BrokerProxy>>) -> JuizResult<()> {
    let (ids, cids) = {
        let b = juiz_lock(&bp)?;
        (
            b.process_list(false, None)?,
            b.container_process_list(false, None)?,
        )
    };
    let id = find_process(&ids, "inc0").clone();
    let proxy = ProcessProxy::new(
        JuizObjectClass::Process("ProcessProxy"),
        id.clone(),
        bp.clone(),
    )?;
    assert_eq!(proxy.identifier(), id);
    assert!(format!("{:?}", &*proxy.lock()?).contains("ProcessProxy"));
    assert_eq!(int_of(proxy.lock()?.call(args("arg1", 1))), 2);
    assert_eq!(proxy.lock()?.profile()?.name, "inc0");
    assert_eq!(int_of(proxy.lock()?.push_by("arg1", jvalue!(5).into())), 6);
    proxy.lock_mut()?.p_apply("arg1", jvalue!(1).into())?;
    assert!(proxy.lock()?.openapi_spec()?.is_object());

    // 相手側でのコネクションの作成も BrokerProxy に任せる
    let dst_id = find_process(&ids, "pyinc0").clone();
    let dst = ProcessProxy::new(
        JuizObjectClass::Process("ProcessProxy"),
        dst_id.clone(),
        bp.clone(),
    )?;
    let cm = ConnectionManifest::new(ConnectionType::Pull, id.clone(), "arg1".to_owned(), dst_id);
    let connected = proxy.lock_mut()?.try_connect_to(dst.clone(), &cm)?;
    assert_eq!(connected.arg_name, "arg1");
    let cons = juiz_lock(&bp)?.connection_list(false)?;
    assert_eq!(cons.len(), 1, "{cons:?}");
    juiz_lock(&bp)?.connection_destroy(&cons[0])?;

    // 識別子を自分自身の HTTP の識別子のままにすると、相手側が自分宛てに問い合わせ直して詰まるので、
    // 相手側の System の中の識別子 (core) にして渡す
    let mut notified = cm.clone();
    for id in [
        &mut notified.source_process_id,
        &mut notified.destination_process_id,
    ] {
        id.broker_type_name = "core".to_owned();
        id.broker_name = "core".to_owned();
    }
    let profile = dst
        .lock_mut()?
        .notify_connected_from(proxy.clone(), &notified)?;
    assert_eq!(profile.arg_name, "arg1");
    let cons = juiz_lock(&bp)?.connection_list(false)?;
    juiz_lock(&bp)?.connection_destroy(&cons[0])?;

    // ContainerProcess の ProcessProxy
    let get = find_process(&cids, "c0get").clone();
    let cproxy = ProcessProxy::new(
        JuizObjectClass::ContainerProcess("ProcessProxy"),
        get,
        bp.clone(),
    )?;
    assert!(int_of(cproxy.lock()?.call(CapsuleMap::new())) >= 0);
    assert!(cproxy.lock()?.openapi_spec()?.is_object());

    // Process と ContainerProcess 以外は ProcessProxy にできない
    assert!(ProcessProxy::new(JuizObjectClass::Container("ProcessProxy"), id, bp).is_err());
    Ok(())
}

#[test]
fn messenger_broker_proxy_api() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest("messenger_broker_proxy_test", PORT))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = local_proxy(system)?;
            {
                let mut bp = juiz_lock(&proxy)?;
                check_system(&mut *bp)?;
                check_process_call_execute(&mut *bp)?;
                check_container_processes(&mut *bp)?;
                check_ec_start_stop(&mut *bp)?;
                check_topics(&mut *bp)?;
                check_remote_error(&mut *bp)?;
                check_is_in_charge_for_process(&mut *bp)?;
                check_connection_by_try_connect_to(&mut *bp)?;
                check_composite(&mut *bp)?;
                check_types(&mut *bp)?;
                check_load_plugins(&mut *bp)?;
            }
            check_process_proxy(proxy)?;
            check_process_proxy(http_proxy(system)?)?;
            Ok(())
        })
}
