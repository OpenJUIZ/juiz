extern crate juiz_core;

use juiz_core::prelude::*;
//use crate::juiz_core::processes::process_impl::*;


mod common;

fn new_increment_process<'a> () -> JuizResult<impl Process> {
    let manifest = ProcessManifest::new("increment")
        .name("test_function")
        .add_int_arg("arg1", "test_argument", 1);
    // let manifest = jvalue!({
    //     "name": "test_function",
    //     "type_name": "increment",
    //     "arguments" : [
    //         {
    //             "name": "arg1",
    //             "type": "int",
    //             "description": "test_argument",
    //             "default": 1,
    //         }, 
    //     ] 
    // });
    let p = process_new(manifest, common::increment_function);
    assert!(p.is_ok() , "ProcessImpl::new() failed. Error is {:?}", p.err());
    p
}


#[cfg(test)]
#[test]
fn core_broker_test() ->JuizResult<()> {
    

    use juiz_core::{SystemStore, SystemStorePtr};

    //use juiz_core::brokers::broker_proxy::ProcessBrokerProxy;

    
    let result = CoreBroker::new(jvalue!(
        {
            "name": "core_broker"
        }
    ), SystemStorePtr::new(SystemStore::new()));
    if result.is_err() {
        assert!(false, "CoreBroker::new failed. {:?}", result.err())
    }

    let mut cb = result.unwrap();

    let p = new_increment_process()?;
    let id = p.identifier().clone();
    let result = cb.worker_mut().store_mut().processes.register(&id.to_string(), ProcessPtr::new(p));

    assert!(result.is_ok());

    //assert!(cb.is_in_charge_for_process(&id));

    let retval = cb.process_call(&id, vec!(("arg1", jvalue!(1))).into());
    match retval {
        Ok(arc) => {
            let iv = arc.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
            assert_eq!(iv, 2);
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    };
    Ok(())

}
