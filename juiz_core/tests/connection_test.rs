extern crate juiz_core;
use juiz_core::prelude::*;

mod common;


fn setup() -> JuizResult<(ProcessPtr, ProcessPtr)> {

    let p1 = common::new_increment_process("process1")?;
    let p2 = common::new_increment_process("process2")?;

    let rp1: ProcessPtr = ProcessPtr::new(p1);
    let rp2: ProcessPtr = ProcessPtr::new(p2);

    return Ok( (rp1, rp2) );

}

fn setup_use_memo() -> JuizResult<(ProcessPtr, ProcessPtr)> {

    let p1 = common::new_increment_process_use_memo("process1")?;
    let p2 = common::new_increment_process_use_memo("process2")?;

    let rp1: ProcessPtr = ProcessPtr::new(p1);
    let rp2: ProcessPtr = ProcessPtr::new(p2);

    return Ok( (rp1, rp2) );

}


#[cfg(test)]
#[test]
fn simple_connection_invoke_test() -> JuizResult<()>{
    

    let (rp1, rp2) = setup()?;

    let manifest = ConnectionManifest::new(
        ConnectionType::Pull,
        rp1.identifier().clone(),
        "arg1".to_owned(),
        rp2.identifier().clone(),
        //Some("con1".to_owned()),
    );
    // let manifeset =jvalue!({
    //     "identi": "con1",
    //     "type": "pull",
    // });
    // rp1 -> rp2
    let result1 = rp2.lock_mut()?.notify_connected_from(rp1.clone(), &manifest);
    assert!(result1.is_ok(), "Failed to connected_from function. Error is {:?}", result1.err());
    let result2 = rp1.lock_mut()?.try_connect_to(rp2.clone(), &manifest);
    assert!(result2.is_ok(), "Failed to connect_to function. Error is {:?}", result2.err());

    let result = rp2.lock()?.invoke().unwrap();

    let iv = result.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    assert_eq!(iv, 3);
    Ok(())
}


#[cfg(test)]
#[test]
fn simple_connection_push_invoke_test() -> JuizResult<()> {
    

    let (rp1, rp2) = setup()?;

    // let manifeset =jvalue!({
    //     "id": "con1",
    //     "type": "push",
    // });
    let manifest = ConnectionManifest::new(
        ConnectionType::Push,
        rp1.identifier().clone(),
        "arg1".to_owned(),
        rp2.identifier().clone()
    );
    // rp1 -> rp2
    let result1 = rp2.lock_mut()?.notify_connected_from(rp1.clone(),  &manifest);
    assert!(result1.is_ok(), "Failed to connected_from function. Error is {:?}", result1.err());
    let result2 = rp1.lock_mut()?.try_connect_to(rp2.clone(), &manifest);
    assert!(result2.is_ok(), "Failed to connect_to function. Error is {:?}", result2.err());

    let result = rp2.lock()?.invoke().unwrap();

    let iv = result.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    assert_eq!(iv, 2);
    Ok(())
}

#[test]
fn simple_connection_execute_test() -> JuizResult<()> {
    let (rp1, rp2) = setup_use_memo()?;

    let manifest = ConnectionManifest::new(
        ConnectionType::Push,
        rp1.identifier().clone(),
        "arg1".to_owned(),
        rp2.identifier().clone(),
    );
    // let manifeset =jvalue!({
    //     "id": "con1",
    //     "type": "push",
    // });

    // rp1 -> rp2
    let result1 = rp2.lock_mut()?.notify_connected_from(rp1.clone(), &manifest);
    assert!(result1.is_ok(), "Failed to connected_from function. Error is {:?}", result1.err());
    let result2 = rp1.lock_mut()?.try_connect_to(rp2.clone(), &manifest);
    assert!(result2.is_ok(), "Failed to connect_to function. Error is {:?}", result2.err());

    //let p =  
    let result_old = rp2.lock()?.get_output();
    //println!("hogehoge= {:?}", result_old.is_none());
    let f = result_old.is_empty().unwrap();
    assert!(f == true);

    let result1 = rp1.lock()?.execute();
    assert!(result1.is_ok(), "Error of ConnectionRack.execute(). Error is {:?}", result1.err());
    //let arc = result1.unwrap();
    let output = rp2.lock()?.get_output();
    //let result = juiz_lock(&output).unwrap();
    //assert_eq!(result.is_some(), true);
    //let v = result.unwrap();
    println!("value = {:?}", output);
    let iv = output.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    assert_eq!(iv, 3);
    Ok(())
}




#[cfg(test)]
#[test]
fn simple_connection_builder_invoke_test() -> JuizResult<()> {
    let (rp1, rp2) = setup_use_memo()?;

    // let manifest =jvalue!({
    //     "id": "con1",
    //     "type": "pull",
    // });
    let manifest = ConnectionManifest::new(
        ConnectionType::Pull,
        rp1.identifier().clone(),
        "arg1".to_owned(),
        rp2.identifier().clone(),
    );

    let result1 = connect(rp1.clone(), rp2.clone(), &manifest);
    // rp1 -> rp2
    assert!(result1.is_ok(), "Failed to ConnectionBuilder::connected function. Error is {:?}", result1.err());
    
    let result = rp2.lock()?.invoke();
    assert!(result.is_ok());
    let arc = result.unwrap();

    let iv = arc.lock_as_value(|value| { value.as_i64().unwrap() }).unwrap();
    assert_eq!(iv, 3);
    Ok(())
}