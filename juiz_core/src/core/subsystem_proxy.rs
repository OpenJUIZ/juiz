use std::{fmt::Display, sync::{Arc, Mutex}};

use uuid::Uuid;

use crate::prelude::*;

#[allow(unused)]
#[derive(Clone)]
pub struct SubSystemProxy {
    uuid: Uuid, 
    broker_proxy: Arc<Mutex<dyn BrokerProxy>>,
    profile: Value,
}

impl Display for SubSystemProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("SubSystemProxy(uuid={}, broker_proxy={})", self.uuid.to_string(), self.profile))
    }
}

#[allow(unused)]
fn assert_subsystem_manifest(manifest: Value) -> JuizResult<Value> {
    Ok(manifest)
}

impl SubSystemProxy {

    pub fn new(system_uuid: Uuid, broker_proxy: Arc<Mutex<dyn BrokerProxy>>) -> JuizResult<Self> {
        let profile = broker_proxy.lock().unwrap().profile_full()?;
        Ok(SubSystemProxy{ 
            uuid: system_uuid,
            broker_proxy,
            profile
        })
    }
    
    pub fn broker_proxy(&self) -> Arc<Mutex<dyn BrokerProxy>> {
        self.broker_proxy.clone()
    }
    
    #[allow(unused)]
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        self.broker_proxy.lock().unwrap().profile_full()
    }

    pub fn subsystems(&self) -> JuizResult<Value> {
        let prof = self.profile_full()?;
        let r = prof.as_object().and_then(|prof_obj| -> Option<Vec<Value>> {
            match prof_obj.get("subsystems") {
                Some(obj) => {
                    match obj.as_array() {
                        Some(v) => Some(v.clone()),
                        None => {
                            log::error!("profile of subsystem is not array type.");
                            Some(Vec::new())
                        }
                    }
                }
                None => Some(Vec::new())
            }
        }).unwrap();
        Ok(jvalue!(r))
    }
}