mod container_process;

mod container_factory;
mod container_process_factory;
mod implementations;

use crate::prelude::*;

pub use container_factory::{ContainerFactory, ContainerFactoryPtr};
pub use container_process_factory::{ContainerProcessFactory, ContainerProcessFactoryPtr};

use implementations::{BindedContainerFunctionType, ContainerProcessFactoryImpl};
pub(crate) use implementations::{ContainerFactoryWrapper, ContainerProcessFactoryWrapper};

pub use implementations::{ContainerImpl, ContainerProxy};

// pub fn container_factory_create<S: 'static>(manifest: ContainerManifest, constructor: ContainerConstructFunction<S>) -> JuizResult<ContainerFactoryPtr> {

//     Ok(ContainerFactoryPtr::new(ContainerFactoryImpl::new(manifest, constructor)?))
// }

// pub fn container_factory_create_with_trait<S: 'static>(manifest: ContainerManifest, constructor: impl Fn(ContainerManifest) -> JuizResult<Box<S>> + 'static) -> JuizResult<ContainerFactoryPtr> {
//     Ok(ContainerFactoryPtr::new(ContainerFactoryImpl::new(manifest, constructor)?))
// }

// pub fn container_process_factory_create<S: 'static>(manifest: ProcessManifest, constructor: &'static ContainerFunctionType<S>) -> JuizResult<ContainerProcessFactoryPtr> {
//     Ok(ContainerProcessFactoryPtr::new(ContainerProcessFactoryImpl::new(manifest, constructor)?))
// }

// pub fn container_process_factory_create_from_trait<S: 'static>(manifest: ProcessManifest, constructor: impl Fn(&mut ContainerImpl<S>, CapsuleMap) -> JuizResult<Capsule> + 'static) -> JuizResult<ContainerProcessFactoryPtr> {
//     Ok(ContainerProcessFactoryPtr::new(ContainerProcessFactoryImpl::new_t(manifest, Arc::new(constructor))?))
// }

pub fn container_process_factory_create_from_trait(
    manifest: ProcessManifest,
    constructor: BindedContainerFunctionType,
) -> JuizResult<ContainerProcessFactoryPtr> {
    Ok(ContainerProcessFactoryPtr::new(
        ContainerProcessFactoryImpl::new_t(manifest, constructor)?,
    ))
}

pub use implementations::bind_container_function;
pub use implementations::container_factory_create;
pub use implementations::container_process_factory_create;
