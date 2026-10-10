//! Rust / Python / C++ のプロセス・コンテナを読み込んだ System に対して、
//! CoreBroker の BrokerProxy API を一通り呼び出すテスト

mod broker_api;

use broker_api::*;
use juiz_core::prelude::*;
use juiz_sdk::connection_identifier::ConnectionIdentifier;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

fn check_system(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let profile = bp.system_profile_full()?;
    assert_eq!(obj_get_str(&profile, "name")?, "core");
    assert!(profile.get("core_store").is_some());
    assert!(bp.system_uuid()?.as_str().is_some());
    assert!(bp.system_filesystem_list(std::path::PathBuf::from("."))?.is_array());

    assert_eq!(bp.process_list(false, None)?.len(), 3);
    assert_eq!(bp.container_list(false, None)?.len(), 3);
    assert_eq!(bp.container_process_list(false, None)?.len(), 6);
    assert_eq!(bp.any_process_list(false, None)?.len(), 9);
    assert_eq!(bp.ec_list(false)?.as_array().unwrap().len(), 1);
    assert!(bp.connection_list(false)?.is_empty());
    assert!(bp.topic_list()?.is_empty());

    let brokers = bp.broker_list(false)?;
    assert_eq!(brokers.len(), 2, "{brokers:?}");
    for b in brokers.iter() {
        let p = bp.broker_profile_full(b)?;
        assert!(!p.name.is_empty());
    }
    assert!(bp.broker_profile_full(&"core://core/broker/none::none".to_owned()).is_err());
    Ok(())
}

fn check_processes(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    for (name, type_name) in PROCESSES {
        let id = find_process(&ids, name);
        let profile = bp.process_profile_full(id)?;
        assert_eq!((profile.name.as_str(), profile.type_name.as_str()), (name, type_name));
        assert_eq!(bp.any_process_profile_full(id)?.name, name);

        assert_eq!(int_of(bp.process_call(id, args("arg1", 10))), 11, "{name}");
        assert_eq!(int_of(bp.any_process_call(id, args("arg1", 20))), 21, "{name}");
        // push_by は引数を設定して実行する
        assert_eq!(int_of(bp.process_push_by(id, "arg1".to_owned(), jvalue!(7).into())), 8, "{name}");
        // p_apply は引数を設定するだけで、execute で反映される
        bp.process_p_apply(id, "arg1", jvalue!(30).into())?;
        assert_eq!(int_of(bp.process_execute(id)), 31, "{name}");
        // 存在しない引数は設定できない
        assert!(bp.process_p_apply(id, "no_such_arg", jvalue!(1).into()).is_err());
    }

    let unknown: ProcessIdentifier = "core://core/process/none::increment_process".try_into()?;
    assert!(bp.process_profile_full(&unknown).is_err());
    assert!(bp.process_call(&unknown, args("arg1", 1)).is_err());
    assert!(bp.process_execute(&unknown).is_err());
    Ok(())
}

fn check_containers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.container_process_list(false, None)?;
    for (container_name, add_name, arg_name, get_name) in CONTAINERS {
        let add = find_process(&ids, add_name);
        let get = find_process(&ids, get_name);
        assert_eq!(bp.container_process_profile_full(add)?.name, add_name);
        assert_eq!(bp.any_process_profile_full(get)?.name, get_name);

        let before = int_of(bp.container_process_call(get, CapsuleMap::new()));
        assert_eq!(int_of(bp.container_process_call(add, args(arg_name, 3))), before + 3, "{container_name}");
        assert_eq!(int_of(bp.any_process_call(get, CapsuleMap::new())), before + 3, "{container_name}");
        bp.container_process_p_apply(add, arg_name, jvalue!(2).into())?;
        assert_eq!(int_of(bp.container_process_execute(add)), before + 5, "{container_name}");
        assert_eq!(int_of(bp.container_process_execute(get)), before + 5, "{container_name}");
        // get には引数がない
        assert!(bp.container_process_p_apply(get, arg_name, jvalue!(1).into()).is_err());
    }

    let mut names = vec![];
    for id in bp.container_list(false, None)? {
        let profile = bp.container_profile_full(&id)?;
        names.push(profile.name);
    }
    names.sort();
    assert_eq!(names, vec!["c0", "cppc0", "pyc0"]);
    Ok(())
}

