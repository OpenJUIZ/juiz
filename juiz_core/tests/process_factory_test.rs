
extern crate juiz_core;

use juiz_core::prelude::*;


mod common;

#[test]
fn simple_process_create_test() -> JuizResult<()>{
    // let manifest = jvalue!({
    //     "type_name" : "increment",
    //     "arguments" : [
    //         {
    //             "name": "arg1",
    //             "type": "int", 
    //             "description": "test_argument",
    //             "default": 1,
    //         }, 
    //     ]
    // });
    let manifest = ProcessManifest::new("increment")
        .add_int_arg("arg1", "test_argument", 1);
    let result_pf =process_factory_create(manifest, common::increment_function)?;
    /*
    assert!(result_pf.is_ok());
    */
    // let proc_manifest = jvalue!(
    //     {
    //         "name": "hogehoge",
    //         "type_name": "increment",
    //     }
    // );
    let proc_manifest = ProcessManifest::new("increment").name("hogehoge");
    let p = result_pf.lock()?.create_process(&proc_manifest);
    assert!(p.is_ok(), "ProcessImpl::new() failed. Error is {:?}", p.err());
    let result = p.ok().unwrap().lock()?.call(vec!(("arg1", jvalue!(3))).into());
    assert!(result.is_ok());
    let res_value = result.ok().unwrap();
    
    let iv = res_value.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    //assert!(juiz_lock(&res_value).unwrap().as_value().unwrap().is_i64());
    assert!(iv == 4);
    Ok(())
}
