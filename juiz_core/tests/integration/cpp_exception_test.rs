//! C++ のプラグインの関数が juiz の例外ではない例外 (std::runtime_error・std::bad_alloc・int など) を投げても、
//! juiz が落ちずにエラーを返すことを確かめるテスト (issue #400)
//!
//! C++ のコンポーネントは examples/cpp/component/cpp_exception_component。scripts/test.sh (CI も同じ) で
//! ../target/debug にビルドされている前提。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn conf() -> Value {
    jvalue!({
        "name": "cpp_exception_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "components": {
                "cpp_exception_component": { "language": "c++", "path": "../target/debug" }
            }
        },
        "processes": [
            { "type_name": "cpp_throw", "name": "throw0" }
        ],
        "containers": [
            { "type_name": "cpp_throwing_part", "name": "part0",
              "processes": [ { "type_name": "cpp_part_throw", "name": "part0_throw" } ] }
        ],
    })
}

fn find_process(system: &System, name: &str) -> JuizResult<ProcessIdentifier> {
    let ids = system.core_broker().lock()?.process_list(false, None)?;
    Ok(ids
        .into_iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found")))
}

fn call_process(system: &System, kind: &str) -> JuizResult<Value> {
    let id = find_process(system, "throw0")?;
    let out = system
        .core_broker()
        .lock()?
        .process_call(&id, jvalue!({ "kind": kind }).try_into()?)?;
    out.lock_as_value(|v| v.clone())
}

fn call_container_process(system: &System, kind: &str) -> JuizResult<Value> {
    let id: ProcessIdentifier =
        "core://core/container_process/part0_throw::cpp_part_throw:part0".try_into()?;
    let out = system
        .core_broker()
        .lock()?
        .container_process_call(&id, jvalue!({ "kind": kind }).try_into()?)?;
    out.lock_as_value(|v| v.clone())
}

/// エラーが CppPluginExceptionError で、message にその文を含むことを確かめる
fn assert_exception(result: JuizResult<Value>, expected_message: &str) {
    let e = result.expect_err("the call should fail");
    match e.downcast_ref::<JuizError>() {
        Some(JuizError::CppPluginExceptionError { message, .. }) => {
            assert!(message.contains(expected_message), "message={message:?}, expected {expected_message:?}");
        }
        _ => panic!("unexpected error: {e:#}"),
    }
}

#[test]
fn cpp_process_exception_becomes_error() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        assert_eq!(call_process(system, "none")?, jvalue!("ok"));
        // std::exception の子は what() が届く
        assert_exception(call_process(system, "runtime_error"), "cannot write /no/such/dir/map.pgm");
        assert_exception(call_process(system, "bad_alloc"), "alloc");
        // std::exception の子ではない例外
        assert_exception(call_process(system, "int"), "unknown exception");
        // juiz は落ちずに、続けて呼べる
        assert_eq!(call_process(system, "none")?, jvalue!("ok"));
        Ok(())
    })
}

#[test]
fn cpp_container_process_exception_becomes_error() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        assert_eq!(call_container_process(system, "none")?, jvalue!(1));
        assert_exception(call_container_process(system, "runtime_error"), "cannot write /no/such/dir/map.pgm");
        assert_exception(call_container_process(system, "bad_alloc"), "alloc");
        assert_exception(call_container_process(system, "int"), "unknown exception");
        // 例外の後も Container は使える (例外の前に count は増えない)
        assert_eq!(call_container_process(system, "none")?, jvalue!(2));
        Ok(())
    })
}

#[test]
fn cpp_container_factory_exception_becomes_error() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let mut cb = system.core_broker().lock_mut()?;
        let r = cb.container_create(
            &ContainerManifest::new("cpp_broken_part").name("broken0"),
            CapsuleMap::new(),
        );
        let e = r.err().expect("creating cpp_broken_part should fail");
        // Container の作成には戻り値の capsule が無いので、エラー文は標準エラーに出て、ここにはコードが入る
        match e.downcast_ref::<JuizError>() {
            Some(JuizError::CppPluginExceptionError { function_name, .. }) => {
                assert_eq!(function_name, "create_container");
            }
            _ => panic!("unexpected error: {e:#}"),
        }
        Ok(())
    })
}
