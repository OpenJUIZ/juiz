

use std::sync::{Arc, Mutex};
use juiz_sdk::anyhow;
use juiz_sdk::object::JuizObject;
use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::prelude::*;
use crate::brokers::BrokerProxy;

use super::execution_context_function::ExecutionContextFunction;


#[allow(unused)]
pub struct ExecutionContextProxy {
    core: ObjectCore,
    broker_proxy: Arc<Mutex<dyn BrokerProxy>>,
    identifier: Identifier,
    class_name_str: String,
}

impl ExecutionContextProxy {

    pub fn new(class_name: JuizObjectClass, identifier: &Identifier, broker_proxy: Arc<Mutex<dyn BrokerProxy>>) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
        log::trace!("ECProxy::new({class_name:?}, {identifier}, broker_proxy) called");
        let id_struct = IdentifierStruct::try_from(identifier.clone())?;
        let class_name_str = match class_name {
            JuizObjectClass::ExecutionContext(_) => Ok("execution_context"),
            _ => {Err(anyhow::Error::from(JuizError::ExecutionContextProxyCanNotAcceptClassError{class_name: class_name.as_str().to_string()}))}
        }?;
        log::trace!("id_struct: {id_struct:?}");
        Ok(Arc::new(Mutex::new(ExecutionContextProxy{
            core: ObjectCore::new(identifier.clone(), class_name, id_struct.type_name.as_str(), id_struct.object_name.as_str(), id_struct.broker_name.as_str(), id_struct.broker_type_name.as_str()),
            broker_proxy,
            identifier: identifier.clone(),
            class_name_str: class_name_str.to_string(),
        })))
    }
}

impl JuizObjectCoreHolder for ExecutionContextProxy {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ExecutionContextProxy {

    fn profile_full(&self) -> JuizResult<Value> {
        let id = self.identifier();
        log::trace!("ProcessProxy({id})::profile_full() called");
        juiz_lock(&self.broker_proxy)?.ec_profile_full(&self.identifier())
    }
}

impl ExecutionContextFunction for ExecutionContextProxy {

    fn start(&mut self) -> JuizResult<Value> {
        let id = self.identifier();
        log::trace!("ExecutionContextProxy({id})::start() called");
        juiz_lock(&self.broker_proxy)?.ec_start(&self.identifier())
    }

    fn stop(&mut self) -> JuizResult<Value> {
        todo!()
    }

    fn get_state(&self) -> JuizResult<super::execution_context_core::ExecutionContextState> {
        todo!()
    }

    fn bind(&mut self, _target_process: ProcessPtr) -> JuizResult<()> {
        todo!()
    }

    fn unbind(&mut self, _target_process_id: ProcessIdentifier) -> JuizResult<()> {
        todo!()
    }
}
