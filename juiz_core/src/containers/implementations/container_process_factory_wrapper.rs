use std::cell::RefCell;


use juiz_sdk::manifests::ProcessProfile;

use crate::prelude::*;
use crate::plugin::{JuizObjectPlugin, Plugin};


#[allow(dead_code)]
pub struct ContainerProcessFactoryWrapper {
    core: ObjectCore,
    container_process_factory: ContainerProcessFactoryPtr,
    container_processes: RefCell<Vec<ProcessPtr>>,
    plugin: JuizObjectPlugin,
}

impl std::fmt::Debug for ContainerProcessFactoryWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerProcessFactoryWrapper").field("core", &self.core).field("container_process_factory", &self.container_process_factory).field("container_processes", &self.container_processes).field("plugin", &self.plugin).finish()
    }
}

impl ContainerProcessFactoryWrapper {


    pub fn new(plugin: JuizObjectPlugin, container_process_factory: ContainerProcessFactoryPtr) -> JuizResult<Self> {
        Ok(Self{
            core: ObjectCore::create_factory(JuizObjectClass::ContainerProcessFactory("ContainerProcessFactoryWrapper"), container_process_factory.type_name()),
            plugin,
            container_process_factory,
            container_processes: RefCell::new(vec![])
        })
    }

}

impl JuizObjectCoreHolder for ContainerProcessFactoryWrapper {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ContainerProcessFactoryWrapper {
    
    fn profile_full(&self) -> JuizResult<Value> {
        //let prof = self.rust_plugin.as_ref().and_then(|p|{ Some(p.profile_full().unwrap()) }).or( Some(self.python_plugin.as_ref().unwrap().profile_full().unwrap()) );
        let prof = match &self.plugin {
            JuizObjectPlugin::Rust(p) => p.profile_full().unwrap(),
            JuizObjectPlugin::Python(p) => p.profile_full().unwrap(),
            JuizObjectPlugin::Cpp(p) => p.profile_full().unwrap(),
        };
        let container_prof = self.container_process_factory.lock()?.profile_full()?;
        let lang = obj_get_str(&container_prof, "language")?.to_owned();
        Ok(obj_merge(self.core.profile_full()?.try_into()?, &jvalue!({
            "plugin": prof,
            "language": lang,
            "container_process_factory": container_prof,
            //container_processes: RefCell<Vec<Arc<Mutex<dyn ContainerProcess>>>>
        }))?.into())
    }
}


impl ContainerProcessFactory for ContainerProcessFactoryWrapper {

    fn create_container_process(&self, container: ContainerPtr, manifest: &ProcessManifest) -> JuizResult<ProcessPtr> {
        log::trace!("ContainerProcessFactoryWrapper::create_container_process(manifest={:?}) called", manifest);
        let p = self.container_process_factory.lock()?.create_container_process(container, manifest)?;
        self.container_processes.borrow_mut().push(p.clone());
        Ok(p.clone())
    }
    
    fn destroy_container_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessProfile> {
        let prof = p.lock()?.profile()?;
        // let id = obj_get_str(&prof, "identifier")?;
        let id = prof.identifier();
        log::trace!("ContainerProcessFactoryWrapper::destroy_container_process(identifier={}) called", id);
        let index = self.container_processes.borrow().iter().enumerate().find(|r| r.1.lock().unwrap().identifier() == id).unwrap().0;
        self.container_processes.borrow_mut().remove(index);
        let r = self.container_process_factory.lock_mut()?.destroy_container_process(p);
        log::trace!("ContainerProcessFactoryWrapper::destroy_container_process(identifier={}) exit", id);
        r
    }
}

impl Drop for ContainerProcessFactoryWrapper {

    fn drop(&mut self) {
        log::trace!("ContainerProcessFactoryWrapper()::drop() called");
    }
}