use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

#[cfg(test)]
#[test]
fn test_system_rust_no_load_but_two_brokers() -> JuizResult<()> {
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

    crate::common::logger::init();
    Ok(System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
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
        })
        .expect("Error in System::run_and_do()"))
}

#[cfg(test)]
#[test]
fn test_system_rust_process_load() -> JuizResult<()> {
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

    crate::common::logger::init();
    Ok(System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
            let r_procs = system.core_broker().lock()?.process_list(false, None);
            assert!(r_procs.is_ok());
            let procs = r_procs.unwrap();
            assert_eq!(procs.len(), 1, "Processes are {:?}", procs);

            let id = "core://core/process/increment_a::increment_process"
                .to_owned()
                .try_into()?;
            let result = system
                .core_broker()
                .lock_mut()?
                .worker_mut()
                .process_proxy_from_identifier(&id, false);
            assert!(
                result.is_ok(),
                "Process({:?}) can not be found. Processes are {:?}",
                id,
                procs
            );
            let process = result.unwrap();
            let result_prof = process.lock()?.profile();
            assert!(result_prof.is_ok(), "Process profile failed.");
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

#[cfg(test)]
#[test]
fn test_system_rust_container_load() -> JuizResult<()> {
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

    crate::common::logger::init();
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

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("execute failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// 設定ファイルの p_apply で、Process と ContainerProcess の引数に初期値を与えられる (issue #61)
#[cfg(test)]
#[test]
fn test_system_p_apply_in_manifest() -> JuizResult<()> {
    let manifest = jvalue!(
        {
            "name": "test_system",
            "option": {
                "http_broker" : {
                    "start": false
                }
            },
            "plugins": {
                "process_factories": {
                    "increment_process": {
                        "path": "../target/debug"
                    }
                },
                "container_factories": {
                    "example_container": {
                        "path": "../target/debug",
                        "processes": {
                            "example_container_increment": {
                                "path": "../target/debug",
                            }
                        }
                    }
                }
            },
            "processes": [
                {
                    "type_name": "increment_process",
                    "name": "inc0",
                    "p_apply": { "arg1": 10 }
                },
            ],
            "containers": [
                {
                    "type_name": "example_container",
                    "name": "container_a",
                    "initial_value": 100,
                    "processes": [
                        {
                            "type_name": "example_container_increment",
                            "name": "increment_a",
                            "p_apply": { "arg1": 5 }
                        }
                    ]
                }
            ],
        }
    );

    crate::common::logger::init();
    Ok(System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
            let id: ProcessIdentifier = "core://core/process/inc0::increment_process".try_into()?;
            // conf の p_apply が初期値として効く
            assert_eq!(
                int_of(system.core_broker().lock_mut()?.process_execute(&id)),
                11
            );
            // 後からの p_apply で上書きできる
            system
                .core_broker()
                .lock_mut()?
                .process_p_apply(&id, "arg1", jvalue!(20).into())?;
            assert_eq!(
                int_of(system.core_broker().lock_mut()?.process_execute(&id)),
                21
            );

            let cid: ProcessIdentifier =
                "core://core/container_process/increment_a::example_container_increment:container_a"
                    .try_into()?;
            assert_eq!(
                int_of(
                    system
                        .core_broker()
                        .lock_mut()?
                        .container_process_execute(&cid)
                ),
                105
            );
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

/// p_apply に存在しない引数を書いたら setup が失敗する (issue #61)
#[cfg(test)]
#[test]
fn test_system_p_apply_in_manifest_unknown_arg() -> JuizResult<()> {
    let manifest = jvalue!(
        {
            "name": "test_system",
            "option": { "http_broker" : { "start": false } },
            "plugins": {
                "process_factories": {
                    "increment_process": { "path": "../target/debug" }
                }
            },
            "processes": [
                {
                    "type_name": "increment_process",
                    "name": "inc0",
                    "p_apply": { "no_such_arg": 10 }
                },
            ]
        }
    );
    crate::common::logger::init();
    assert!(System::new(manifest)?.setup().is_err());
    Ok(())
}

/// Container を作った後に setup が失敗しても、System を捨てるときに落ちない。
/// 以前はプラグインのライブラリを閉じた後に Container を捨てて SIGSEGV になっていた (issue #148)
#[cfg(test)]
#[test]
fn test_system_setup_fails_after_container_created() -> JuizResult<()> {
    let manifest = jvalue!(
        {
            "name": "test_system",
            "option": { "http_broker" : { "start": false } },
            "plugins": {
                "container_factories": {
                    "example_container": { "path": "../target/debug" }
                }
            },
            "containers": [
                {
                    "type_name": "example_container",
                    "name": "container_a",
                    // 存在しない ContainerProcess の型で、Container を作った後に setup を失敗させる
                    "processes": [ { "type_name": "no_such_container_process", "name": "nothing" } ]
                }
            ],
        }
    );
    crate::common::logger::init();
    assert!(System::new(manifest)?.setup().is_err());
    Ok(())
}
