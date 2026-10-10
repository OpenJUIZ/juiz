use std::sync::{Arc, Mutex};
use crate::prelude::*;
use crate::plugin::RustPlugin;

use super::{execution_context_factory::ExecutionContextFactory, execution_context_function::ExecutionContextFunction, execution_context_holder::ExecutionContextHolder};

#[allow(unused)]
pub struct ExecutionContextHolderFactory {
    core: ObjectCore,
    ec_factory: Arc<Mutex<dyn ExecutionContextFactory>>,
    plugin: Option<RustPlugin>,
    //tokio_runtime: &'static tokio::runtime::Runtime,
}

impl ExecutionContextHolderFactory {
    pub fn new(plugin: Option<RustPlugin>, ec_factory: Arc<Mutex<dyn ExecutionContextFactory>>) -> JuizResult<Arc<Mutex<ExecutionContextHolderFactory>>> {
        let type_name = juiz_lock(&ec_factory)?.type_name().to_string();
        Ok(Arc::new(Mutex::new(
            ExecutionContextHolderFactory{
                plugin,
                core: ObjectCore::create_factory(JuizObjectClass::ExecutionContextFactory("ExecutionContextHolderFactory"), type_name.as_str()),
                ec_factory,
            }
        )))
    }


    pub fn create(&self, manifest: Value) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
        let f = juiz_lock(&self.ec_factory)?;
        let auto_start = match obj_get_bool(&manifest, "auto_start") {
            Ok(v) => v,
            Err(_) => false
        };
        Ok(
            ExecutionContextHolder::new(
                f.type_name(), 
                //self.tokio_runtime,
                f.create(manifest)?, 
                auto_start
            )?
        )
    }
}

impl JuizObjectCoreHolder for ExecutionContextHolderFactory {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ExecutionContextHolderFactory {
    
}