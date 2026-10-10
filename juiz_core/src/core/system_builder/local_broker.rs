
use std::sync::{mpsc, Arc, Mutex};
use juiz_sdk::anyhow::Context;

use crate::{brokers::{broker_factories_wrapper::BrokerFactoriesWrapper, local_broker::{create_local_broker_factory, BrokerSideSenderReceiverPair, ByteSenderReceiverPair, ProxySideSenderReceiverPair}, local_broker_proxy::create_local_broker_proxy_factory}, prelude::*};

pub(super) fn setup_local_broker_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_local_broker_factory() called");
    let (c_sender, c_receiver) = mpsc::channel::<CapsuleMap>();
    let (p_sender, p_receiver) = mpsc::channel::<CapsulePtr>();
    let (_c_b_sender, c_b_receiver) = mpsc::channel::<Vec<u8>>();
    let (p_b_sender, _p_b_receiver) = mpsc::channel::<Vec<u8>>();
    let lbf = create_local_broker_factory(system.core_broker().clone(), Arc::new(Mutex::new(BrokerSideSenderReceiverPair(p_sender, c_receiver))), Arc::new(Mutex::new(ByteSenderReceiverPair(p_b_sender, c_b_receiver))))?;
    let lbpf = create_local_broker_proxy_factory(Arc::new(Mutex::new(ProxySideSenderReceiverPair(c_sender, p_receiver))))?;
    //juiz_lock(system.core_broker())?.store_mut().broker_proxies.register_factory(lbpf.clone())?;
    let _wrapper = system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, lbf, lbpf)?)?;
    Ok(())
}




pub(super) fn setup_local_broker(system: &mut System) -> JuizResult<()> {
    log::trace!("【呼出】system_builder::setup_local_broker()");
    let local_broker = system.create_broker(&jvalue!({
        "type_name": "local",
        "name": "local"
    })).context("system.create_broker() failed in system_builder::setup_local_broker()")?;
    system.register_broker(local_broker)?;
    log::info!("【生成】LocalBroker");
    Ok(())
}