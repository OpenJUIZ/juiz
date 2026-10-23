//! conf の namespace と同じ ID の二重登録 (issue #148)
use juiz_core::prelude::*;
use juiz_sdk::{manifests::ContainerProfile, process_identifier::ProcessIdentifier};

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("execute failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

fn plugins() -> Value {
    jvalue!({
        "process_factories": {
            "increment_process": { "path": "../target/debug" }
        },
        "container_factories": {
            "example_container": {
                "path": "../target/debug",
                "processes": {
                    "example_container_increment": { "path": "../target/debug" },
                }
            }
        }
    })
}

/// 同じ名前の Process・Container・EC を 2 つのネームスペースに置き、それぞれ呼び出せる
#[cfg(test)]
#[test]
fn test_same_conf_in_two_namespaces() -> JuizResult<()> {
    let manifest = jvalue!({
        "name": "test_system",
        "namespace": "robot1",
        "option": { "http_broker": { "start": false } },
        "plugins": plugins(),
        "processes": [
            // 相対指定なし => robot1/inc0, robot1/inc1
            { "type_name": "increment_process", "name": "inc0", "p_apply": { "arg1": 10 } },
            { "type_name": "increment_process", "name": "inc1" },
            // 絶対指定 => robot2/inc0, robot2/inc1
            { "type_name": "increment_process", "name": "inc0", "namespace": "/robot2", "p_apply": { "arg1": 20 } },
            { "type_name": "increment_process", "name": "inc1", "namespace": "/robot2", "p_apply": { "arg1": 0 } },
        ],
        "containers": [
            {
                "type_name": "example_container", "name": "container0", "initial_value": 100,
                "processes": [ { "type_name": "example_container_increment", "name": "increment0" } ]
            },
            {
                "type_name": "example_container", "name": "container0", "namespace": "/robot2", "initial_value": 200,
                "processes": [ { "type_name": "example_container_increment", "name": "increment0" } ]
            },
        ],
        "ecs": [
            {
                "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0", "auto_start": false,
                "bind": [ { "type_name": "increment_process", "name": "inc1" } ]
            },
            {
                "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0", "namespace": "/robot2", "auto_start": false,
                "bind": [ { "type_name": "increment_process", "name": "inc1" } ]
            },
        ],
        "connections": [
            // 名前での参照は Connection のネームスペース (既定の robot1) で解決する
            {
                "connection_type": "Pull",
                "source": { "type_name": "increment_process", "name": "inc0" },
                "destination": { "type_name": "increment_process", "name": "inc1" },
                "arg_name": "arg1"
            },
        ],
    });

    let _r = env_logger::try_init();
    Ok(System::new(manifest)?
        .setup()?
        .run_and_do_once(|system| {
            let r1_inc0: ProcessIdentifier =
                "core://core/process/robot1/inc0::increment_process".try_into()?;
            let r2_inc0: ProcessIdentifier =
                "core://core/process/robot2/inc0::increment_process".try_into()?;
            let r1_inc1: ProcessIdentifier =
                "core://core/process/robot1/inc1::increment_process".try_into()?;
            let r2_inc1: ProcessIdentifier =
                "core://core/process/robot2/inc1::increment_process".try_into()?;
            assert_eq!(r1_inc0.namespace, "robot1");

            let ids = system.core_broker().lock()?.process_list(false, None)?;
            assert_eq!(ids.len(), 4, "{ids:?}");
            assert!(ids.contains(&r1_inc0) && ids.contains(&r2_inc0), "{ids:?}");

            // それぞれ別の Process として呼び出せる
            assert_eq!(int_of(system.core_broker().lock_mut()?.process_execute(&r1_inc0)), 11);
            assert_eq!(int_of(system.core_broker().lock_mut()?.process_execute(&r2_inc0)), 21);
            // robot1/inc1 は robot1/inc0 につながっている。robot2/inc1 はつながっていない
            assert_eq!(int_of(system.core_broker().lock_mut()?.process_execute(&r1_inc1)), 12);
            assert_eq!(int_of(system.core_broker().lock_mut()?.process_execute(&r2_inc1)), 1);

            // profile の name は短い名前のまま、namespace が付く
            let prof = system.core_broker().lock()?.process_profile_full(&r2_inc0)?;
            assert_eq!(prof.name, "inc0");
            assert_eq!(prof.namespace, "robot2");
            assert_eq!(prof.identifier(), r2_inc0);

            // Container と ContainerProcess は所属 Container のネームスペースに置かれる
            let containers = system.core_broker().lock()?.container_list(false, None)?;
            assert_eq!(containers.len(), 2, "{containers:?}");
            let mut namespaces = containers
                .iter()
                .map(|c| c.namespace.clone())
                .collect::<Vec<String>>();
            namespaces.sort();
            assert_eq!(namespaces, vec!["robot1", "robot2"]);
            for c in containers.iter() {
                let cprof: ContainerProfile =
                    system.core_broker().lock()?.container_profile_full(c)?;
                assert_eq!(cprof.name, "container0");
                assert_eq!(cprof.namespace, c.namespace);
            }
            let r1_cp: ProcessIdentifier =
                "core://core/container_process/robot1/increment0::example_container_increment:container0"
                    .try_into()?;
            let r2_cp: ProcessIdentifier =
                "core://core/container_process/robot2/increment0::example_container_increment:container0"
                    .try_into()?;
            let cps = system.core_broker().lock()?.container_process_list(false, None)?;
            assert!(cps.contains(&r1_cp) && cps.contains(&r2_cp), "{cps:?}");
            let r1_value = int_of(system.core_broker().lock_mut()?.container_process_execute(&r1_cp));
            let r2_value = int_of(system.core_broker().lock_mut()?.container_process_execute(&r2_cp));
            assert!(r1_value > 100 && r1_value < 200, "{r1_value}");
            assert!(r2_value > 200, "{r2_value}");

            // EC も同じ名前で 2 つのネームスペースに置ける
            let ecs = system.core_broker().lock()?.ec_list(false)?;
            let mut ec_ids = get_array(&ecs)?
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<String>>();
            ec_ids.sort();
            assert_eq!(
                ec_ids,
                vec![
                    "core://core/execution_context/robot1/ec0::main_loop_ec",
                    "core://core/execution_context/robot2/ec0::main_loop_ec",
                ]
            );
            let ec_prof = system.core_broker().lock()?.ec_profile_full(&ec_ids[1])?;
            assert_eq!(ec_prof["name"], jvalue!("ec0"));
            assert_eq!(ec_prof["namespace"], jvalue!("robot2"));
            // bind は EC のネームスペースで解決される
            assert_eq!(
                ec_prof["targets"][0]["namespace"],
                jvalue!("robot2"),
                "{ec_prof}"
            );
            Ok(())
        })
        .expect("Error in System::run_and_do()"))
}

fn setup_error(objects: Value) -> String {
    let mut manifest = jvalue!({
        "name": "test_system",
        "option": { "http_broker": { "start": false } },
        "plugins": plugins(),
    });
    obj_merge_mut(&mut manifest, &objects).unwrap();
    let _r = env_logger::try_init();
    match System::new(manifest).unwrap().setup() {
        Ok(_) => panic!("setup must fail: {objects}"),
        Err(e) => format!("{e:?}"),
    }
}

/// 同じ ID の Process を 2 回登録するとエラー (今までは黙って上書きしていた)
#[cfg(test)]
#[test]
fn test_duplicate_process_is_error() {
    let e = setup_error(jvalue!({
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "increment_process", "name": "inc0" },
        ]
    }));
    assert!(
        e.contains("core://core/process/inc0::increment_process"),
        "{e}"
    );
    assert!(e.contains("already registered"), "{e}");

    // ネームスペースを省略したものと、既定のネームスペースを絶対指定したものは同じ ID
    let e = setup_error(jvalue!({
        "namespace": "robot1",
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "increment_process", "name": "inc0", "namespace": "/robot1" },
        ]
    }));
    assert!(
        e.contains("core://core/process/robot1/inc0::increment_process"),
        "{e}"
    );
}

