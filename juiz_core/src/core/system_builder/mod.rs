mod brokers;
mod builtin_brokers;
mod cleanup_objects;
mod components;
mod composites;
mod connections;
mod containers;
mod ecs;
mod namespaces;
mod processes;
mod setup_objects;
mod setup_plugins;
mod subsystems;
mod topics;

mod http_broker;
#[cfg(feature = "ipc-broker")]
mod ipc_broker;
mod local_broker;

use crate::prelude::*;
pub(crate) use cleanup_objects::cleanup_objects;
pub(crate) use setup_objects::setup_objects;
pub(crate) use setup_plugins::setup_plugins;
use uuid::Uuid;

use crate::brokers::{broker_proxy::TopicBrokerProxy, SystemBrokerProxy};

use super::system::System;

pub(crate) fn setup_topic_synchronization(system: &mut System) -> JuizResult<()> {
    log::trace!("setup_topic_synchronization() called");
    let mut should_request_subscribe_topic_names: Vec<String> = Vec::new();
    let mut should_request_publish_topic_names: Vec<String> = Vec::new();
    system.core_broker().lock_mut().and_then(|mut cb| {
        let system_uuid = Uuid::parse_str(cb.system_uuid()?.as_str().unwrap())?;
        for (topic_name, topic) in cb.worker_mut().store_mut().topics.iter() {
            if topic.num_local_publishers().unwrap() > 0 {
                should_request_subscribe_topic_names.push(topic_name.to_owned());
            }
            if topic.num_local_subscribers().unwrap() > 0 {
                should_request_publish_topic_names.push(topic_name.to_owned());
            }
        }
        for topic_name in should_request_subscribe_topic_names.iter() {
            let _result = cb.topic_request_subscribe(topic_name, Some(system_uuid))?;
        }
        for topic_name in should_request_publish_topic_names.iter() {
            let _result = cb.topic_request_publish(topic_name, Some(system_uuid))?;
        }

        log::trace!("setup_topic_synchronization() exit");
        Ok(())
    })
}

pub(crate) use components::register_component;
pub(crate) use composites::create_composite_container_from_value;
pub(crate) use containers::{register_container_factory, register_container_process_factory};
pub(crate) use processes::register_process_factory;

pub(crate) fn setup_ec_activation(system: &mut System) -> JuizResult<()> {
    let ec_ids = system
        .core_broker()
        .lock()?
        .ec_list(false)?
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<String>>();
    for ec_id in ec_ids.iter() {
        let ec = system.core_broker().lock()?.worker().ec_from_id(&ec_id)?;
        let prof = juiz_lock(&ec)?.profile_full()?;
        let auto_start = match obj_get_bool(&prof, "auto_start") {
            Ok(v) => v,
            Err(_) => false,
        };
        if auto_start {
            juiz_lock(&ec)?.start()?;
        }
    }
    Ok(())
}
