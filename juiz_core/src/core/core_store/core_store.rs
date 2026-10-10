use std::collections::HashMap;
use super::buffer_object_collection::BufferObjectCollection;
use super::object_collection::ObjectCollection;
use super::mutex_object_collection::MutexObjectCollection;

use crate::topics::TopicPtr;
use crate::prelude::*;
use crate::ecs::{execution_context_function::ExecutionContextFunction, execution_context_holder_factory::ExecutionContextHolderFactory};

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::manifests::ContainerIdentifier;
use juiz_sdk::manifests::{ContainerProfile, ProcessProfile};
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::topic_identifier::TopicIdentifier;

// #[derive(Debug)]
pub struct CoreStore {
    manifest: Value,
    broker_factories_manifests: HashMap<Identifier, Value>,
    brokers_manifests: HashMap<Identifier, Value>,
    pub topics: HashMap<Identifier, TopicPtr>,

    pub processes: Box<ObjectCollection::<ProcessPtr, ProcessFactoryPtr>>,
    pub containers: Box<ObjectCollection::<ContainerPtr, ContainerFactoryPtr>>,
    pub container_processes: Box<ObjectCollection::<ProcessPtr, ContainerProcessFactoryPtr>>,
    pub ecs: Box<MutexObjectCollection::<dyn ExecutionContextFunction, ExecutionContextHolderFactory>>,
    pub broker_proxies: Box<BufferObjectCollection::<dyn BrokerProxy, dyn BrokerProxyFactory>>,
}


impl CoreStore {
    pub fn new(manifest: Value) -> CoreStore {
        CoreStore{
            manifest,
            brokers_manifests: HashMap::new(),
            broker_proxies: BufferObjectCollection::new("broker_proxy"),
            broker_factories_manifests: HashMap::new(),
            topics: HashMap::new(),
            //processes: RwObjectCollection::new("process"), 
            processes: ObjectCollection::new("process"), 
            //containers: RwObjectCollection::new("container"), 
            containers: ObjectCollection::new("container"), 
            //container_processes: RwObjectCollection::new("container_process"), 
            container_processes: ObjectCollection::new("container_process"), 
            ecs: MutexObjectCollection::new("ecs"),
        }
    }

    pub fn manifest(&self) -> Value {
        self.manifest.clone()
    }

    pub fn manifest_mut(&mut self) -> &mut Value {
        &mut self.manifest
    }

    pub fn get_opt_mut(&mut self) -> &mut Value {
        let manif_obj = self.manifest.as_object_mut().unwrap();
        if manif_obj.contains_key("option") {
            manif_obj.get_mut("option").unwrap()
        } else {
            manif_obj.insert("option".to_owned(), jvalue!({}));
            manif_obj.get_mut("option").unwrap()        
        }
    }

    pub fn clear(&mut self) -> JuizResult<()> {
        log::trace!("CoreStore::clear() called");
        self.clear_container_process_factories()?;
        self.clear_container_factories()?;
        self.clear_process_factories()?;
        self.clear_broker_factories()?;
        Ok(())
    }
    
    fn clear_container_process_factories(&mut self) -> JuizResult<()> {
        log::trace!("clear_container_process_factories() called");
        self.container_processes.clear()?;
        Ok(())
    }
    
    
    fn clear_container_factories(&mut self) -> JuizResult<()> {
        log::trace!("clear_container_factories() called");
        self.containers.clear()?;
        Ok(())
    }

    fn clear_process_factories(&mut self) -> JuizResult<()> {
        log::trace!("clear_broker_factories() called");
        self.processes.clear()?;
        Ok(())
    }

    fn clear_broker_factories(&mut self) -> JuizResult<()> {
        log::trace!("clear_broker_factories() called");
        self.broker_factories_manifests.clear();
        self.brokers_manifests.clear();
        self.broker_proxies.clear()?;
        Ok(())
    }

    pub fn register_broker_manifest(&mut self, type_name: &str, b: Value) -> JuizResult<()> {
        self.brokers_manifests.insert(type_name.to_string(), b);
        Ok(())
    }

