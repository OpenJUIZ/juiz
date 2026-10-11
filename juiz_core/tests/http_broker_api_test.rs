//! Rust / Python / C++ のプロセス・コンテナを読み込んだ System に、
//! HTTP ブローカー経由の BrokerProxy でアクセスするテスト
//!
//! System 自身の HTTP サーバに CRUDBrokerProxy で繋ぎ、
//! HTTP クライアント → HTTP ルータ → CRUDBroker → CoreBroker の経路を通す。

mod broker_api;

use broker_api::*;
use juiz_core::prelude::*;

const PORT: i64 = 8012;

fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

fn check_system(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    assert!(bp.system_profile_full()?.is_object());
    assert!(bp.system_uuid()?.as_str().is_some());
    bp.system_filesystem_list(std::path::PathBuf::from("."))?;

    let procs = bp.process_list(false, None)?;
    assert_eq!(procs.len(), 3);
    // HTTP 経由で得た識別子は HTTP ブローカーを指す
    assert!(
        procs.iter().all(|id| id.broker_type_name == "http"),
        "{procs:?}"
    );
    assert_eq!(bp.container_list(false, None)?.len(), 3);
    assert_eq!(bp.container_process_list(false, None)?.len(), 6);
    assert_eq!(bp.any_process_list(false, None)?.len(), 9);
    Ok(())
}

fn check_processes(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    for (name, _type_name) in PROCESSES {
        let id = find_process(&ids, name);
        assert_eq!(int_of(bp.process_call(id, args("arg1", 10))), 11, "{name}");
        assert_eq!(
            int_of(bp.any_process_call(id, args("arg1", 20))),
            21,
            "{name}"
        );
        assert_eq!(
            int_of(bp.process_push_by(id, "arg1".to_owned(), jvalue!(7).into())),
            8,
            "{name}"
        );
        bp.process_p_apply(id, "arg1", jvalue!(1).into())?;
        assert!(try_int_of(&bp.process_execute(id)?).is_some(), "{name}");
    }
    Ok(())
}

fn check_containers(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
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
        assert_eq!(
            int_of(bp.any_process_call(get, CapsuleMap::new())),
            before + 4,
            "{container_name}"
        );
        bp.container_process_p_apply(add, arg_name, jvalue!(1).into())?;
        assert!(
            try_int_of(&bp.container_process_execute(add)?).is_some(),
            "{container_name}"
        );
    }
    Ok(())
}

fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ecs = bp.ec_list(false)?;
    let ids: Vec<String> = ecs
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let id = ids
        .iter()
        .find(|id| id.contains("timer0"))
        .expect("timer0 not listed")
        .clone();
    let state = |bp: &mut dyn BrokerProxy| -> JuizResult<Value> {
        Ok(bp.ec_get_state(&id)?["__value__"].clone())
    };

    assert_eq!(
        bp.ec_profile_full(&id)?["__value__"]["name"],
        jvalue!("timer0")
    );
    assert_eq!(state(bp)?, jvalue!("STOPPED"));
    bp.ec_start(&id)?;
    let mut started = false;
    for _ in 0..200 {
        if state(bp)? == jvalue!("STARTED") {
            started = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(started);
    bp.ec_stop(&id)?;
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
    Ok(())
}

#[test]
fn http_broker_api_with_rust_python_cpp_plugins() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(system_manifest("http_broker_api_test", PORT))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system.core_broker().lock()?.worker().broker_proxy(
                "http",
                format!("localhost:{PORT}").as_str(),
                true,
            )?;
            let mut bp = juiz_lock(&proxy)?;
            check_system(&mut *bp)?;
            check_processes(&mut *bp)?;
            check_containers(&mut *bp)?;
            check_ec(&mut *bp)?;
            check_topics(&mut *bp)?;
            Ok(())
        })
}
