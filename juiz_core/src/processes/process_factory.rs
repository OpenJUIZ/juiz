use crate::prelude::*;
use crate::processes::ProcessBodyFunctionTrait;
use juiz_sdk::anyhow::anyhow;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait ProcessFactory: JuizObject + 'static {
    fn create_process(&self, manifest: &ProcessManifest) -> JuizResult<ProcessPtr>;

    /// Factory の manifest (型の情報)。Composite の子の検査に使う (issue #167)
    fn manifest(&self) -> JuizResult<ProcessManifest>;

    /// 本体関数。Composite が子の本体を合成するときに取り出す (issue #167)
    fn body(&self) -> JuizResult<Arc<ProcessBodyFunctionTrait>>;
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
        ProcessFactoryPtr {
            identifier,
            type_name: proc.type_name().to_owned(),
            ptr: Arc::new(RwLock::new(proc)),
        }
    }

    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }

    pub fn type_name(&self) -> &str {
        self.type_name.as_str()
    }

    pub fn lock(&self) -> JuizResult<RwLockReadGuard<dyn ProcessFactory>> {
        self.ptr.read().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "ProcessPtr".to_owned()
            }))
        })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<dyn ProcessFactory>> {
        self.ptr.write().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "ProcessPtr".to_owned()
            }))
        })
    }
}
