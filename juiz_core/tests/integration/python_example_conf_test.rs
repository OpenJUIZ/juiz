//! examples/python の conf を読み込んで起動できること (issue #324)
//!
//! examples/python は他のテストから使われておらず、pyjuiz の型注釈の必須化 (#52) のあと
//! talker_listener_py が読み込めなくなっていたのに気づかなかった。conf をそのまま読んで起動し、
//! 古くならないようにする。

use juiz_core::anyhow;
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::utils::yaml_conf_load::yaml_conf_load;
use std::path::Path;

/// examples/python の下の conf を読み、テスト用に HTTP ブローカーと EC の自動開始を止める
fn load_example(dir: &str, conf: &str) -> JuizResult<Value> {
    let path = Path::new("../examples/python").join(dir).join(conf);
    let mut manifest = yaml_conf_load(path.display().to_string())?;
    manifest["option"]["http_broker"]["start"] = jvalue!(false);
    if let Some(ecs) = manifest["ecs"].as_array_mut() {
        for ec in ecs {
            ec["auto_start"] = jvalue!(false);
        }
    }
    Ok(manifest)
}

fn as_string(out: JuizResult<CapsulePtr>) -> JuizResult<String> {
    out?.lock_as_value(|v| v.as_str().map(str::to_owned))?
        .ok_or_else(|| anyhow::anyhow!("戻り値が文字列ではない"))
}

#[test]
fn talker_listener_py_conf_runs() -> JuizResult<()> {
    crate::common::logger::init();
    let dir = "process/talker_listener_py";
    System::new(load_example(dir, "talker_listener.conf")?)?
        .set_working_dir(&Path::new("../examples/python").join(dir))
        .setup()?
        .run_and_do_once(|system| {
            let talker: ProcessIdentifier =
                "core://core/process/talker0::talker_python".try_into()?;
            let listener: ProcessIdentifier =
                "core://core/process/listener0::listener_python".try_into()?;
            let cb = system.core_broker().lock_mut()?;
            assert_eq!(as_string(cb.process_execute(&talker))?, "Hello, World!");
            // con0 で talker0 の出力が listener0 の arg1 に届いている
            assert_eq!(as_string(cb.process_execute(&listener))?, "Hello, World!");
            Ok(())
        })
}

fn as_int(out: JuizResult<CapsulePtr>) -> JuizResult<i64> {
    out?.lock_as_value(|v| v.as_i64())?
        .ok_or_else(|| anyhow::anyhow!("戻り値が整数ではない"))
}

/// conf の `publishes` / `subscribes` で Topic を通って値が届くこと (issue #401)
///
/// この conf は `publish` / `subscribe` と書かれていて、Topic が黙って作られていなかった。
#[test]
fn increment_process_topic_conf_delivers_values() -> JuizResult<()> {
    crate::common::logger::init();
    let dir = "process/increment_process";
    System::new(load_example(dir, "increment_process_topic.conf")?)?
        .set_working_dir(&Path::new("../examples/python").join(dir))
        .setup()?
        .run_and_do_once(|system| {
            let id = |s: &str| -> JuizResult<ProcessIdentifier> { s.to_owned().try_into() };
            let pyinc0 = id("core://core/process/pyinc0::increment_process_python")?;
            let pyinc1 = id("core://core/process/pyinc1::increment_process_python")?;
            let pyprint = id("core://core/process/pyprint::print_value_python")?;
            // /api/topic/list と同じ一覧に 2 つの Topic が載る
            let mut topics: Vec<String> = {
                let cb = system.core_broker().lock()?;
                let bp: &dyn BrokerProxy = &*cb;
                bp.topic_list()?.into_iter().map(|t| t.name).collect()
            };
            topics.sort();
            assert_eq!(topics, vec!["print_topic", "value_topic"]);

            let cb = system.core_broker().lock_mut()?;
            // pyinc0 の出力が value_topic で pyinc1 の arg1 に、その出力が print_topic で pyprint に届く
            let v0 = as_int(cb.process_execute(&pyinc0))?;
            assert_eq!(as_int(cb.process_execute(&pyinc1))?, v0 + 1);
            assert_eq!(as_int(cb.process_execute(&pyprint))?, v0 + 1);
            Ok(())
        })
}
