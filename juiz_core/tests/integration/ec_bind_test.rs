//! conf の ecs の bind で ContainerProcess を名前で指定する (issue #184)
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

fn manifest(bind: Value) -> Value {
    jvalue!({
        "name": "test_system",
        "namespace": "robot1",
        "option": { "http_broker": { "start": false } },
        "plugins": {
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
        },
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
        ],
        "containers": [
            {
                "type_name": "example_container", "name": "container0", "initial_value": 100,
                "processes": [ { "type_name": "example_container_increment", "name": "increment0" } ]
            },
        ],
        "ecs": [
            { "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0", "auto_start": false, "bind": bind },
        ],
    })
}

fn bound_targets(bind: Value) -> JuizResult<Vec<String>> {
    crate::common::logger::init();
    let mut targets = Vec::new();
    System::new(manifest(bind))?
        .setup()?
        .run_and_do_once(|system| {
            let prof = system.core_broker().lock()?.ec_profile_full(
                &"core://core/execution_context/robot1/ec0::main_loop_ec".to_owned(),
            )?;
            targets = get_array(&prof["targets"])?
                .iter()
                .map(|t| {
                    let id: ProcessIdentifier = serde_json::from_value(t.clone()).unwrap();
                    id.to_string()
                })
                .collect();
            Ok(())
        })?;
    Ok(targets)
}

/// name / type_name / container_name で書いた ContainerProcess を bind できる
#[cfg(test)]
#[test]
fn test_bind_container_process_by_name() -> JuizResult<()> {
    let targets = bound_targets(jvalue!([
        { "name": "increment0", "type_name": "example_container_increment", "container_name": "container0" },
        { "name": "inc0", "type_name": "increment_process" },
    ]))?;
    assert_eq!(
        targets,
        vec![
            "core://core/container_process/robot1/increment0::example_container_increment:container0",
            "core://core/process/robot1/inc0::increment_process",
        ]
    );
    Ok(())
}

/// ID の文字列でも今まで通り bind できる
#[cfg(test)]
#[test]
fn test_bind_container_process_by_id() -> JuizResult<()> {
    let targets = bound_targets(jvalue!([
        "core://core/container_process/robot1/increment0::example_container_increment:container0",
    ]))?;
    assert_eq!(
        targets,
        vec!["core://core/container_process/robot1/increment0::example_container_increment:container0"]
    );
    Ok(())
}
