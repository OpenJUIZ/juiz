use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{prelude::*};
use anyhow::anyhow;



#[derive(Clone)]
pub struct ContainerPtr {
    identifier: ContainerIdentifier,
    ptr: Arc<RwLock<dyn Container>>,
}

impl std::fmt::Debug for ContainerPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerPtr").field("identifier", &self.identifier).finish()
    }
}

impl ContainerPtr {

    pub fn new(container: impl Container) -> Self {
        ContainerPtr{
            identifier: container.identifier().clone(),
            ptr: Arc::new(RwLock::new(container))
        }
    }

    pub fn identifier(&self) -> ContainerIdentifier {
        self.identifier.clone()
    }

    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn Container>> {
        self.ptr.read().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ContainerPtr".to_owned()})) })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn Container>> {
        self.ptr.write().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ContainerPtr".to_owned()})) })
    }

    
    // pub fn downcast_and_then<T: 'static, R, F>(&self, func: F) -> JuizResult<R> where F: FnOnce(&ContainerImpl<T>)->R {
    //     match self.lock()?.downcast_ref::<ContainerImpl<T>>() {
    //         None => Err(anyhow::Error::from(JuizError::ContainerDowncastingError{identifier: self.identifier.clone()})),
    //         Some(container_impl) => { 
    //             Ok(func(container_impl))
    //         }
    //     }
    // }

    // pub fn downcast_mut_and_then<T: 'static, R, F>(&self, func: F) -> JuizResult<R> where F: FnOnce(&mut ContainerImpl<T>)->R {
    //     match self.lock_mut()?.downcast_mut::<ContainerImpl<T>>() {
    //         None => Err(anyhow::Error::from(JuizError::ContainerDowncastingError{identifier: self.identifier.clone()})),
    //         Some(container_impl) => { 
    //             Ok(func(container_impl))
    //         }
    //     }
    // }
}