/// EC の状態はスレッド側で切り替わるので、しばらく待つ
fn wait_ec_state(bp: &mut dyn BrokerProxy, id: &Identifier, state: &str) -> JuizResult<bool> {
    for _ in 0..200 {
        if bp.ec_get_state(id)? == jvalue!(state) {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Ok(false)
}

fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ecs = bp.ec_list(false)?;
    let id = ecs.as_array().unwrap()[0].as_str().unwrap().to_owned();
    let profile = bp.ec_profile_full(&id)?;
    assert_eq!(obj_get_str(&profile, "name")?, "timer0");
    assert_eq!(bp.ec_get_state(&id)?, jvalue!("STOPPED"));
    bp.ec_start(&id)?;
    assert!(wait_ec_state(bp, &id, "STARTED")?);
    bp.ec_stop(&id)?;
    assert!(wait_ec_state(bp, &id, "STOPPED")?);

    let created = bp.ec_create(&jvalue!({"type_name": "TimerEC", "name": "timer1", "rate": 0.01}))?;
    assert_eq!(obj_get_str(&created, "name")?, "timer1");
    assert_eq!(bp.ec_list(false)?.as_array().unwrap().len(), 2);
    assert!(bp.ec_create(&jvalue!({"type_name": "NoSuchEC", "name": "x"})).is_err());
    assert!(bp.ec_get_state(&"core://core/execution_context/none::TimerEC".to_owned()).is_err());
    Ok(())
}

fn check_create_connect_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    // 型ごとに新しいプロセスを作る
    let created = bp.process_create(&ProcessManifest::new("increment_process").name("inc_new"))?;
    assert_eq!(created.name, "inc_new");
    let dst = created.identifier();
    assert_eq!(bp.process_list(false, None)?.len(), 4);
    assert_eq!(int_of(bp.process_call(&dst, args("arg1", 1))), 2);
    assert!(bp.process_create(&ProcessManifest::new("no_such_type").name("x")).is_err());

    // inc0 の出力を inc_new の arg1 に Pull で繋ぐ
    let src = find_process(&bp.process_list(false, None)?, "inc0").clone();
    let cm = ConnectionManifest::new(ConnectionType::Pull, src.clone(), "arg1".to_owned(), dst.clone());
    let con: ConnectionIdentifier = bp.connection_create(&cm)?.into();
    let cons = bp.connection_list(false)?;
    assert_eq!(cons.len(), 1);
    assert_eq!(bp.connection_profile_full(&con)?.arg_name, "arg1");

    // inc_new を実行すると inc0 の出力を pull して使う
    let src_out = int_of(bp.process_execute(&src));
    assert_eq!(int_of(bp.process_execute(&dst)), src_out + 1);

    let destroyed = bp.process_destroy(&dst)?;
    assert_eq!(destroyed.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), 3);
    assert!(bp.process_call(&dst, args("arg1", 1)).is_err());

    // コンテナを作り、そこにコンテナプロセスを追加する
    let cprofile = bp.container_create(
        &ContainerManifest::new("example_container").name("c_new"),
        vec![("name", jvalue!("c_new")), ("initial_value", jvalue!(5))].into(),
    )?;
    assert_eq!(cprofile.name, "c_new");
    let cid = cprofile.identifier();
    assert_eq!(bp.container_list(false, None)?.len(), 4);
    assert_eq!(bp.container_profile_full(&cid)?.name, "c_new");
    let pprofile = bp.container_process_create(&cid, &ProcessManifest::new("example_container_get").name("c_new_get"))?;
    assert_eq!(pprofile.name, "c_new_get");
    let cprocs = bp.container_process_list(false, None)?;
    assert_eq!(cprocs.len(), 7);
    let pid = find_process(&cprocs, "c_new_get").clone();
    assert_eq!(int_of(bp.container_process_call(&pid, CapsuleMap::new())), 5);
    assert!(bp.container_process_create(&cid, &ProcessManifest::new("no_such_type").name("x")).is_err());
    Ok(())
}

fn check_topics(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let uuid = Some(uuid::Uuid::new_v4());
    // ローカルにサブスクライバもパブリッシャもいないので購読・配信は要求されない
    assert_eq!(bp.topic_request_subscribe("topic0", uuid)?, jvalue!({"subscribe": false}));
    assert_eq!(bp.topic_request_publish("topic0", uuid)?, jvalue!({"publish": false}));
    // 存在しないトピックには push できない
    assert!(bp.topic_push("topic0", jvalue!(1).into(), None).is_err());
    Ok(())
}

#[test]
fn core_broker_api_with_rust_python_cpp_plugins() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(system_manifest("core_broker_api_test", 8011))?
        .setup()?
        .run_and_do_once(|system| {
            let mut cb = system.core_broker().lock_mut()?;
            check_system(&mut *cb)?;
            check_processes(&mut *cb)?;
            check_containers(&mut *cb)?;
            check_ec(&mut *cb)?;
            check_create_connect_destroy(&mut *cb)?;
            check_topics(&mut *cb)?;
            Ok(())
        })
}
