
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use juiz_sdk::{anyhow::anyhow, manifests::ContainerProfile};
use crate::prelude::*;


//pub type ContainerConstructFunction<T>=fn(ContainerManifest) -> JuizResult<Box<T>>;
//pub type ContainerConstructFunctionTrait<T>=dyn Fn(ContainerManifest) -> JuizResult<Box<T>>;

pub trait ContainerFactory : JuizObject + 'static {

    fn create_container(&self, core_worker: &mut CoreWorker, name: String, args: CapsuleMap) -> JuizResult<ContainerPtr>;

    fn destroy_container(&mut self, c: ContainerPtr) -> JuizResult<ContainerProfile>;
    
}

//pub type ContainerFactoryPtr = Arc<Mutex<dyn ContainerFactory>>;

#[derive(Clone)]
pub struct ContainerFactoryPtr {
    identifier: Identifier,
    ptr: Arc<RwLock<dyn ContainerFactory>>
}


impl ContainerFactoryPtr {

    pub fn new(cf: impl ContainerFactory) -> Self {
        let identifier = cf.identifier().clone();
        Self{
            identifier,
            ptr: Arc::new(RwLock::new(cf))
        }
    }

    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }
    
    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn ContainerFactory>> {
        self.ptr.read().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ContainerPtr".to_owned()})) })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn ContainerFactory>> {
        self.ptr.write().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ContainerPtr".to_owned()})) })
    }
}
