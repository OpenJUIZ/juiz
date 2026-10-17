//! 作業ディレクトリが相対パスのときに Python プラグインを読み込めること (issue #78)
//!
//! `juiz -f confs/xxx.conf` のように相対パスで設定ファイルを渡すと、作業ディレクトリが相対パスになる。

use juiz_core::prelude::*;
use std::path::Path;

fn manifest(plugin_dir: &str) -> Value {
    jvalue!({
        "name": "python_plugin_path_test",
        "option": {
            "http_broker": { "start": false },
            // Python プラグインの位置からの相対パス (tests/python -> リポジトリルート)
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": {
            "process_factories": {
                "py_increment": { "language": "python", "path": plugin_dir }
            }
        },
        "processes": [
            { "type_name": "py_increment", "name": "pyinc0" }
        ]
    })
}

#[test]
fn python_process_loads_with_relative_working_dir() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(manifest("python"))?
        .set_working_dir(Path::new("tests"))
        .setup()?
        .run_and_do_once(|system| {
            let proxy = system
                .core_broker()
                .lock()?
                .worker()
                .broker_proxy("local", "local", true)?;
            let bp = juiz_lock(&proxy)?;
            let ids = bp.process_list(false, None)?;
            let id = ids
                .iter()
                .find(|id| id.name == "pyinc0")
                .expect("pyinc0 が見つからない");
            let result = bp.process_push_by(id, "arg1".to_owned(), jvalue!(7).into())?;
            assert_eq!(result.lock_as_value(|v| v.as_i64())?, Some(8));
            Ok(())
        })
}

#[test]
fn python_process_missing_file_returns_error() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let result = System::new(manifest("no_such_dir"))?
        .set_working_dir(Path::new("tests"))
        .setup();
    let err = result.err().expect("存在しないファイルでエラーにならない");
    let message = format!("{err:?}");
    assert!(
        message.contains("no_such_dir"),
        "エラーにパスが含まれていない: {message}"
    );
    Ok(())
}
