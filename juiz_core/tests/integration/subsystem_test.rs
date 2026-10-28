//! サブシステムの登録に関するテスト

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;
use uuid::Uuid;

const MASTER_PORT: i64 = 8020;

fn system_manifest() -> Value {
    jvalue!({
        "name": "test_system",
        "option": {
            "http_broker": {
                "start": true,
                "port": MASTER_PORT
            }
        }
    })
}

/// increment_process を 1 つ持つ 2 台目の juiz。ポートが使われているので次のポートで待ち受ける
fn subsystem_manifest() -> Value {
    let mut manifest = system_manifest();
    manifest["plugins"] = jvalue!({
        "process_factories": {
            "increment_process": { "path": "../target/debug" }
        }
    });
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "inc0" }
    ]);
    manifest
}

fn subsystems(master: &System) -> JuizResult<Vec<Value>> {
    let profile = master.core_broker().lock()?.system_profile_full()?;
    Ok(profile["subsystems"]
        .as_array()
        .expect("subsystems must be array")
        .clone())
}

/// 2 台目の juiz を止めて同じアドレスで起動し直しても、1 台目の juiz に
/// サブシステムが二重に登録されない (issue #60)
#[cfg(test)]
#[test]
fn test_restarted_subsystem_replaces_old_one() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            let mut uuids = Vec::new();
            for _ in 0..2 {
                // 同じポートを指定すると次のポートで待ち受け、先の juiz のサブシステムとして登録される
                System::new(subsystem_manifest())?
                    .setup()?
                    .run_and_do_once(|sub| {
                        uuids.push(sub.core_broker().lock()?.system_uuid()?);
                        assert_eq!(subsystems(master)?.len(), 1);
                        // 先の juiz の再帰的な一覧に、2 台目の Process が 1 回だけ出る
                        let procs = master.core_broker().lock()?.process_list(true, None)?;
                        assert_eq!(procs.len(), 1, "{procs:?}");
                        Ok(())
                    })?;
            }
            assert_ne!(uuids[0], uuids[1]);
            assert_eq!(subsystems(master)?.len(), 1);
            Ok(())
        })
}

/// 1 台目の juiz で Topic を publish し、2 台目の juiz で subscribe する
fn publisher_manifest() -> Value {
    let mut manifest = system_manifest();
    manifest["plugins"] = jvalue!({
        "process_factories": {
            "increment_process": { "path": "../target/debug" }
        }
    });
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "pub0", "publishes": ["topic0"] }
    ]);
    manifest
}

fn subscriber_manifest() -> Value {
    let mut manifest = subsystem_manifest();
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "sub0", "subscribes": { "arg1": "topic0" } }
    ]);
    manifest
}

fn num_topic_subscriber_subsystems(system: &System) -> JuizResult<usize> {
    system
        .core_broker()
        .lock()?
        .worker()
        .store()
        .topics
        .get("topic0")
        .expect("topic0 must exist")
        .num_subscriber_subsystems()
}

/// 同じ問い合わせが 2 回来ても、また 2 台目の juiz を同じアドレスで起動し直しても、
/// Topic の転送先が二重に登録されない (issue #67)
#[cfg(test)]
#[test]
fn test_topic_subscriber_subsystem_is_not_duplicated() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(publisher_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            for _ in 0..2 {
                System::new(subscriber_manifest())?
                    .setup()?
                    .run_and_do_once(|sub| {
                        let uuid = Uuid::parse_str(
                            sub.core_broker().lock()?.system_uuid()?.as_str().unwrap(),
                        )?;
                        for _ in 0..2 {
                            let mut cb = sub.core_broker().lock_mut()?;
                            let bp: &mut dyn BrokerProxy = &mut *cb;
                            let r = bp.topic_request_publish("topic0", Some(uuid))?;
                            assert_eq!(r, jvalue!({"publish": true}));
                        }
                        assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
                        Ok(())
                    })?;
            }
            assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
            Ok(())
        })
}

/// 再帰的な system/profile_full で、サブシステムの中身と、経由する BrokerProxy が分かる (issue #116)
#[cfg(test)]
#[test]
fn test_recursive_profile_full_has_subsystem_contents() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            System::new(subsystem_manifest())?
                .setup()?
                .run_and_do_once(|sub| {
                    let uuid = sub.core_broker().lock()?.system_uuid()?;
                    // 再帰しない profile の subsystems は BrokerProxy の profile だけ
                    assert!(subsystems(master)?[0].get("core_store").is_none());

                    let profile = master
                        .core_broker()
                        .lock()?
                        .system_profile_full_recursive()?;
                    assert!(profile["connections"].is_array(), "{profile}");
                    let ss = profile["subsystems"]
                        .as_array()
                        .expect("subsystems must be array");
                    assert_eq!(ss.len(), 1);
                    assert_eq!(ss[0]["system_store"]["uuid"], uuid);
                    assert_eq!(ss[0]["broker_proxy"]["type_name"], jvalue!("http"));
                    assert_eq!(ss[0]["core_store"]["processes"][0]["name"], jvalue!("inc0"));
                    assert!(ss[0]["connections"].is_array(), "{}", ss[0]);
                    Ok(())
                })
        })
}

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("execute failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// サブシステム越しの publish / subscribe は、ネームスペースで解決した完全な名前でやり取りする (issue #149)
#[cfg(test)]
#[test]
fn test_namespaced_topic_over_subsystem() -> JuizResult<()> {
    crate::common::logger::init();
    let mut publisher = publisher_manifest();
    publisher["namespace"] = jvalue!("robot1");
    publisher["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "pub0", "publishes": ["value_topic"], "p_apply": { "arg1": 10 } }
    ]);
    let mut subscriber = subsystem_manifest();
    subscriber["namespace"] = jvalue!("robot1");
    subscriber["processes"] = jvalue!([
        // => robot1/value_topic (1 台目の juiz の pub0 とつながる)
        { "type_name": "increment_process", "name": "sub0", "subscribes": { "arg1": "value_topic" } },
        // => robot2/value_topic (つながらない)
        { "type_name": "increment_process", "name": "sub0", "namespace": "/robot2", "subscribes": { "arg1": "value_topic" } }
    ]);
    System::new(publisher)?.setup()?.run_and_do_once(|master| {
        System::new(subscriber)?.setup()?.run_and_do_once(|sub| {
            let topic_subsystems = |name: &str| -> JuizResult<usize> {
                match master
                    .core_broker()
                    .lock()?
                    .worker()
                    .store()
                    .topics
                    .get(name)
                {
                    Some(t) => t.num_subscriber_subsystems(),
                    None => Ok(0),
                }
            };
            assert_eq!(topic_subsystems("robot1/value_topic")?, 1);
            assert_eq!(topic_subsystems("robot2/value_topic")?, 0);
            assert_eq!(topic_subsystems("value_topic")?, 0);

            let pub0: ProcessIdentifier =
                "core://core/process/robot1/pub0::increment_process".try_into()?;
            let r1_sub0: ProcessIdentifier =
                "core://core/process/robot1/sub0::increment_process".try_into()?;
            let r2_sub0: ProcessIdentifier =
                "core://core/process/robot2/sub0::increment_process".try_into()?;
            assert_eq!(
                int_of(master.core_broker().lock_mut()?.process_execute(&pub0)),
                11
            );
            assert_eq!(
                int_of(sub.core_broker().lock_mut()?.process_execute(&r1_sub0)),
                12
            );
            assert_eq!(
                int_of(sub.core_broker().lock_mut()?.process_execute(&r2_sub0)),
                1
            );
            Ok(())
        })
    })
}
