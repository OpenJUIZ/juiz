

use std::sync::{Arc, Mutex};
use juiz_sdk::{anyhow, connections::{ConnectionManifest, ConnectionProfile}, manifests::ProcessProfile, process_identifier::ProcessIdentifier};
use crate::prelude::*;
use juiz_sdk::prelude::*;
use crate::brokers::BrokerProxy;

#[allow(unused)]
pub struct ProcessProxy {
    // core: ObjectCore,
    broker_proxy: Arc<Mutex<dyn BrokerProxy>>,
    identifier: ProcessIdentifier,
    //profile: ProcessProfile,
    class_name_str: String,
}

impl ProcessProxy {

    pub fn new(class_name: JuizObjectClass, identifier: ProcessIdentifier, broker_proxy: Arc<Mutex<dyn BrokerProxy>>) -> JuizResult<ProcessPtr> {
        log::trace!("【呼出】ProcessProxy::new({class_name:}, {identifier}, broker_proxy)");
        // let id_struct = IdentifierStruct::try_from(identifier.clone())?;
        let class_name_str = match class_name {
            JuizObjectClass::Process(_) => Ok("process"),
            JuizObjectClass::ContainerProcess(_) => Ok("container_process"),
            _ => {Err(anyhow::Error::from(JuizError::ProcessProxyCanNotAcceptClassError{class_name: class_name.as_str().to_string()}))}
        }?;
        log::debug!("【作成】ProcessProxy({class_name}, {identifier})");
        Ok(ProcessPtr::new(ProcessProxy{
            broker_proxy,
            identifier,
            class_name_str: class_name_str.to_string(),
        }))
    }
}

// impl JuizObjectCoreHolder for ProcessProxy {
//     fn core(&self) -> &ObjectCore {
//         &self.core
//     }
// }
// impl JuizObject for ProcessProxy {

//     fn profile_full(&self) -> JuizResult<Value> {
//         let id = self.identifier();
//         log::trace!("ProcessProxy({id})::profile_full() called");
//         juiz_lock(&self.broker_proxy)?.any_process_profile_full(self.identifier())
//     }
// }

impl std::fmt::Debug for ProcessProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessProxy").field("identifier", &self.identifier).field("class_name_str", &self.class_name_str).finish()
    }
}

impl Process for ProcessProxy {
    
    fn call(&self, args: CapsuleMap) -> JuizResult<CapsulePtr> {
        log::trace!("ProcessProxy({})::call() called", self.identifier());
        juiz_lock(&self.broker_proxy)?.any_process_call(&self.identifier(), args).and_then(|v| {
            log::trace!("【完了】ProcessProxy({})::call()", self.identifier());
            Ok(v)
        }).or_else(|e| {
            log::error!("【失敗】ProcessProxy({})::call()", self.identifier());
            Err(e)
        })
    }

    fn is_updated(& self) -> JuizResult<bool> {
        todo!()
    }

    fn profile(&self) -> JuizResult<ProcessProfile> {
        juiz_lock(&self.broker_proxy)?.any_process_profile_full(&self.identifier())
    }

    fn invoke<'b>(&self) -> JuizResult<CapsulePtr> {
        todo!()
    }

    fn execute(&self) -> JuizResult<CapsulePtr> {
        todo!()
    }

    fn push_by(&self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】ProcessProxy::push_by({arg_name}, {value})");
        juiz_lock(&self.broker_proxy)?.process_push_by(&self.identifier(), arg_name.to_owned(), value).and_then(|v| {
            log::trace!("【完了】ProcessProxy({})::push_by({})", self.identifier(), arg_name);
            Ok(v)
        }).or_else(|e| {
            log::error!("【失敗】ProcessProxy({})::push_by({})", self.identifier(), arg_name);
            Err(e)
        })
    }

    fn get_output(&self) -> CapsulePtr {
        todo!()
    }

    fn notify_connected_from<'b>(&'b mut self, _source: ProcessPtr, manifest: &ConnectionManifest) -> JuizResult<ConnectionProfile> {
        log::trace!("【呼出】ProcessProxy::notify_connected_from({manifest})");
        juiz_lock(&self.broker_proxy)?.process_notify_connected_from(&manifest).and_then(|v| {
            log::trace!("【完了】ProcessProxy({})::notify_connect_to()", self.identifier());
            Ok(v)
        }).or_else(|e| {
            log::error!("【失敗】ProcessProxy({})::notify_connect_to()", self.identifier());
            Err(e)
        })
    }

    fn try_connect_to(&mut self, _destination: ProcessPtr, manifest: &ConnectionManifest) -> JuizResult<ConnectionManifest> {
        log::trace!("【呼出】ProcessProxy::try_connect_to({manifest})");
        juiz_lock(&self.broker_proxy)?.process_try_connect_to(&manifest).and_then(|v| {
            log::trace!("【完了】ProcessProxy({})::try_connect_to()", self.identifier());
            Ok(v)
        }).or_else(|e| {
            log::error!("【失敗】ProcessProxy({})::try_connect_to()", self.identifier());
            Err(e)
        })
    }

    fn source_connections(&self) -> JuizResult<Vec<&Box<dyn SourceConnection>>> {
        log::trace!("【呼出】ProcessProxy::source_connections()");
        todo!("source_connectionsはまだ実装されていません。")
    }

    fn destination_connections(&self) -> JuizResult<Vec<&Box<dyn DestinationConnection>>> {
        log::trace!("【呼出】ProcessProxy::destination_connections()");
        todo!("destination_connectionsはまだ実装されていません。")
    }


    fn p_apply(&mut self, arg_name: &str, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】ProcessProxy({})::p_aplly({arg_name}, {value:})", self.identifier());
        juiz_lock(&self.broker_proxy)?.process_p_apply(&self.identifier(), arg_name, value).and_then(|v| {
            log::trace!("【完了】ProcessProxy({})::p_aplly({arg_name})", self.identifier());
            Ok(v)
        }).or_else(|e| {
            log::error!("【失敗】ProcessProxy({})::p_aplly({arg_name})", self.identifier());
            Err(e)
        })
    }
    
    fn purge(&mut self) -> JuizResult<()> {
        log::trace!("【呼出】ProcessProxy({})::purge()", self.identifier());
        todo!()
    }
    
    fn identifier(&self) -> ProcessIdentifier {
        let id = self.identifier.clone();
        log::debug!("【identifier】id = {id}");
        return id;
    }
    
    fn openapi_spec(&self) -> JuizResult<Value> {
        log::trace!("【呼出】ProcessProxy({})::openapi_spec()", self.identifier());
        todo!()
    }
    
}
