

use std::sync::{Arc, Mutex};
use juiz_sdk::anyhow::Context;

use crate::{brokers::broker_factories_wrapper::BrokerFactoriesWrapper, brokers::CoreBrokerPtr, plugin::{concat_dirname, plugin_name_to_file_name, RustPlugin}, prelude::*};

pub(super) fn setup_broker_factories(system: &mut System, manifest: &Value) -> JuizResult<()> {
    log::trace!("system_builder::setup_broker_factories() called");
    for (name, v) in get_hashmap(manifest)?.iter() {
        log::debug!("BrokerFactory (name={name:}) Loading...");
        setup_broker_factory(system, manifest, name, v)?;
        log::info!("BrokerFactory (name={name:}) Loaded");
    }
    Ok(())
}


fn setup_broker_factory(system: &mut System, manifest: &Value, name: &String, v: &Value) -> JuizResult<()> {
    log::trace!("setup_broker_factory(name={name:}) called");
    let working_dir = None;
    let plugin_filename = concat_dirname(v, plugin_name_to_file_name(&name.to_string()), working_dir)?;
    let bf;
    let bpf;
    unsafe {
        let plugin: RustPlugin = RustPlugin::load(plugin_filename)?;
        {
            type BrokerFactorySymbolType<'a> = libloading::Symbol<'a, unsafe extern "Rust" fn(CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>>>;
            type BrokerProxyFactorySymbolType<'a> = libloading::Symbol<'a, unsafe extern "Rust" fn() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>>;
            let symbol_bf = plugin.load_symbol::<BrokerFactorySymbolType>(b"broker_factory")?;
            bf = (symbol_bf)(system.core_broker().clone()).with_context(||format!("calling symbol 'broker_factory'. arg is {manifest:}"))?;
            log::trace!("BrokerFactory (type_name={:?}) created.", juiz_lock(&bf)?.type_name());
            let symbol_bpf = plugin.load_symbol::<BrokerProxyFactorySymbolType>(b"broker_proxy_factory")?;
            bpf = (symbol_bpf)().with_context(||format!("calling symbol 'broker_proxy_factory'. arg is {manifest:}"))?;
            log::trace!("BrokerProxyFactory (type_name={:?}) created.", juiz_lock(&bpf)?.type_name());
        }
        system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(Some(plugin), bf, bpf)?)?;
    }
    Ok(())
}


pub fn setup_brokers(system: &mut System, manifest: &Value) -> JuizResult<()> {
    log::trace!("system_builder::setup_brokers() called");
    for p in get_array(manifest)?.iter() {
        let type_name = obj_get_str(p, "type_name")?;
        let name = obj_get_str(p, "name")?;
        let _ = system.create_broker(&p)?;
        log::info!("Broker({:}:{:}) Created", name, type_name);
    }
    Ok(())
}

pub fn setup_broker_proxies(system: &mut System, manifest: &Value) -> JuizResult<()> {
    log::trace!("system_builder::setup_broker_proxies() called");
    for p in get_array(manifest)?.iter() {
        let type_name = obj_get_str(p, "type_name")?;
        let name = obj_get_str(p, "name")?;
        let _ = system.create_broker_proxy(&p)?;
        log::info!("BrokerProxy({:}:{:}) Created", name, type_name);
    }
    Ok(())
}





pub fn cleanup_brokers(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::cleanup_brokers() called");
    let r = system.core_broker().lock_mut().and_then(|mut cb|{
        cb.worker_mut().store_mut().clear()
    });
    system.cleanup_brokers()?;
    log::trace!("system_builder::cleanup_brokers() exit");
    r
}