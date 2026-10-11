use std::sync::{Arc, Mutex};

use crate::brokers::BrokerProxy;
use crate::core::CoreWorker;
use crate::prelude::*;

pub trait BrokerProxyFactory: JuizObject {
    fn create_broker_proxy(
        &self,
        core_worker: &CoreWorker,
        manifest: Value,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>>;

    fn profile_full(&self) -> JuizResult<Value>;
}

pub struct BrokerProxyFactoryImpl {
    core: ObjectCore,
    create_function: fn(&CoreWorker, Value) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>>,
}

impl BrokerProxyFactoryImpl {
    pub fn new(
        manifest: Value,
        create_function: fn(
            core_broker: &CoreWorker,
            manifest: Value,
        ) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>>,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
        let class_name = "BrokerPRoxyFactoryImpl";
        let type_name = obj_get_str(&manifest, "type_name")?;
        Ok(Arc::new(Mutex::new(BrokerProxyFactoryImpl {
            core: ObjectCore::create_factory(
                JuizObjectClass::BrokerProxyFactory(class_name),
                type_name,
            ),
            create_function,
        })))
    }
}

impl JuizObjectCoreHolder for BrokerProxyFactoryImpl {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for BrokerProxyFactoryImpl {}

impl BrokerProxyFactory for BrokerProxyFactoryImpl {
    fn create_broker_proxy(
        &self,
        core_broker: &CoreWorker,
        manifest: Value,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        log::trace!(
            "BrokerProxyFactoryImpl::create_broker_proxy(manifest={}) called",
            manifest
        );
        (self.create_function)(core_broker, manifest)
        /*
        let object_name = obj_get_str(&manifest, "name").context("BrokerProxyFactoryImpll::create_broker_proxy")?;
        let class_name = "BrokerProxy";
        let type_name = self.type_name();
        */
    }

    fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "type_name": self.type_name()
        }))
    }
}

pub fn create_broker_proxy_factory_impl(
    manifest: Value,
    create_broker_function: fn(&CoreWorker, Value) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>>,
) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    Ok(BrokerProxyFactoryImpl::new(
        manifest,
        create_broker_function,
    )?)
}
