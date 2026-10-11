use anyhow::Context;
use juiz_sdk::anyhow;
use std::sync::{Arc, Mutex};

use crate::core::CoreWorker;

use crate::brokers::{
    BrokerProxy, BrokerProxyFactory, MessengerBrokerProxy, MessengerBrokerProxyCoreFactory,
};
use crate::prelude::*;

pub fn create_messenger_broker_proxy_factory(
    impl_class_name: &'static str,
    type_name: &str,
    core_factory: Box<dyn MessengerBrokerProxyCoreFactory>,
) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    log::trace!("create_messenger_broker_factory called");
    MessengerBrokerProxyFactory::new(
        ObjectCore::create_factory(
            JuizObjectClass::BrokerProxyFactory(impl_class_name),
            type_name,
        ),
        core_factory,
    )
}
pub struct MessengerBrokerProxyFactory {
    core: ObjectCore,
    core_factory: Box<dyn MessengerBrokerProxyCoreFactory>,
}

impl MessengerBrokerProxyFactory {
    pub fn new(
        core: ObjectCore,
        core_factory: Box<dyn MessengerBrokerProxyCoreFactory>,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
        Ok(Arc::new(Mutex::new(MessengerBrokerProxyFactory {
            core,
            core_factory,
        })))
    }
}

impl JuizObjectCoreHolder for MessengerBrokerProxyFactory {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for MessengerBrokerProxyFactory {}

impl BrokerProxyFactory for MessengerBrokerProxyFactory {
    fn create_broker_proxy(
        &self,
        _core_broker: &CoreWorker,
        manifest: Value,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        log::trace!(
            "MessengerBrokerProxyFactory::create_broker_proxy(manifest={}) called",
            manifest
        );
        let object_name = obj_get_str(&manifest, "name")
            .context("LocalBrokerProxyFactory::create_broker_proxy")?;
        let class_name = "BrokerProxy";
        let type_name = self.type_name();
        MessengerBrokerProxy::new(
            class_name,
            type_name,
            object_name,
            self.core_factory.create_core(object_name)?,
        )
    }

    fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "type_name": self.type_name()
        }))
    }
}
