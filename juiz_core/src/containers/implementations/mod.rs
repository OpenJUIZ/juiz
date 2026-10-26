mod composite_container;
mod container_factory_impl;
mod container_factory_wrapper;
mod container_process_factory_impl;
mod container_process_factory_wrapper;
mod container_process_ptr;
mod container_proxy;
mod container_stack_factory;

use std::sync::Arc;

use container_factory_impl::{ContainerConstructor, ContainerFactoryImpl};
pub use container_factory_wrapper::ContainerFactoryWrapper;
pub use container_process_factory_impl::ContainerProcessFactoryImpl;
pub use container_process_factory_wrapper::ContainerProcessFactoryWrapper;
pub use container_proxy::ContainerProxy;
pub use juiz_sdk::containers::ContainerImpl;

pub use container_process_factory_impl::bind_container_function;
use juiz_sdk::prelude::*;

use super::{ContainerFactoryPtr, ContainerProcessFactoryPtr};
pub use container_process_factory_impl::BindedContainerFunctionType;

pub use composite_container::{
    composite_container_new, CompositeContainer, CompositeContainerProcessFactory,
};

pub fn container_factory_create(
    manifest: ContainerManifest,
    constructor: Arc<ContainerConstructor>,
) -> JuizResult<ContainerFactoryPtr> {
    Ok(ContainerFactoryPtr::new(ContainerFactoryImpl::new(
        manifest,
        constructor,
    )?))
}

pub fn container_process_factory_create(
    manifest: ProcessManifest,
    constructor: Arc<dyn Fn(ContainerPtr, CapsuleMap) -> JuizResult<Capsule> + 'static>,
) -> JuizResult<ContainerProcessFactoryPtr> {
    Ok(ContainerProcessFactoryPtr::new(
        ContainerProcessFactoryImpl::new_t(manifest, constructor)?,
    ))
}

/// スロットを持つ Composite の Factory を作る (issue #168)
///
/// `lookup` は子の type_name から、同じ juiz プロセスに登録された ProcessFactory か ContainerProcessFactory を返す。
/// システムに登録するときは [`CoreWorker::register_composite_container`] を使う。
pub fn composite_container_process_factory_create<F>(
    composite: &juiz_sdk::manifests::CompositeManifest,
    lookup: F,
) -> JuizResult<ContainerProcessFactoryPtr>
where
    F: Fn(&str) -> Option<crate::processes::CompositeChildFactory>,
{
    Ok(ContainerProcessFactoryPtr::new(
        CompositeContainerProcessFactory::new(composite, lookup)?,
    ))
}
