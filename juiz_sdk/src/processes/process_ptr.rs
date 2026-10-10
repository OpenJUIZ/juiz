

/// 
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use anyhow::anyhow;
use crate::{prelude::*, process_identifier::ProcessIdentifier};

use super::Process;

#[derive(Clone)]
pub struct ProcessPtr {
    identifier: ProcessIdentifier,
    // type_name: String,
    ptr: Arc<RwLock<dyn Process>>,
}

impl std::fmt::Debug for ProcessPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessPtr").field("identifier", &self.identifier).field("ptr", &self.ptr.read().unwrap()).finish()
    }
}

impl ProcessPtr {

    pub fn new(proc: impl Process) -> Self {
        let identifier = proc.identifier();
        ProcessPtr{
            identifier,
           //  type_name: proc.type_name().to_owned(),
            ptr: Arc::new(RwLock::new(proc))
        }
    }

    pub fn identifier(&self) -> ProcessIdentifier {
        self.identifier.clone()
    }

    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn Process>> {
        self.ptr.read().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ProcessPtr".to_owned()})) })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn Process>> {
        self.ptr.write().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ProcessPtr".to_owned()})) })
    }

    pub fn downcast_and_then<T: 'static + Process, R, F>(&self, func: F) -> JuizResult<R> where F: FnOnce(&T)->R {
        match self.lock()?.downcast_ref::<T>() {
            None => Err(anyhow::Error::from(JuizError::ContainerDowncastingError{identifier: self.identifier.to_string()})),
            Some(container_impl) => { 
                Ok(func(container_impl))
            }
        }
    }

    pub fn downcast_mut_and_then<T: 'static + Process, R, F>(&self, func: F) -> JuizResult<R> where F: FnOnce(&mut T)->R {
        match self.lock_mut()?.downcast_mut::<T>() {
            None => Err(anyhow::Error::from(JuizError::ContainerDowncastingError{identifier: self.identifier.to_string()})),
            Some(container_impl) => { 
                Ok(func(container_impl))
            }
        }
    }
}
