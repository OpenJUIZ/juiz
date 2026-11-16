//! C++ のプロセスが int64_t 以外の型 (double / bool) の戻り値を
//! その型のまま返すことを確かめるテスト (issue #77)
//!
//! C++ の例は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn system_manifest() -> Value {
    jvalue!({
        "name": "cpp_return_type_test",
        "plugins": {
            "process_factories": {
                "limit_process_cpp": { "language": "c++", "path": "../target" },
                "is_positive_process_cpp": { "language": "c++", "path": "../target" }
            }
        },
        "processes": [
            { "type_name": "limit_process_cpp", "name": "limit0" },
            { "type_name": "is_positive_process_cpp", "name": "positive0" }
        ]
    })
}

fn find(cb: &CoreBroker, name: &str) -> JuizResult<ProcessIdentifier> {
    let ids = cb.process_list(false, None)?;
    Ok(ids
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found")))
}

fn call(cb: &CoreBroker, id: &ProcessIdentifier, args: Vec<(&str, Value)>) -> Value {
    cb.process_call(id, args.into())
        .expect("call failed")
        .lock_as_value(|v| v.clone())
        .unwrap()
}

#[test]
fn cpp_process_returns_float_and_bool() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let cb = system.core_broker().lock()?;

            let limit = find(&cb, "limit0")?;
            for (v, expected) in [(0.7, 0.7), (-3.0, -1.0), (2.5, 1.0)] {
                let r = call(&cb, &limit, vec![("v", jvalue!(v)), ("v_max", jvalue!(1.0))]);
                // 整数に切り捨てられず、浮動小数点のまま返る
                assert!(r.is_f64(), "v={v}: {r:?}");
                assert_eq!(r.as_f64(), Some(expected), "v={v}");
            }

            let positive = find(&cb, "positive0")?;
            for (v, expected) in [(0.5, true), (-0.5, false)] {
                let r = call(&cb, &positive, vec![("v", jvalue!(v))]);
                assert_eq!(r, jvalue!(expected), "v={v}");
            }
            Ok(())
        })
}