    pub fn register_broker_factory_manifest(&mut self, type_name: &str, b: Value) -> JuizResult<()> {
        log::trace!("core_store::register_broker_factory_manifest(type_name={type_name:?}) called");
        self.broker_factories_manifests.insert(type_name.to_string(), b);
        Ok(())
    }

    
    fn broker_factories_profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!(self.broker_factories_manifests))
    }

    pub fn broker_profile_full(&self, id: &Identifier) -> JuizResult<Value> {
        match self.brokers_manifests.get(id) {
            Some(p) => Ok(p.clone()),
            None => {
                Err(anyhow::Error::from(JuizError::BrokerProfileNotFoundError{id: id.to_string()}))
            }
        }
    }

    pub fn brokers_profile_full(&self) -> JuizResult<Value> {
        Ok(self.brokers_manifests.iter().map(| (_k, v) | { v.clone() }).collect())
    }

    pub fn brokers_list_ids(&self) -> JuizResult<Vec<String>> {
        self.brokers_manifests.values().into_iter().map(|pv| {
            Ok(obj_get_str(pv, "identifier")?.to_owned())
        }).collect()
    }

    pub fn topics_list_ids(&self) -> JuizResult<Vec<TopicIdentifier>> {
        self.topics.values().into_iter().map(|topic| {
            topic.name().to_owned().try_into()
        }).collect()
    }

    pub fn topics_profile_full(&self) -> JuizResult<Vec<Value>> {
        self.topics.values().into_iter().map(|t| {
            t.profile()
        }).collect()
    }

    pub fn processes_profile_full(&self) -> JuizResult<Vec<ProcessProfile>> {
        log::trace!("process_profile_full() called");
        self.processes.objects().iter().map(|(_k, c)| {
            c.lock().and_then(|co| { 
                //let id = co.identifier().clone();
                co.profile()
            })} ).collect()
     }

    pub fn containers_profile_full(&self) -> JuizResult<Vec<ContainerProfile>> {
       self.containers.objects().iter().map(|(_k, c)| {
            c.lock()
                .and_then(|co| { 
                    //let id = co.identifier().clone();
                    Ok(co.profile()?)
                })
            } ).collect()
    }

    pub fn container_processes_profile_full(&self) -> JuizResult<Vec<ProcessProfile>> {
        self.container_processes.objects().iter().map(|(_k, c)| {
            c.lock()
                .and_then(|co| { 
                    co.profile()
                })
            } ).collect()
    }

    pub fn processes_id(&self) -> Vec<ProcessIdentifier> {
        log::debug!("process_id()が呼ばれました");
        self.processes.objects().iter().map(|(_k, c)| {
            c.identifier()
        }).collect()
    }

    pub fn containers_id(&self) -> Vec<ContainerIdentifier> {
        self.containers.objects().iter().map(|(_k, c)| {
         c.identifier().clone()
         } ).collect()
    }

    pub fn container_processes_id(&self) -> Vec<ProcessIdentifier> {
        self.container_processes.objects().iter().map(|(_k, c)| {
            c.identifier()
        }).collect()
    }

    pub fn process_factories_profile_full(&self) -> JuizResult<Vec<Value>> {
        self.processes.factories().iter().map(|(_k, c)| {
         c.lock().or_else(|e|{Err(anyhow!(JuizError::ObjectLockError{target:e.to_string()}))}).and_then(|co| { co.profile_full() })
         } ).collect()
    }

    pub fn container_factories_profile_full(&self) -> JuizResult<Vec<Value>> {
        self.containers.factories().iter().map(|(_k, c)| {
         c.lock().or_else(|e|{Err(anyhow!(JuizError::ObjectLockError{target:e.to_string()}))}).and_then(|co| { co.profile_full() })
         } ).collect()
    }

    pub fn container_process_factories_profile_full(&self) -> JuizResult<Vec<Value>> {
        self.container_processes.factories().iter().map(|(_k, c)| {
         c.lock().or_else(|e|{Err(anyhow!(JuizError::ObjectLockError{target:e.to_string()}))}).and_then(|co| { co.profile_full() })
         } ).collect()
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        let r = self.broker_proxies.list_ids()?;
        Ok(jvalue!({
            "process_factories": self.process_factories_profile_full()?,
            "container_factories": self.container_factories_profile_full()?,
            "container_process_factories": self.container_process_factories_profile_full()?,
            "processes": self.processes_profile_full()?,
            "containers": self.containers_profile_full()?,
            "container_processes": self.container_processes_profile_full()?,
            "brokers": self.brokers_profile_full()?,
            "broker_factories": self.broker_factories_profile_full()?,
            "broker_proxies": r,
            "ecs": self.ecs.objects_profile_full()?,
            "ec_factories": self.ecs.factories_profile_full()?,
            "topics": self.topics_profile_full()?,
        }))
    }
}