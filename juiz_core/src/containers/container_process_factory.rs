
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::fmt::Debug;
use crate::prelude::*;
use juiz_sdk::{anyhow::anyhow, manifests::ProcessProfile};
/// ContainerProcessを生成するためのFactoryクラスのtrait
/// 
/// 
pub trait ContainerProcessFactory : JuizObject + Debug + 'static {

    /// ContainerProcessを生成
    /// 
    fn create_container_process(&self, container: ContainerPtr, manifest: &ProcessManifest) -> JuizResult<ProcessPtr>;

    fn destroy_container_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessProfile>;
}

// pub type ContainerProcessFactoryPtr = Arc<Mutex<dyn ContainerProcessFactory>>;



#[derive(Clone, Debug)]
pub struct ContainerProcessFactoryPtr {
    identifier: Identifier,
    type_name: String,
    ptr: Arc<RwLock<dyn ContainerProcessFactory>>,
}

impl std::fmt::Display for ContainerProcessFactoryPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ContainerProcessFactoryPtr(type_name={}, identifier={})", self.type_name, self.identifier))
    }
}

impl ContainerProcessFactoryPtr {

    pub fn new(proc: impl ContainerProcessFactory) -> Self {
        let identifier = proc.identifier().clone();
        ContainerProcessFactoryPtr{
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
    
    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn ContainerProcessFactory>> {
        self.ptr.read().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ProcessPtr".to_owned()})) })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn ContainerProcessFactory>> {
        self.ptr.write().or_else(|_|{ Err(anyhow!(JuizError::ObjectLockError{target:"ProcessPtr".to_owned()})) })
    }

}