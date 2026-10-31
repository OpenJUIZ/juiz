//! System を起動して BrokerProxy の API を叩くテスト用の共通部品
//!
//! Rust / Python / C++ のプロセスとコンテナを読み込んだ System を作る。
//! C++ の例は CI の "Build C++ examples" で ../target にビルドされ、
//! Rust の例は cargo build で ../target/debug にビルドされている前提。

#![allow(dead_code)]

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

pub fn system_manifest(name: &str, port: i64) -> Value {
    jvalue!({
        "name": name,
        "option": {
            "http_broker": { "start": true, "port": port },
            // Python プラグインの位置からの相対パス (tests/python -> リポジトリルート)
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" },
                "py_increment": { "language": "python", "path": "tests/python" },
                "increment_process_cpp": { "language": "c++", "path": "../target" }
            },
            "container_factories": {
                "example_container": {
                    "path": "../target/debug",
                    "processes": {
                        "example_container_increment": { "path": "../target/debug" },
                        "example_container_get": { "path": "../target/debug" }
                    }
                },
                "py_counter": {
                    "language": "python",
                    "path": "tests/python",
                    "processes": {
                        "py_counter_add": { "path": "tests/python" },
                        "py_counter_get": { "path": "tests/python" }
                    }
                },
                "example_container_cpp": {
                    "language": "c++",
                    "path": "../target",
                    "processes": {
                        "example_container_cpp_increment": { "path": "../target" },
                        "example_container_cpp_get": { "path": "../target" }
                    }
                }
            },
            "ec_factories": {
                "timer_ec": { "path": "../target/debug" }
            }
        },
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "py_increment", "name": "pyinc0" },
            { "type_name": "increment_process_cpp", "name": "cppinc0" }
        ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [
                  { "type_name": "example_container_increment", "name": "c0inc" },
                  { "type_name": "example_container_get", "name": "c0get" } ] },
            { "type_name": "py_counter", "name": "pyc0",
              "processes": [
                  { "type_name": "py_counter_add", "name": "pyc0add" },
                  { "type_name": "py_counter_get", "name": "pyc0get" } ] },
            { "type_name": "example_container_cpp", "name": "cppc0",
              "processes": [
                  { "type_name": "example_container_cpp_increment", "name": "cppc0inc" },
                  { "type_name": "example_container_cpp_get", "name": "cppc0get" } ] }
        ],
        "ecs": [
            { "type_name": "TimerEC", "name": "timer0", "rate": 100.0 }
        ]
    })
}

/// 各言語のプロセス (名前, 型名)
pub const PROCESSES: [(&str, &str); 3] = [
    ("inc0", "increment_process"),
    ("pyinc0", "py_increment"),
    ("cppinc0", "increment_process_cpp"),
];

/// 各言語のコンテナ (コンテナ名, 加算プロセス名, 加算の引数名, 取得プロセス名)
pub const CONTAINERS: [(&str, &str, &str, &str); 3] = [
    ("c0", "c0inc", "arg1", "c0get"),
    ("pyc0", "pyc0add", "arg1", "pyc0get"),
    ("cppc0", "cppc0inc", "arg0", "cppc0get"),
];

pub fn find_process<'a>(ids: &'a [ProcessIdentifier], name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found in {ids:?}"))
}

pub fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// 結果のカプセルに、要求の経路 (class_name / function_name など) が option として残っていない (#277)
pub fn assert_no_options(c: JuizResult<CapsulePtr>) -> CapsulePtr {
    let c = c.expect("call failed");
    let options = c.lock_as_capsule(|c| c.get_options().clone()).unwrap();
    assert!(options.is_empty(), "options should be empty: {options:?}");
    c
}

/// get_output などでカプセルが Value でない場合も考えて、整数として取り出す
pub fn try_int_of(c: &CapsulePtr) -> Option<i64> {
    c.lock_as_value(|v| v.as_i64()).ok().flatten()
}
