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
