extern crate juiz_core;
use juiz_core::{prelude::*, SystemStore, SystemStorePtr};

mod common;


fn new_process_factory(cb: &mut CoreBroker) -> JuizResult<ProcessFactoryPtr> {
    // let manifest = jvalue!({
    //     "type_name" : "increment",
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
    let manifest = ProcessManifest::new("increment")
        .use_memo(true)
        .add_int_arg("arg1", "test_argument", 1);
    let type_name = "increment";
    let pf = process_factory_create(manifest, common::increment_function)?;
    let result_pf = cb.worker_mut().store_mut().processes.register_factory(
        &type_name.to_owned(), pf.clone());
    assert!(result_pf.is_ok(), "register_process_factory failed. Error is {:?}", result_pf.err());
    Ok(pf)
}

fn new_core_broker() -> CoreBroker {

    let result = CoreBroker::new(jvalue!(
        {
            "name": "core_broker"
        }
    ), SystemStorePtr::new(SystemStore::new()));
    
    assert!(result.is_ok(), "CoreBroker::new failed. {:?}", result.err());
    result.ok().unwrap()
}

//#[cfg(test)]
//#[test]
#[allow(dead_code)]
fn core_broker_process_factory_integration_test() -> JuizResult<()> {
    //use juiz_core::ProcessBrokerProxy;

    let mut cb = new_core_broker();
    let _pf = new_process_factory(&mut cb);

    //let mut id = "".to_string();

    let p_result = cb.worker_mut().create_process_ref(
        &ProcessManifest::new("increment").name("test_function")
    );
    assert!(p_result.is_ok(), "process_create failed. Error is {:?}", p_result.err());

    let arc_p = p_result.ok().unwrap();
    let p = arc_p.lock()?;
    
    let id = p.identifier().clone();
    

    //assert!(cb.is_in_charge_for_process(&id));

    let retval = cb.process_call(&id, vec!(("arg1", jvalue!(
        1
    ))).into());
    match retval {
        Ok(arc) => {

            let iv = arc.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
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
fn core_broker_process_factory_integration_connection_test() -> JuizResult<()> {
    //use juiz_core::brokers::broker_proxy::{ConnectionBrokerProxy, ProcessBrokerProxy};

    let mut cb = new_core_broker();
    let _pf = new_process_factory(&mut cb);
    
    let p1_result = cb.worker_mut().create_process_ref(
        &ProcessManifest::new("increment").name("test_function1"));
    //     jvalue!({
    //     "name": "test_function1",
    //     "type_name": "increment",
    // }).try_into()?);
    assert!(p1_result.is_ok(), "process_create failed. Error is {:?}", p1_result.err());

    let list_v1 = cb.process_list(true, None)?;
    assert!(list_v1.len() == 1, "First process is not registered to CoreBroker");

    let arc_p1 = p1_result.ok().unwrap();
    
    let id1 = arc_p1.lock()?.identifier().clone();

    let p2_result = cb.worker_mut().create_process_ref(
        &ProcessManifest::new("increment").name("test_function2"));
    //     jvalue!({
    //     "name": "test_function2",
    //     "type_name": "increment",
    // }).try_into()?);
    assert!(p2_result.is_ok(), "process_create failed. Error is {:?}", p2_result.err());

    let arc_p2 = p2_result.ok().unwrap();
    let id2 = arc_p2.lock()?.identifier().clone();
    
    //assert!(cb.is_in_charge_for_process(&id1));
    //assert!(cb.is_in_charge_for_process(&id2));
    
    let con_manif = ConnectionManifest::new(
        ConnectionType::Push,
        id1.clone(),
        "arg1".to_owned(),
        id2.clone()
    );

    let list_v = cb.process_list(true, None)?;
    println!("list_v={list_v:?}");
    let con_result = cb.connection_create(&con_manif);
    assert!(con_result.is_ok(), "core_broker_integration_test.rs: CoreBroker::connect() failed. Error is {:?}", con_result.err());

    let retval = cb.process_execute(&id1);
    match retval {
        Ok(arc) => {

            let iv = arc.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
            //let vv = juiz_lock(&arc).unwrap();
            assert_eq!(iv, 2);
        }, 
        Err(ev) => {
            print!("Return value is {:?}", ev);
        }
    }
    let p2_result2 = cb.worker().store().processes.get(&id2.to_string());
    assert!(p2_result2.is_ok(), "Process 2 can not acquire. Error is {:?}", p2_result2.err());
    
    let arc_out = p2_result2.ok().unwrap().lock()?.get_output();
    //let output = juiz_lock(&arc_out).unwrap();
    assert!(!arc_out.is_empty().unwrap(), "Error. Process2 Output is None.");

    //
    // 1 (default) -> proc1 -> 2 -> procec2 -> 3. Answer must be 3.

    let iv = arc_out.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    assert_eq!(iv, 3, "Error. Execution output of Process 2 is wrong.");
    Ok(())
}
