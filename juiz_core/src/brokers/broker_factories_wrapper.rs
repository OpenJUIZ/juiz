use juiz_sdk::anyhow;

use crate::{
    brokers::{BrokerFactory, BrokerProxy, BrokerProxyFactory},
    plugin::RustPlugin,
};
use crate::{core::CoreWorker, prelude::*};
use juiz_sdk::utils::juiz_lock;
use std::sync::{Arc, Mutex, RwLock};

use super::broker_ptr::BrokerPtr;

#[allow(dead_code)]
pub struct BrokerFactoriesWrapper {
    type_name: String,
    pub broker_factory: Arc<Mutex<dyn BrokerFactory>>,
    pub broker_proxy_factory: Arc<Mutex<dyn BrokerProxyFactory>>,
    plugin: Option<RustPlugin>,
}

impl BrokerFactoriesWrapper {
    pub fn new(
        plugin: Option<RustPlugin>,
        broker_factory: Arc<Mutex<dyn BrokerFactory>>,
        broker_proxy_factory: Arc<Mutex<dyn BrokerProxyFactory>>,
    ) -> JuizResult<Arc<Mutex<BrokerFactoriesWrapper>>> {
        let type_name_bf = juiz_lock(&broker_factory)?.type_name().to_string();
        let type_name_bpf = juiz_lock(&broker_proxy_factory)?.type_name().to_string();
        if type_name_bf != type_name_bpf {
            return Err(anyhow::Error::from(
                JuizError::BrokerFactoryAndBrokerProxyFactoryWithDifferentTypeIsRegisteredError {
                    type_name_bf: type_name_bf,
                    type_name_bpf: type_name_bpf,
                },
            ));
        }

        Ok(Arc::new(Mutex::new(BrokerFactoriesWrapper {
            type_name: type_name_bpf.to_string(),
            plugin,
            broker_factory,
            broker_proxy_factory,
        })))
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        log::trace!("profile_full(type_name={:}) called", self.type_name);
        juiz_lock(&self.broker_factory)?.profile_full()
    }

    pub fn type_name(&self) -> &str {
        &self.type_name.as_str()
    }

    pub fn create_broker(&self, manifest: &Value) -> JuizResult<BrokerPtr> {
        juiz_lock(&self.broker_factory)?.create_broker(manifest.clone())
    }

    pub fn create_broker_proxy(
        &self,
        core_broker: &CoreWorker,
        manifest: &Value,
    ) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
        log::trace!("create_broker_proxy(manifest='{manifest:}') called");
        juiz_lock(&self.broker_proxy_factory)?.create_broker_proxy(core_broker, manifest.clone())
    }
}

impl Drop for BrokerFactoriesWrapper {
    fn drop(&mut self) {
        log::trace!("BrokerFactoriesWrapper({})::drop() called", self.type_name);
    }
}
