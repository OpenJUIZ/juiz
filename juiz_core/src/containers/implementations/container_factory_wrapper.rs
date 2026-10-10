use std::cell::RefCell;

use juiz_sdk::manifests::ContainerIdentifier;
use juiz_sdk::manifests::ContainerProfile;

use crate::prelude::*;
use crate::plugin::{JuizObjectPlugin, Plugin};

#[allow(dead_code)]
pub struct ContainerFactoryWrapper {
    core: ObjectCore,
    container_factory: ContainerFactoryPtr,
    containers: RefCell<Vec<ContainerPtr>>,
    plugin: JuizObjectPlugin,
}

impl ContainerFactoryWrapper {
    
    pub fn new(plugin: JuizObjectPlugin, container_factory: ContainerFactoryPtr) -> JuizResult<Self> {
        let type_name = container_factory.lock()?.type_name().to_owned();
        Ok(ContainerFactoryWrapper{
            core: ObjectCore::create_factory(JuizObjectClass::ContainerFactory("ContainerFactoryWrapper"), type_name),
            plugin,
            container_factory,
            containers: RefCell::new(vec![])
        })
    }

}

impl JuizObjectCoreHolder for ContainerFactoryWrapper {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ContainerFactoryWrapper {

    fn profile_full(&self) -> JuizResult<Value> {
        let prof = match &self.plugin {
            JuizObjectPlugin::Rust(p) => p.profile_full().unwrap(),
            JuizObjectPlugin::Python(p) => p.profile_full().unwrap(),
            JuizObjectPlugin::Cpp(p) => p.profile_full().unwrap(),
        };
        let container_prof = self.container_factory.lock()?.profile_full()?;
        let lang = obj_get_str(&container_prof, "language")?.to_owned();
        //let prof = self.rust_plugin.as_ref().and_then(|p|{ Some(p.profile_full().unwrap()) }).or_else(|| { self.python_plugin.as_ref().and_then( |p| {Some(p.profile_full().unwrap())}).or( Some(self.cpp_plugin.as_ref().unwrap().profile_full().unwrap())) });
        Ok(obj_merge(self.core.profile_full()?, &jvalue!(
            {
                "__class__": "ContainerFactoryWrapper",
                "plugin": prof,
                "language": lang,
                "container_factory": container_prof,
                
            }
        ))?.into())
    }
}

impl ContainerFactory for ContainerFactoryWrapper {
    
    fn create_container(&self, core_worker: &mut CoreWorker, name: String, args: CapsuleMap) -> JuizResult<ContainerPtr> {
        log::trace!("ContainerFactoryWrapper::create_container(name={name}, argst={:?}) called", args);
        let p = self.container_factory.lock()?.create_container(core_worker, name, args)?;
        self.containers.borrow_mut().push(p.clone());
        Ok(p)
    }


    fn destroy_container(&mut self, c: ContainerPtr) -> JuizResult<ContainerProfile> {
        let prof = c.lock()?.profile()?;
        let id: ContainerIdentifier = prof.identifier();
        log::trace!("ContainerFactoryWrapper::destroy_container(manifest={}) called", prof);
        let index = self.containers.borrow().iter().enumerate().find(|rc| rc.1.lock().unwrap().identifier() == id).unwrap().0;
        self.containers.borrow_mut().remove(index);
        self.container_factory.lock_mut()?.destroy_container(c)
    }
}

impl Drop for ContainerFactoryWrapper {

    fn drop(&mut self) {
        log::trace!("ContainerFactoryWrapper()::drop() called");
    }
}