/// 同じ ID の Container・EC を 2 回登録するとエラー
#[cfg(test)]
#[test]
fn test_duplicate_container_and_ec_is_error() {
    let e = setup_error(jvalue!({
        "containers": [
            { "type_name": "example_container", "name": "container0" },
            { "type_name": "example_container", "name": "container0" },
        ]
    }));
    assert!(e.contains("already registered"), "{e}");

    let e = setup_error(jvalue!({
        "ecs": [
            { "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0" },
            { "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0" },
        ]
    }));
    assert!(e.contains("already registered"), "{e}");
}

/// 名前・ネームスペースに / : | を使うとエラー
#[cfg(test)]
#[test]
fn test_invalid_name_is_error() {
    for (name, namespace) in [
        ("inc:0", ""),
        ("inc|0", ""),
        ("inc0", "robot:1"),
        ("inc0", "robot1//arm"),
    ] {
        let e = setup_error(jvalue!({
            "processes": [
                { "type_name": "increment_process", "name": name, "namespace": namespace },
            ]
        }));
        assert!(e.contains("Name is invalid"), "{e}");
    }
    let e = setup_error(jvalue!({
        "containers": [ { "type_name": "example_container", "name": "container|0" } ]
    }));
    assert!(e.contains("Name is invalid"), "{e}");
    let e = setup_error(jvalue!({
        "ecs": [ { "type_name": "main_loop_ec", "rate": 10.0, "name": "ec:0" } ]
    }));
    assert!(e.contains("Name is invalid"), "{e}");
}
