use crate::prelude::*;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::Broker;

use juiz_sdk::anyhow::anyhow;

#[derive(Clone)]
pub struct BrokerPtr {
    identifier: BrokerIdentifier,
    ptr: Arc<RwLock<dyn Broker>>,
}

impl BrokerPtr {
    pub fn new(br: impl Broker) -> Self {
        let identifier = br.identifier().clone();
        BrokerPtr {
            identifier,
            ptr: Arc::new(RwLock::new(br)),
        }
    }

    pub fn identifier(&self) -> &BrokerIdentifier {
        &self.identifier
    }

    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn Broker>> {
        self.ptr.read().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "BrokerPtr".to_owned()
            }))
        })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn Broker>> {
        self.ptr.write().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "BrokerPtr".to_owned()
            }))
        })
    }
}
