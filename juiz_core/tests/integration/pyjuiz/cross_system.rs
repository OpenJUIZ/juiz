//! System をまたぐ Transform (issue #415、docs/spec/transformations.md §19) を pyjuiz のプロキシで確かめる
//!
//! HTTP ブローカーを立てた System を 2 つ (S1: 地図の `map`、S2: ロボットの `odom`・`base0`・`c2`、
//! S2 は S1 をマスターシステムにする) 起動したまま、別プロセスの Python で
//! bindings/pyjuiz/test/cross_system_test.py を実行する。

use juiz_core::prelude::*;

const PORT_S1: i64 = 8096;
const PORT_S2: i64 = 8097;

fn s1_manifest() -> Value {
    jvalue!({
        "name": "pyjuiz_cross_system_s1",
        "option": { "http_broker": { "start": true, "port": PORT_S1 } },
        "containers": [ { "type_name": "static_anchor", "name": "map" } ]
    })
}

fn s2_manifest() -> Value {
    jvalue!({
        "name": "pyjuiz_cross_system_s2",
        "option": { "http_broker": { "start": true, "port": PORT_S2 } },
        "plugins": {
            "container_factories": { "example_container": { "path": "../target/debug" } }
        },
        "containers": [
            { "type_name": "example_container", "name": "odom" },
            { "type_name": "example_container", "name": "base0" },
            { "type_name": "example_container", "name": "c2" }
        ],
        "mastersystem": { "broker_type": "http", "broker_name": format!("localhost:{PORT_S1}") }
    })
}

#[test]
fn pyjuiz_transforms_across_systems() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(s1_manifest())?
        .setup()?
        .run_and_do_once(|_s1| {
            System::new(s2_manifest())?
                .setup()?
                .run_and_do_once(|_s2| {
                    let python =
                        std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_owned());
                    let output = std::process::Command::new(python)
                        .current_dir("../bindings/pyjuiz")
                        .env("PYTHONPATH", ".")
                        .args([
                            "test/cross_system_test.py",
                            PORT_S1.to_string().as_str(),
                            PORT_S2.to_string().as_str(),
                        ])
                        .output()?;
                    assert!(
                        output.status.success(),
                        "cross_system_test.py failed\nstdout:\n{}\nstderr:\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    Ok(())
                })
        })
}
