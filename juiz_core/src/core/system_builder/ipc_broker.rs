use juiz_sdk::anyhow::Context;

use crate::{
    brokers::{
        broker_factories_wrapper::BrokerFactoriesWrapper,
        ipc::{
            ipc_broker::create_ipc_broker_factory,
            ipc_broker_proxy::create_ipc_broker_proxy_factory,
        },
    },
    prelude::*,
};

#[allow(unused)]
pub fn setup_ipc_broker_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_local_broker_factory() called");
    let lbf = create_ipc_broker_factory(system.core_broker().clone())?;
    let lbpf = create_ipc_broker_proxy_factory()?;
    //juiz_lock(system.core_broker())?.store_mut().broker_proxies.register_factory(lbpf.clone())?;
    let _wrapper =
        system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, lbf, lbpf)?)?;
    Ok(())
}

#[allow(unused)]
pub fn setup_ipc_broker(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_ipc_broker() called");
    let ipc_broker = system
        .create_broker(&jvalue!({
            "type_name": "ipc",
            "name": "ipc"
        }))
        .context("system.create_broker() failed in system_builder::setup_ipc_broker()")?;
    system.register_broker(ipc_broker)?;
    log::info!("IpcBroker Created");
    Ok(())
}
