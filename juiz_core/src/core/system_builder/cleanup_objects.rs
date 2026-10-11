use crate::{
    core::system_builder::{
        brokers::cleanup_brokers, containers::cleanup_containers, ecs::cleanup_ecs,
        processes::cleanup_processes,
    },
    prelude::*,
};
use juiz_sdk::anyhow::Context;

pub(crate) fn cleanup_objects(system: &mut System) -> JuizResult<()> {
    log::trace!("System::cleanup() called");
    cleanup_containers(system)
        .context("system_builder::cleanup_cotainers in System::cleanup() failed")?;
    cleanup_processes(system)
        .context("system_builder::cleanup_processes in System::cleanup() failed")?;
    cleanup_ecs(system).context("system_builder::cleanup_ecs in System::cleanup() failed")?;
    cleanup_brokers(system)
        .context("system_builder::cleanup_brokers in System::cleanup() failed")?;
    log::trace!("System::cleanup() exit");
    Ok(())
}
