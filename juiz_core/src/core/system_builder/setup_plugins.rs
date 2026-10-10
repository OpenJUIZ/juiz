use juiz_sdk::anyhow::Context;

use crate::{core::system_builder::{brokers::setup_broker_factories, components::setup_components, containers::setup_container_factories, ecs::{setup_execution_context_factories, setup_main_loop_ec_factory}, processes::setup_process_factories}, prelude::*};

pub(crate) fn setup_plugins(system: &mut System, manifest: &Value, option: &Value) -> JuizResult<()> {
    log::trace!("system_builder::setup_plugins({}) called", manifest);

    let _ = when_contains_do_mut(manifest, "broker_factories", |v| {
        setup_broker_factories(system, v).with_context(||format!("system_builder::setup_broker_factories(manifest={manifest:}) failed."))
    })?;
    let _ = when_contains_do(manifest, "process_factories", |v| {
        setup_process_factories(system, v, option).with_context(||format!("system_builder::setup_process_factories(manifest={manifest:}) failed."))
    })?;
    let _ = when_contains_do(manifest, "container_factories", |v| {
        setup_container_factories(system, v, option).with_context(||format!("system_builder::setup_container_factories(manifest={manifest:}) failed."))
    })?;
    let _ = when_contains_do_mut(manifest, "components", |v| {
        setup_components(system, v, option).with_context(||format!("system_builder::setup_component_factories(manifest={manifest:}) failed."))
    })?;
    let _ = when_contains_do(manifest, "ec_factories", |v| {
        setup_execution_context_factories(system, v).with_context(||format!("system_builder::setup_execution_context_factories(manifest={manifest:}) failed."))
    })?;

    setup_main_loop_ec_factory(system).with_context(||format!("system_builder::setup_main_loop_ec_factory() failed."))?;
    log::trace!("【終了】system_builder::setup_plugins()");
    Ok(())
}



