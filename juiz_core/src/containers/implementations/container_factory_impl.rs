

use std::sync::Arc;

use juiz_sdk::manifests::ContainerProfile;

use crate::prelude::*;

pub type ContainerConstructor = dyn Fn(ContainerManifest, CapsuleMap)->JuizResult<ContainerPtr>;
#[repr(C)]
pub struct ContainerFactoryImpl {
    core: ObjectCore,
    manifest: ContainerManifest,
    // constructor: Arc<ContainerConstructFunctionTrait<T>>,
    binded_container_constructor: Arc<ContainerConstructor>,
}

impl ContainerFactoryImpl {

    pub fn new(manifest: ContainerManifest, constructor: Arc<ContainerConstructor>) -> JuizResult<Self> {
        log::trace!("new({manifest:?}, constructor) called");
        Ok(ContainerFactoryImpl{
                core: ObjectCore::create_factory(JuizObjectClass::ContainerFactory("ContainerFactoryImpl"), manifest.type_name.clone().as_str()),
                manifest,
                // constructor: Arc::new(constructor),
                binded_container_constructor: constructor,
        })
    }

    // fn apply_default_manifest(&self, manifest: Value) -> Result<Value, JuizError> {
    //     let mut new_manifest = self.manifest.clone();
    //     for (k, v) in manifest.as_object().unwrap().iter() {
    //         new_manifest.as_object_mut().unwrap().insert(k.to_owned(), v.clone());
    //     }
    //     return Ok(new_manifest);
    // }
}


impl JuizObjectCoreHolder for ContainerFactoryImpl {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ContainerFactoryImpl {
    fn profile_full(&self) -> JuizResult<Value> {
        let mut v = self.core.profile_full()?;
        let vv = self.manifest.arguments.iter().map(|v|{ serde_json::to_value(v) }).collect::<serde_json::Result<Vec<Value>>>()?;
        obj_merge_mut(&mut v, &jvalue!({
            "arguments": vv,
            "language": self.manifest.language,
        }))?;
        Ok(v)
    }
}

impl ContainerFactory for ContainerFactoryImpl {

    fn create_container(&self, _core_worker: &mut CoreWorker, _name: String, mut args: CapsuleMap) -> JuizResult<ContainerPtr>{
        log::trace!("ContainerFactoryImpl::create_container(manifest={:?}) called", args);
        //println!("create_container");
        // ここでデフォルトを与えていく
        // println!("manifest========{:?}", self.manifest);
        for arg in self.manifest.arguments.iter() {
            match args.get(arg.name.as_str()) {
                Ok(_) => {}
                Err(_) => {
                    println!("set default {arg:?}");
                    args.insert(arg.name.clone(), arg.default.clone().into());
                }
            }
        }
        let mut manifest = self.manifest.clone();
        let name_cap = args.get("name")?;

        let name_str = name_cap.lock_as_value(|v| -> JuizResult<String> {
            Ok(v.as_str().unwrap().to_owned())
        })??;
        manifest = manifest.name(name_str.as_str());
        (self.binded_container_constructor)(manifest, args)
    }
    
    fn destroy_container(&mut self, c: ContainerPtr) -> JuizResult<ContainerProfile> {
        // todo!()
        log::trace!("ContainerFractoryImpl::destroy_container() called");
        c.lock()?.profile()
    }
    
}
