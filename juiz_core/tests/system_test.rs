
use juiz_core::prelude::*;

#[cfg(test)]
#[test]
fn test_system_rust_no_load_but_two_brokers() -> JuizResult<()>{

    let manifest = jvalue!(
        {
            "name": "test_system",
            "option": {
                "http_broker" : {
                    "start": true,
                    "port": 8001
                }
            },
            "plugins": {
            },
        }
    );

    let _r = env_logger::try_init();
    Ok(System::new(manifest)?.setup()?.run_and_do_once(|system|{
        println!("JuizSystem started!!");
        let r_brokers = system.core_broker().lock()?.broker_list(false);
        assert!(r_brokers.is_ok());
        let brokers = r_brokers.unwrap();
        assert_eq!(brokers.len(), 2, "Brokers are {:?}", brokers);

        let r_procs = system.core_broker().lock()?.process_list(false, None);
        assert!(r_procs.is_ok());
        let procs = r_procs.unwrap();
        assert_eq!(procs.len(), 0, "Processes are {:?}", procs);
        Ok(())
    }).expect("Error in System::run_and_do()"))
}

#[cfg(test)]
#[test]
fn test_system_rust_process_load() -> JuizResult<()>{

    let manifest = jvalue!(
        {
            "name": "test_system",
            "option": {
                "http_broker" : {
                    "start": true,
                    "port": 8002
                }
            },
            "plugins": {
                "process_factories": {
                    "increment_process": {
                        "path": "../target/debug"
                    }
                }
            },
            "processes": [
                {
                    "type_name": "increment_process",
                    "name": "increment_a"
                },
            ]
        }
    );

    let _r = env_logger::try_init();
    Ok(System::new(manifest)?.setup()?.run_and_do_once(|system|{
        let r_procs = system.core_broker().lock()?.process_list(false, None);
        assert!(r_procs.is_ok());
        let procs = r_procs.unwrap();
        assert_eq!(procs.len(), 1, "Processes are {:?}", procs);


        let id = "core://core/process/increment_a::increment_process".to_owned().try_into()?;
        let result = system.core_broker().lock_mut()?.worker_mut().process_proxy_from_identifier(&id, false);
        assert!(result.is_ok(), "Process({:?}) can not be found. Processes are {:?}", id, procs);
        let process = result.unwrap();
        let result_prof = process.lock()?.profile();
        assert!(result_prof.is_ok(), "Process profile failed.");
        Ok(())
    }).expect("Error in System::run_and_do()"))
}

#[cfg(test)]
#[test]
fn test_system_rust_container_load() -> JuizResult<()>{

    let manifest = jvalue!(
        {
            "name": "test_system",
            "option": {
                "http_broker" : {
                    "start": true,
                    "port": 8003
                }
            },
            "plugins": {
                "container_factories": {
                    "example_container": {
                        "path": "../target/debug",
                        "processes": {
                            "example_container_increment": {
                                "path": "../target/debug",
                            },
                            "example_container_get": {
                                "path": "../target/debug",
                            }
                        }
                    }
                }
            },
            "containers": [
                {
                    "type_name": "example_container",
                    "name": "container_a",
                    "processes": [
                        {
                            "type_name": "example_container_increment",
                            "name": "increment_a",
                        },
                        {
                            "type_name": "example_container_get",
                            "name": "get_a"
                        }
                    ]
                }
            ],
        }
    );

    let _r = env_logger::try_init();
    Ok(System::new(manifest)?.setup()?.run_and_do_once(|system|{
        println!("JuizSystem started!!");

        let r_conts = system.core_broker().lock()?.container_list(false, None);
        assert!(r_conts.is_ok());
        let conts = r_conts.unwrap();
        assert_eq!(conts.len(), 1, "Containers are {:?}", conts);


        let r_procs = system.core_broker().lock()?.container_process_list(false, None);
        assert!(r_procs.is_ok());
        let procs = r_procs.unwrap();
        assert_eq!(procs.len(), 2, "ContainerProcesses are {:?}", procs);


        let id = "core://core/container_process/increment_a::example_container_increment:container_a".to_owned().try_into()?;
        let result = system.core_broker().lock_mut()?.worker_mut().container_process_proxy_from_identifier(&id);
        assert!(result.is_ok(), "Process({:?}) can not be found.", id);
        let process = result.unwrap();
        let result_prof = process.lock()?.profile();
        assert!(result_prof.is_ok(), "Process profile failed.");
        Ok(())
    }).expect("Error in System::run_and_do()"))
}