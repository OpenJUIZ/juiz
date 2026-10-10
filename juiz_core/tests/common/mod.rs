

use juiz_core::prelude::*;


#[allow(dead_code)]
pub fn add_function(v: CapsuleMap) -> JuizResult<Capsule> {
    //let i = arg(&v, "arg1")?.as_i64().unwrap();
    //let iv = juiz_lock(&v.get("arg1").unwrap())?.as_value().unwrap().as_i64().unwrap();
    let iv1 = v.get("arg1")?.lock_as_value(|value| {
        value.as_i64().unwrap()
    } )?;
    let iv2 = v.get("arg2")?.lock_as_value(|value| {
        value.as_i64().unwrap()
    } )?;
    return Ok(jvalue!(iv1+iv2).into());
}

#[allow(dead_code)]
pub fn new_add_process<'a> (name: &str) -> JuizResult<impl Process> {
    let manifest = ProcessManifest::new("add").name(name)
        .add_int_arg("arg1", "test_argument_1", 1)
        .add_int_arg("arg2", "test_argument_2", 1);
    // let manifest = jvalue!({
    //     "name": name,
    //     "type_name": "add",
    //     "arguments" : [
    //         {
    //             "name": "arg1",
    //             "type": "int",
    //             "description": "test_argument_1",
    //             "default": 1,
    //         }, 
    //          {
    //             "name": "arg2",
    //             "type": "int",
    //             "description": "test_argument_2",
    //             "default": 1,
    //         }, 
        
    //     ]
    // });
    let p = process_new(manifest, add_function);
    assert!(p.is_ok(), "ProcessImpl::new() failed. Error is '{:?}'", p.err());
    p
}



#[allow(dead_code)]
pub fn increment_function(v: CapsuleMap) -> JuizResult<Capsule> {
    //let i = arg(&v, "arg1")?.as_i64().unwrap();
    //let iv = juiz_lock(&v.get("arg1").unwrap())?.as_value().unwrap().as_i64().unwrap();
    let iv = v.get("arg1")?.lock_as_value(|value| {
        value.as_i64().unwrap()
    } )?;
    return Ok(jvalue!(iv+1).into());
}

#[allow(dead_code)]
pub fn new_increment_process<'a> (name: &str) -> JuizResult<impl Process> {
    let manifest = ProcessManifest::new("increment")
        .name(name)
        .add_int_arg("arg1", "test_argument", 1);
    // let manifest = jvalue!({
    //     "name": name,
    //     "type_name": "increment",
    //     "arguments" : [
    //         {
    //             "name": "arg1",
    //             "type": "int",
    //             "description": "test_argument",
    //             "default": 1,
    //         }, 
    //     ], 
    // });
    let p = process_new(manifest, increment_function);
    assert!(p.is_ok(), "ProcessImpl::new() failed. Error is '{:?}'", p.err());
    p
}

#[allow(dead_code)]
pub fn new_increment_process_use_memo<'a> (name: &str) -> JuizResult<impl Process> {
    let manifest = ProcessManifest::new("increment")
        .name(name)
        .use_memo(true)
        .add_int_arg("arg1", "test_argument", 1);
    // let manifest = jvalue!({
    //     "name": name,
    //     "type_name": "increment",
    //     "use_memo": true,
    //     "arguments" : [
    //         {
    //             "name": "arg1",
    //             "type": "int",
    //             "description": "test_argument",
    //             "default": 1,
    //         }, 
    //     ], 
    // });
    let p = process_new(manifest, increment_function);
    assert!(p.is_ok(), "ProcessImpl::new() failed. Error is '{:?}'", p.err());
    p
}


static mut COUNTER: i64 = 0;

#[allow(dead_code)]
pub fn execution_function(_v: CapsuleMap) -> JuizResult<Capsule> {
    #[allow(unused)]
    let mut val: i64 = 0;
    unsafe {
        COUNTER = COUNTER + 1;
        val = COUNTER;
    }
    return Ok(jvalue!(val).into());
}

#[allow(dead_code)]
pub fn new_execution_process<'a> (name: &str) -> JuizResult<impl Process> {
    let manifest = ProcessManifest::new(name)
        .name("test_function")
        .add_int_arg("arg1", "test_argument", 1);
    // let manifest = jvalue!({
    //     "name": "test_function",
    //     "type_name": name,
    //     "arguments" : [
    //         {
    //             "name": "arg1",
    //             "type": "int",
    //             "description": "test_argument",
    //             "default": 1,
    //         }, 
    //     ], 
    // });
    let p = process_new(manifest, execution_function);
    assert!(p.is_ok(), "ProcessImpl::new() failed. Error is {:?}", p.err());
    p
}
