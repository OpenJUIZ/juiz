use juiz_sdk::anyhow::anyhow;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

use crate::brokers::broker_factories_wrapper::BrokerFactoriesWrapper;
use crate::brokers::broker_ptr::BrokerPtr;
use crate::prelude::*;

use super::CoreWorker;

#[allow(unused)]
pub struct SystemStore {
    pub broker_factories: HashMap<String, Arc<Mutex<BrokerFactoriesWrapper>>>,
    pub brokers: HashMap<String, BrokerPtr>,
    pub broker_proxies: HashMap<String, Arc<Mutex<dyn BrokerProxy>>>,
    pub uuid: Uuid,
}

impl SystemStore {
    pub fn new() -> Self {
        Self {
            uuid: Uuid::new_v4(),
            broker_factories: HashMap::new(),
            brokers: HashMap::new(),
            broker_proxies: HashMap::new(),
        }
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "uuid": self.uuid.to_string(),
            "broker_factories": self.broker_factories.keys().collect::<Vec<&String>>(),
            "brokers": self.brokers.keys().collect::<Vec<&String>>(),
            "broker_proxies": self.broker_proxies.keys().collect::<Vec<&String>>()
        }))
    }

    pub fn register_broker(&mut self, broker: BrokerPtr) -> JuizResult<BrokerPtr> {
        let type_name = broker.lock()?.identifier().type_name;
        self.brokers.insert(type_name, broker.clone());
        Ok(broker)
    }
}

#[derive(Clone)]
pub struct SystemStorePtr {
    ptr: Arc<RwLock<SystemStore>>,
}

impl SystemStorePtr {
    pub fn new(store: SystemStore) -> Self {
        Self {
            ptr: Arc::new(RwLock::new(store)),
        }
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        return self.lock()?.profile_full();
    }

    pub fn lock(&self) -> JuizResult<RwLockReadGuard<SystemStore>> {
        self.ptr.read().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "SystemStorePtr".to_owned()
            }))
        })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<SystemStore>> {
        self.ptr.write().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "SystemStorePtr".to_owned()
            }))
        })
    }

    pub fn create_broker_proxy(
        &self,
        core_broker: &CoreWorker,
        manifest: &Value,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        log::trace!("create_broker_proxy({manifest:}) called");
        let type_name = obj_get_str(manifest, "type_name")?;
        match self.lock()?.broker_factories.get(type_name) {
            Some(bf) => juiz_lock(bf)?
                .create_broker_proxy(core_broker, &manifest)
                .or_else(|e| {
                    log::error!("creating BrokerProxy(type_name={type_name}) failed. Error ({e})");
                    Err(e)
                }),
            None => Err(anyhow!(JuizError::FactoryCanNotFoundError {
                type_name: type_name.to_owned()
            })),
        }
    }

    pub fn uuid(&self) -> JuizResult<Uuid> {
        Ok(self.lock()?.uuid.clone())
    }
}
