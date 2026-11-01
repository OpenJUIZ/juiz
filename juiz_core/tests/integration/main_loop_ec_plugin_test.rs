//! conf の ec_factories に main_loop_ec を書いても起動できる (issue #323)
//!
//! main_loop_ec は juiz_core に組み込みがある。プラグインとして読み込み済みなら、ブローカーと同じく
//! 組み込みは登録せず、プラグインの方を使う(以前は二重登録で setup が失敗していた)。
//! プラグイン (main_loop_ec、increment_process) は ../target/debug から読む。

use juiz_core::prelude::*;

const EC_ID: &str = "core://core/execution_context/ec0::main_loop_ec";

fn manifest(with_plugin: bool) -> Value {
    let mut plugins = jvalue!({
        "process_factories": {
            "increment_process": { "path": "../target/debug" }
        }
    });
    if with_plugin {
        plugins["ec_factories"] = jvalue!({ "main_loop_ec": { "path": "../target/debug" } });
    }
    jvalue!({
        "name": "main_loop_ec_plugin_test",
        "option": { "http_broker": { "start": false } },
        "plugins": plugins,
        "processes": [ { "type_name": "increment_process", "name": "inc0" } ],
        "ecs": [
            { "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0", "auto_start": false,
              "bind": [ { "name": "inc0", "type_name": "increment_process" } ] },
        ],
    })
}

fn builtin_skipped_log_count() -> usize {
    crate::common::logger::captured()
        .iter()
        .filter(|l| {
            l.contains(
                "ExecutionContextFactory(type_name=main_loop_ec) is already loaded as a plugin",
            )
        })
        .count()
}

/// ec_factories に main_loop_ec があっても setup が通り、EC を作って bind できる
#[test]
fn main_loop_ec_as_plugin_does_not_conflict_with_builtin() -> JuizResult<()> {
    crate::common::logger::init();
    let before = builtin_skipped_log_count();
    System::new(manifest(true))?
        .setup()?
        .run_and_do_once(|system| {
            let prof = system
                .core_broker()
                .lock()?
                .ec_profile_full(&EC_ID.to_owned())?;
            assert_eq!(get_array(&prof["targets"])?.len(), 1, "{prof:?}");
            Ok(())
        })?;
    // 組み込みを使わなかったことを info で知らせる(並列に走る別のテストのログが混ざるので増えた分を見る)
    assert!(builtin_skipped_log_count() > before);
    Ok(())
}

/// ec_factories に書かなければ組み込みの main_loop_ec が使われる(これまでどおり)
#[test]
fn main_loop_ec_builtin_is_used_without_plugin() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(manifest(false))?
        .setup()?
        .run_and_do_once(|system| {
            let prof = system
                .core_broker()
                .lock()?
                .ec_profile_full(&EC_ID.to_owned())?;
            assert_eq!(get_array(&prof["targets"])?.len(), 1, "{prof:?}");
            Ok(())
        })?;
    Ok(())
}
