
mod identifier_test;

extern crate juiz_core;
use juiz_core::prelude::*;

mod common;

  

// #[test]
// fn no_name_manifest_process_test() -> JuizResult<()> {
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
// fn no_arguments_manifest_process_test()  -> JuizResult<()> {
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
// fn no_default_manifest_process_test() -> JuizResult<()>  {
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
fn call_process_test() -> JuizResult<()>  {
    

    match common::new_increment_process("incremnet")?.call(vec!(("arg1", jvalue!(1))).into()) {
        Ok(vv) => {

            let iv = vv.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
            assert_eq!(iv, 2);
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn invoke_process_test() -> JuizResult<()>  {
    

    match common::new_increment_process("increment")?.invoke() {
        Ok(vv) => {

            let iv = vv.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
            assert_eq!(iv, 2);
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}



#[cfg(test)]
#[test]
fn execute_process_test()  -> JuizResult<()> {
    

    match common::new_execution_process("execute")?.execute() {
        Ok(vv) => {

            let iv = vv.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
            assert_eq!(iv, 1);
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn call_invalid_argument_process_test() -> JuizResult<()>  {
    match common::new_increment_process("increment")?.call(vec!(("arg2", jvalue!(1))).into()) {
        Ok(_vv) => {
            assert!(false, "Process must be return error.");
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
            // assert_eq!(ev, JuizError::ArgumentMissingWhenCallingError{});
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn invoke_add_process_test()  -> JuizResult<()> {
    

    match common::new_add_process("add_01")?.invoke() {
        Ok(vv) => {

            let iv = vv.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
            assert_eq!(iv, 2);
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn bind_and_invoke_add_process_test()->JuizResult<()> {
    
    let mut p = common::new_add_process("add_01")?;
    p.p_apply("arg1", jvalue!(2).into()).expect("Bind Error.");
    let vv = p.invoke().expect("Bind Error");
    let iv = vv.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    assert_eq!(iv, 3);
    Ok(())
}