//! Rust / Python / C++ のプロセス・コンテナを読み込んだ System に、
//! ローカルブローカー経由の BrokerProxy でアクセスするテスト
//!
//! ローカルブローカーはメッセンジャー系ブローカー (MessengerBroker / MessengerBrokerProxy) の実装で、
//! MessengerBrokerProxy → チャンネル → MessengerBroker → CRUDBroker → CoreBroker の経路を通す。
//! IPC などのメッセンジャー系ブローカーのクライアント側はすべて MessengerBrokerProxy を使う。

use crate::broker_api;

use broker_api::*;
use juiz_core::prelude::*;

const PORT: i64 = 8013;

fn check_system(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    assert!(bp.system_profile_full()?.is_object());
    // サブシステム登録では UUID を文字列として読むので、包まずに返すこと
    assert!(bp.system_uuid()?.as_str().is_some());
    bp.system_filesystem_list(std::path::PathBuf::from("."))?;
    Ok(())
}

fn check_lists_and_profiles(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    // メッセンジャー経由の応答をそのまま型に読み込めること (issue #36)
    let ids = bp.process_list(false, None)?;
    assert_eq!(ids.len(), 3, "{ids:?}");
    for (name, type_name) in PROCESSES {
        let id = find_process(&ids, name);
        let profile = bp.process_profile_full(id)?;
        assert_eq!(
            (profile.name.as_str(), profile.type_name.as_str()),
            (name, type_name)
        );
    }

    let ids = bp.container_process_list(false, None)?;
    assert_eq!(ids.len(), 6, "{ids:?}");
    for (_container_name, add_name, _arg_name, get_name) in CONTAINERS {
        assert_eq!(
            bp.container_process_profile_full(find_process(&ids, add_name))?
                .name,
            add_name
        );
        assert_eq!(
            bp.container_process_profile_full(find_process(&ids, get_name))?
                .name,
            get_name
        );
    }

    let mut names = vec![];
    for id in bp.container_list(false, None)? {
        names.push(bp.container_profile_full(&id)?.name);
    }
    names.sort();
    assert_eq!(names, vec!["c0", "cppc0", "pyc0"]);

    let broker_ids = bp.broker_list(false)?;
    assert!(
        broker_ids
            .iter()
            .any(|id| id.ends_with("/broker/local::local")),
        "{broker_ids:?}"
    );
    for id in broker_ids.iter() {
        let profile = bp.broker_profile_full(id)?;
        assert_eq!(
            profile.name,
            IdentifierStruct::try_from(id.clone())?.object_name
        );
    }
    assert!(bp.topic_list()?.is_empty());
    Ok(())
}

fn check_processes(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    for (name, _type_name) in PROCESSES {
        let id = find_process(&ids, name);
        assert_eq!(
            int_of(bp.process_push_by(id, "arg1".to_owned(), jvalue!(7).into())),
            8,
            "{name}"
        );
        // 応答の印 (function_name / class_name) は受け取った側で外す (#277)
        assert_no_options(bp.process_call(id, CapsuleMap::new()));
    }
    Ok(())
}

fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    // Value を返す EC の API は CRUD 側と同じく __value__ で包んだまま返す
    let ecs = bp.ec_list(false)?;
    let id = ecs["__value__"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .find(|id| id.contains("timer0"))
        .expect("timer0 not listed");
    assert_eq!(
        bp.ec_profile_full(&id)?["__value__"]["name"],
        jvalue!("timer0")
    );
    assert_eq!(bp.ec_get_state(&id)?["__value__"], jvalue!("STOPPED"));
    Ok(())
}

fn check_connections(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let src = find_process(&ids, "inc0").clone();
    let dst = find_process(&ids, "pyinc0").clone();
    let cm = ConnectionManifest::new(ConnectionType::Pull, src, "arg1".to_owned(), dst);
    assert_eq!(bp.connection_create(&cm)?.arg_name, "arg1");
    let cons = bp.connection_list(false)?;
    assert_eq!(cons.len(), 1, "{cons:?}");
    assert_eq!(bp.connection_profile_full(&cons[0])?.arg_name, "arg1");
    assert_eq!(bp.connection_destroy(&cons[0])?.arg_name, "arg1");
    assert!(bp.connection_list(false)?.is_empty());
    Ok(())
}

fn check_create_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let created = bp.process_create(&ProcessManifest::new("increment_process").name("inc_new"))?;
    assert_eq!(created.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), 4);
    assert_eq!(bp.process_destroy(&created.identifier())?.name, "inc_new");
    assert_eq!(bp.process_list(false, None)?.len(), 3);

    let cprofile = bp.container_create(
        &ContainerManifest::new("example_container").name("c_new"),
        vec![("name", jvalue!("c_new")), ("initial_value", jvalue!(5))].into(),
    )?;
    assert_eq!(cprofile.name, "c_new");
    let cid = cprofile.identifier();
    assert_eq!(bp.container_list(false, None)?.len(), 4);
    let pprofile = bp.container_process_create(
        &cid,
        &ProcessManifest::new("example_container_get").name("c_new_get"),
    )?;
    assert_eq!(pprofile.name, "c_new_get");
    let pid = find_process(&bp.container_process_list(false, None)?, "c_new_get").clone();
    assert_eq!(bp.container_process_destroy(&pid)?.name, "c_new_get");
    assert_eq!(bp.container_process_list(false, None)?.len(), 6);
    assert_eq!(bp.container_destroy(&cid)?.name, "c_new");
    assert_eq!(bp.container_list(false, None)?.len(), 3);

    // Value を返す EC の API は __value__ で包んだまま返す
    let created =
        bp.ec_create(&jvalue!({"type_name": "TimerEC", "name": "timer1", "rate": 100.0}))?;
    assert_eq!(created["__value__"]["name"], jvalue!("timer1"));
    // ec_destroy はメッセンジャーブローカーのスレッド内で EC を drop する。
    // EC が持つ tokio のランタイムをそこで落としても panic しないこと (issue #231)
    let id = created["__value__"]["identifier"]
        .as_str()
        .expect("identifier")
        .to_owned();
    bp.ec_destroy(&id)?;
    assert!(!bp.ec_list(false)?["__value__"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(id.as_str())));
    Ok(())
}

#[test]
fn local_broker_api_with_rust_python_cpp_plugins() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest("local_broker_api_test", PORT))?
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system
                .core_broker()
                .lock()?
                .worker()
                .broker_proxy("local", "local", true)?;
            let mut bp = juiz_write(&proxy)?;
            check_system(&mut *bp)?;
            check_lists_and_profiles(&mut *bp)?;
            check_processes(&mut *bp)?;
            check_ec(&mut *bp)?;
            check_connections(&mut *bp)?;
            check_create_destroy(&mut *bp)?;

            // p_apply で送った値が __value__ で包まれずに引数へ入ること (issue #36)
            let id = find_process(&bp.process_list(false, None)?, "inc0").clone();
            bp.process_p_apply(&id, "arg1", jvalue!(3).into())?;
            assert_eq!(
                int_of(system.core_broker().lock_mut()?.process_execute(&id)),
                4
            );
            Ok(())
        })
}
