use juiz_sdk::{anyhow::anyhow, log, serde_json, topic_identifier::TopicIdentifier, prelude::*, value::{CapsulePtr, Value}};
use uuid::Uuid;

use crate::brokers::broker_proxy::TopicBrokerProxy;

use super::CallbackContainerType;



pub(crate) fn create_read_calbacks() -> CallbackContainerType {
    let mut topic_cbs = CallbackContainerType::new();
    topic_cbs.insert("list", |_crud, cb, _args| {
        log::debug!("[READ  ] topic/list called");
        let topics: Vec<TopicIdentifier> = cb.lock()?.topic_list()?;
        Ok(topics.into_iter().map(|id| { serde_json::to_value(id) }).collect::<serde_json::Result<Value>>()?.into())
    });
    topic_cbs
}
    
pub(crate) fn create_update_calbacks() -> CallbackContainerType {
    let mut topic_cbs = CallbackContainerType::new();
    topic_cbs.insert("push", |_crud,cb, args| {
    log::debug!("[UPDATE] topic/push called");
    let topic_name = args.get_param("topic_name").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "topic_name".to_owned() })})?;
        let system_uuid = match args.get_param("system_uuid") {
            Some(system_uuid_str) => {
                Some(Uuid::parse_str(system_uuid_str)?)
            }
            None => {
                log::warn!("CRUDBroker Callback (topic_push) can not detect 'system_uuid' parameter.");
                None
            }
        };
        let input = args.get("input")?;
        cb.lock()?.topic_push(topic_name.as_str(), input, system_uuid).and(Ok(CapsulePtr::new()))
    });
    topic_cbs.insert("request_subscribe", |_crud,cb, args| {
        log::debug!("[UPDATE] topic/request_subscribe called");
        let topic_name = args.get_param("topic_name").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "topic_name".to_owned() })})?;
        let system_uuid = match args.get_param("system_uuid") {
            Some(system_uuid_str) => {
                Some(Uuid::parse_str(system_uuid_str)?)
            }
            None => {
                log::warn!("CRUDBroker Callback (topic_request) can not detect 'system_uuid' parameter.");
                None
            }
        };
        cb.lock_mut()?.topic_request_subscribe(topic_name.as_str(), system_uuid).and_then(|v| { Ok(v.into())} )
    });
    topic_cbs.insert("request_publish", |_crud,cb, args| {
        log::debug!("[UPDATE] topic/request_publish called");
        let topic_name = args.get_param("topic_name").ok_or_else(||{anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError { key_name: "topic_name".to_owned() })})?;
        let system_uuid = match args.get_param("system_uuid") {
            Some(system_uuid_str) => {
                Some(Uuid::parse_str(system_uuid_str)?)
            }
            None => {
                log::warn!("CRUDBroker Callback (topic_request) can not detect 'system_uuid' parameter.");
                None
            }
        };
        cb.lock_mut()?.topic_request_publish(topic_name.as_str(), system_uuid).and_then(|v| { Ok(v.into())} )
    });
    topic_cbs
}
