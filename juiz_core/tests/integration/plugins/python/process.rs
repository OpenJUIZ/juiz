//! Python プラグインのテスト (issue #221)
//!
//! - component_manifest() の無い Python ファイルをコンポーネントとして読み込むと、デコレータの付いた
//!   Process・Container・ContainerProcess からマニフェストを作る
//! - Python と juiz の値の変換 (bool / float / None / tuple / set / 入れ子 / u64)
//! - 読み込みに失敗したときのエラー
//!
//! Python のファイルは tests/python から読む。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn system_manifest() -> Value {
    jvalue!({
        "name": "python_plugin_test",
        "option": {
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": {
            "process_factories": {
                "py_types": { "language": "python", "path": "tests/python" }
            },
            "components": {
                "py_component": { "language": "python", "path": "tests/python" }
            }
        },
        "processes": [
            { "type_name": "py_types", "name": "types0" },
            { "type_name": "py_comp_double", "name": "double0" }
        ],
        "containers": [
            { "type_name": "py_comp_acc", "name": "acc0", "initial_value": 10,
              "processes": [ { "type_name": "py_comp_acc_add", "name": "acc0add" } ] }
        ]
    })
}

fn call(system: &System, id: &str, args: Value) -> JuizResult<Value> {
    let id: ProcessIdentifier = id.try_into()?;
    let args: CapsuleMap = args.try_into()?;
    let class_name = id.class_name.clone();
    let cb = system.core_broker().lock()?;
    let out = if class_name == "container_process" {
        cb.container_process_call(&id, args)?
    } else {
        cb.process_call(&id, args)?
    };
    out.lock_as_value(|v| v.clone())
}

fn types(system: &System, kind: &str) -> JuizResult<Value> {
    call(
        system,
        "core://core/process/types0::py_types",
        jvalue!({ "kind": kind, "value": [] }),
    )
}

fn check_component(system: &System) -> JuizResult<()> {
    assert_eq!(
        call(
            system,
            "core://core/process/double0::py_comp_double",
            jvalue!({"x": 21})
        )?,
        jvalue!(42)
    );
    assert_eq!(
        call(
            system,
            "core://core/container_process/acc0add::py_comp_acc_add:acc0",
            jvalue!({"x": 5})
        )?,
        jvalue!(15)
    );
    Ok(())
}

fn check_value_conversion(system: &System) -> JuizResult<()> {
    assert_eq!(types(system, "bool")?, jvalue!(true));
    assert_eq!(types(system, "float")?, jvalue!(1.5));
    assert_eq!(types(system, "none")?, Value::Null);
    assert_eq!(types(system, "tuple")?, jvalue!([1, "a"]));
    assert_eq!(types(system, "set")?, jvalue!([3]));
    assert_eq!(
        types(system, "nested")?,
        jvalue!([[1, 2], {"a": [true, null]}])
    );

    // juiz から Python へ: 真偽値、i64 に入らない整数、入れ子の配列、小数、null
    let value = jvalue!([true, u64::MAX, [1, 2], 0.5, null, "s"]);
    let echoed = call(
        system,
        "core://core/process/types0::py_types",
        jvalue!({ "kind": "echo", "value": value.clone() }),
    )?;
    assert_eq!(echoed[0], value);
    assert_eq!(
        echoed[1],
        jvalue!(["bool", "int", "list", "float", "NoneType", "str"])
    );
    Ok(())
}

#[test]
fn python_component_and_value_conversion() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            check_component(system)?;
            check_value_conversion(system)?;
            Ok(())
        })
}

/// 読み込めない Python のファイルはエラーになり、System の setup が失敗する
#[test]
fn python_plugin_load_errors() -> JuizResult<()> {
    crate::common::logger::init();
    for plugins in [
        // ファイルが無い
        jvalue!({ "process_factories": { "py_no_such_file": { "language": "python", "path": "tests/python" } } }),
        jvalue!({ "components": { "py_no_such_component": { "language": "python", "path": "tests/python" } } }),
    ] {
        let manifest = jvalue!({
            "name": "python_plugin_error_test",
            "option": { "pythonpath": ["../../../bindings/pyjuiz"] },
            "plugins": plugins.clone(),
        });
        let result = System::new(manifest).and_then(|s| s.setup());
        assert!(result.is_err(), "{plugins:?}");
    }
    Ok(())
}
