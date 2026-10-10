
use std::sync::{Arc, Mutex};

use juiz_sdk::manifests::ContainerProfile;

use crate::prelude::*;
//use crate::{containers::ContainerImpl};

pub type ContainerStackConstructFunction<S>=fn(ContainerPtr, ContainerManifest) -> JuizResult<Box<S>>;



#[repr(C)]
pub struct ContainerStackFactoryImpl<T: 'static> {
    core: ObjectCore,
    manifest: ContainerManifest,
    constructor: ContainerStackConstructFunction<T>
}

#[allow(unused)]
impl<T: 'static> ContainerStackFactoryImpl<T> {

    pub fn new(manifest: ContainerManifest, constructor: ContainerStackConstructFunction<T>) -> JuizResult<Self> {
        //let type_name = obj_get_str(&manifest, "type_name")?;
        Ok(ContainerStackFactoryImpl::<T>{
                core: ObjectCore::create_factory(JuizObjectClass::ContainerFactory("ContainerStackFactoryImpl"), manifest.type_name.clone()),
                manifest, //: check_process_factory_manifest(manifest)?,
                constructor
        })
    }

    pub fn create(manifest: ContainerManifest, constructor: ContainerStackConstructFunction<T>) -> JuizResult<Arc<Mutex<dyn ContainerFactory>>> {
        Ok(Arc::new(Mutex::new(Self::new(manifest, constructor)?)))
    }

    // fn apply_default_manifest(&self, manifest: Value) -> Result<Value, JuizError> {
    //     let mut new_manifest = self.manifest.clone();
    //     for (k, v) in manifest.as_object().unwrap().iter() {
    //         new_manifest.as_object_mut().unwrap().insert(k.to_owned(), v.clone());
    //     }
    //     return Ok(new_manifest);
    // }
}


impl<T: 'static> JuizObjectCoreHolder for ContainerStackFactoryImpl<T> {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl<T: 'static> JuizObject for ContainerStackFactoryImpl<T> {}

impl<T: 'static> ContainerFactory for ContainerStackFactoryImpl<T> {

    fn create_container(&self, _core_worker: &mut CoreWorker, _name: String, manifest: CapsuleMap) -> JuizResult<ContainerPtr>{
        log::trace!("ContainerStackFactory::create_container(manifest={:?}) called", manifest);
        // //let parent_id = obj_get_str(&manifest, "parent_container")?.to_owned();
        // //let parent_manifest = obj_get(&manifest, "parent_container")?;
        // //let parent_container = core_worker.container_from_identifier(&parent_id)?;
        // let parent_container_manifest = manifest.parent_container_manifest();
        // let parent_container = core_worker.container_from_manifest(&parent_container_manifest.into())?;
        // Ok(ContainerPtr::new(ContainerImpl::new_with_parent(
        //     manifest.clone(),
        //     // self.apply_default_manifest(manifest.clone())?,
        //     (self.constructor)(parent_container.clone(), manifest)?,
        //     parent_container,
        // )?))
        todo!()
    }
    
    fn destroy_container(&mut self, c: ContainerPtr) -> JuizResult<ContainerProfile> {
        log::trace!("ContainerFractoryImpl::destroy_container() called");
        c.lock()?.profile()
    }
    
}
