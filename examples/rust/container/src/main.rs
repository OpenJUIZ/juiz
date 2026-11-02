use juiz_core::{jvalue, JuizResult, System};

fn main() -> JuizResult<()> {
    let manifest = jvalue!(
        {

            "name": "test_system",
            "plugins": {
                "process_factories": {
                    "increment_process": {
                        "path": "./target/debug"
                    }
                },
                "container_factories": {
                    "example_container": {
                        "path": "./target/debug",
                        "processes": {
                            "example_container_increment": {
                                "path": "./target/debug",
                            },
                            "example_container_get": {
                                "path": "./target/debug",
                            }
                        }
                    }
                },
                "broker_factories": {
                    "http_broker": {
                        "path": "./target/debug"
                    }
                }
            },
            "brokers": [
                {
                    "type_name": "http",
                    "name": "localhost:3000"
                }
            ],
            "processes": [
                {
                    "type_name": "increment_process",
                    "name": "increment_a"
                },
            ],
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

    Ok(System::new(manifest)?
        .run_and_do(|system| {
            println!("JuizSystem started!!");
            let v = system
                .broker_proxy(&jvalue!({"type_name":"local"}))?
                .read()
                .unwrap()
                .system_profile_full()?;
            println!("System: {:#}", v);

            let id =
                "http://localhost:3000/ContainerProcess/increment_a::example_container_increment";
            let p1 = system.container_process_proxy(&id.to_string())?;
            let prof = p1.read().unwrap().profile_full()?;
            println!("Process: {:#}", prof);

            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}
