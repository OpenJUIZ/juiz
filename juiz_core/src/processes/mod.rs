

mod process_factory;
mod implementations;

pub use juiz_sdk::processes::{Process, ProcessBodyFunctionTrait,  ProcessBodyFunctionType};
pub use juiz_sdk::processes::ProcessPtr;
pub use process_factory::{ProcessFactory, ProcessFactoryPtr};
pub(crate) use implementations::{
    // process_from_clousure_new_with_class_name,
    // process_from_clousure,
    ProcessFactoryWrapper,
    ProcessFactoryImpl,
};

pub use implementations::{
    ProcessProxy,
    // process_new,
};

use crate::prelude::*;

pub(crate) use implementations::ProcessImpl;
pub use implementations::process_new;

pub fn process_factory_create(manifest: ProcessManifest, function: ProcessBodyFunctionType) -> JuizResult<ProcessFactoryPtr> {
    Ok(ProcessFactoryPtr::new(ProcessFactoryImpl::new(manifest, function)?))
}

pub fn process_factory_create_from_trait(manifest: ProcessManifest, function: impl Fn(CapsuleMap) -> JuizResult<Capsule> +'static ) -> JuizResult<ProcessFactoryPtr> {
    log::trace!("【呼出】process_factory_create_from_trait()");
    Ok(ProcessFactoryPtr::new(ProcessFactoryImpl::new_from_clousure(manifest, function)?))
}