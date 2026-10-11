mod implementations;
mod process_factory;

pub(crate) use implementations::{
    ProcessFactoryImpl,
    // process_from_clousure_new_with_class_name,
    // process_from_clousure,
    ProcessFactoryWrapper,
};
pub use juiz_sdk::processes::ProcessPtr;
pub use juiz_sdk::processes::{Process, ProcessBodyFunctionTrait, ProcessBodyFunctionType};
pub use process_factory::{ProcessFactory, ProcessFactoryPtr};

pub use implementations::{
    ProcessProxy,
    // process_new,
};

use crate::prelude::*;

pub use implementations::process_new;
pub(crate) use implementations::ProcessImpl;

pub fn process_factory_create(
    manifest: ProcessManifest,
    function: ProcessBodyFunctionType,
) -> JuizResult<ProcessFactoryPtr> {
    Ok(ProcessFactoryPtr::new(ProcessFactoryImpl::new(
        manifest, function,
    )?))
}

pub fn process_factory_create_from_trait(
    manifest: ProcessManifest,
    function: impl Fn(CapsuleMap) -> JuizResult<Capsule> + 'static,
) -> JuizResult<ProcessFactoryPtr> {
    log::trace!("【呼出】process_factory_create_from_trait()");
    Ok(ProcessFactoryPtr::new(
        ProcessFactoryImpl::new_from_clousure(manifest, function)?,
    ))
}
