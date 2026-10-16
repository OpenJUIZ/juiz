//! サブシステムの登録に関するテスト

use juiz_core::prelude::*;

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
