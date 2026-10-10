use std::cell::RefCell;
use crate::prelude::*;
use crate::{plugin::JuizObjectPlugin};

#[allow(dead_code)]
pub struct ProcessFactoryWrapper {
    core: ObjectCore,
    process_factory: ProcessFactoryPtr,
    processes: RefCell<Vec<ProcessPtr>>,
    plugin: JuizObjectPlugin,
}

impl ProcessFactoryWrapper {
    
    pub fn new(plugin: JuizObjectPlugin, process_factory: ProcessFactoryPtr) -> JuizResult<Self> {
        Ok(ProcessFactoryWrapper{
            core: ObjectCore::create_factory(JuizObjectClass::ProcessFactory("ProcessFactoryWrapper"), process_factory.type_name()),
            plugin,
            process_factory,
            processes: RefCell::new(vec![])
        })
    }
}

impl JuizObjectCoreHolder for ProcessFactoryWrapper {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ProcessFactoryWrapper {

    fn profile_full(&self) -> JuizResult<Value> {
        log::trace!("ProcessFactoryWrapper({:})::profile_full() called", self.type_name());
        let prof = self.plugin.profile_full().unwrap();
        let v = self.core.profile_full()?;
        let proc_prof = self.process_factory.lock()?.profile_full()?;
        let lang = obj_get_str(&proc_prof, "language")?.to_owned();
    
        Ok(obj_merge(v, &jvalue!({
            "plugin": prof,
            "language": lang,
            "given_process_factory": proc_prof,
        }))?.into())
    }

}


impl ProcessFactory for ProcessFactoryWrapper {

    fn create_process(&self, manifest: &ProcessManifest) -> JuizResult<ProcessPtr> {
        log::trace!("ProcessFactoryWrapper::create_process(manifest={:?}) called", manifest);
        let p = self.process_factory.lock()?.create_process(manifest)?;
        self.processes.borrow_mut().push(p.clone());
        Ok(p)
    }


    
}

impl Drop for ProcessFactoryWrapper {

    fn drop(&mut self) {
        log::trace!("ProcecssFactoryWrapper()::drop() called");
    }
}