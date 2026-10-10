use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use juiz_sdk::anyhow::anyhow;
use crate::prelude::*;

pub trait ProcessFactory: JuizObject + 'static  {
    fn create_process(&self, manifest: &ProcessManifest) -> JuizResult<ProcessPtr>;
}

#[derive(Clone)]
pub struct ProcessFactoryPtr {
    identifier: Identifier,
    type_name: String,
    ptr: Arc<RwLock<dyn ProcessFactory>>,
}

impl ProcessFactoryPtr {

    pub fn new(proc: impl ProcessFactory) -> Self {
        let identifier = proc.identifier().clone();
        ProcessFactoryPtr{
            identifier,
            type_name: proc.type_name().to_owned(),
            ptr: Arc::new(RwLock::new(proc))
        }
    }

    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }

    pub fn type_name(&self) -> &str {
        self.type_name.as_str()
    }
    
    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn ProcessFactory>> {
        self.ptr.read().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ProcessPtr".to_owned()})) })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn ProcessFactory>> {
        self.ptr.write().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ProcessPtr".to_owned()})) })
    }

}