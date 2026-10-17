//! サブシステムの登録に関するテスト

use juiz_core::prelude::*;
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
    let _r = env_logger::try_init();
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
    let _r = env_logger::try_init();
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
