extern crate juiz_core;
use juiz_core::prelude::*;

use crate::common;

// #[test]
// fn manifest_without_name_is_error() -> JuizResult<()> {
//     let p: JuizResult<ProcessManifest> = jvalue!({
//         "arguments" : [
//             {
//                 "name": "arg1",
//                 "type": "int",
//                 "description": "test_argument",
//                 "default": 1,
//             },
//         ]
//     }).try_into();
//     //let p = process_new(manifest.try_into()?, common::increment_function);
//     assert!(p.is_err());
//     Ok(())
//     // assert!(p.err() == Some(JuizError::ManifestNameMissingError{}));
// }

// #[test]
// fn manifest_without_arguments_is_error()  -> JuizResult<()> {
//     let p: JuizResult<ProcessManifest> = serde_json::json!({
//         "name": "hoge",
//         "type_name": "increment",
//     }).try_into();
//     //let p = process_new(manifest.try_into()?, common::increment_function);
//     assert!(p.is_err());
//     Ok(())
//     // assert!(p.err() == Some(JuizError::ManifestArgumentsMissingError{}));
// }

// #[test]
// fn manifest_without_default_is_error() -> JuizResult<()>  {
//     let manifest: JuizResult<ProcessManifest> = jvalue!({
//         "name": "hoge",
//         "type_name": "increment",
//         "arguments": [
//             {
//                 "name": "arg1",
//                 "description": "test_argument",
//             },
//         ]
//     }).try_into();
//     assert!(manifest.is_err());
//     //let _e = p.err();
//     Ok(())
//     // assert!(e == Some(JuizError::ManifestArgumentDefaultValueMissingError{}), "Error is {:?})", e);
// }

#[cfg(test)]
#[test]
fn call_returns_result() -> JuizResult<()> {
    match common::new_increment_process("incremnet")?.call(vec![("arg1", jvalue!(1))].into()) {
        Ok(vv) => {
            let iv = vv.lock_as_value(|value| value.as_i64().unwrap()).unwrap();
            assert_eq!(iv, 2);
        }
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn invoke_returns_result() -> JuizResult<()> {
    match common::new_increment_process("increment")?.invoke() {
        Ok(vv) => {
            let iv = vv.lock_as_value(|value| value.as_i64().unwrap()).unwrap();
            assert_eq!(iv, 2);
        }
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn execute_returns_result() -> JuizResult<()> {
    match common::new_execution_process("execute")?.execute() {
        Ok(vv) => {
            let iv = vv.lock_as_value(|value| value.as_i64().unwrap()).unwrap();
            assert_eq!(iv, 1);
        }
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn call_with_invalid_argument_is_error() -> JuizResult<()> {
    // 知らない引数 arg2 は無視され、省いた arg1 は既定値 1 で補われる (issue #265)
    let v = common::new_increment_process("increment")?.call(vec![("arg2", jvalue!(1))].into())?;
    assert_eq!(v.lock_as_value(|value| value.as_i64().unwrap())?, 2);
    Ok(())
}

#[cfg(test)]
#[test]
fn call_with_omitted_argument_uses_default() -> JuizResult<()> {
    let v = common::new_increment_process("increment")?.call(CapsuleMap::new())?;
    assert_eq!(v.lock_as_value(|value| value.as_i64().unwrap())?, 2);
    Ok(())
}

#[cfg(test)]
#[test]
fn call_with_omitted_argument_without_default_is_error() -> JuizResult<()> {
    let manifest =
        ProcessManifest::new("increment")
            .name("increment")
            .add_arg(ArgumentManifest::new(
                ArgumentType::Int,
                "arg1",
                "".into(),
                Value::Null,
            ));
    let p = process_new(manifest, common::increment_function)?;
    let err = p.call(CapsuleMap::new()).err().expect("must be error");
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::ArgumentMissingWhenCallingError { .. })
    ));
    Ok(())
}

#[cfg(test)]
#[test]
fn invoke_uses_default_arguments() -> JuizResult<()> {
    match common::new_add_process("add_01")?.invoke() {
        Ok(vv) => {
            let iv = vv.lock_as_value(|value| value.as_i64().unwrap()).unwrap();
            assert_eq!(iv, 2);
        }
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn invoke_uses_p_applied_argument() -> JuizResult<()> {
    let mut p = common::new_add_process("add_01")?;
    p.p_apply("arg1", jvalue!(2).into()).expect("Bind Error.");
    let vv = p.invoke().expect("Bind Error");
    let iv = vv.lock_as_value(|value| value.as_i64().unwrap()).unwrap();
    assert_eq!(iv, 3);
    Ok(())
}
