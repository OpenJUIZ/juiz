//! juizにおける各オブジェクトのベースとなるtrait
//! 
//! 
use std::fmt::Display;

use crate::prelude::*;
use crate::identifier::identifier_new;

#[derive(Clone, Debug)]
pub enum JuizObjectClass {

    Process(&'static str),
    Container(&'static str),
    ContainerProcess(&'static str),
    ProcessFactory(&'static str),
    ContainerFactory(&'static str),
    ContainerProcessFactory(&'static str),
    Connection(&'static str),
    ExecutionContext(&'static str),
    ExecutionContextFactory(&'static str),

    Broker(&'static str),
    BrokerFactory(&'static str),
    BrokerProxy(&'static str),
    BrokerProxyFactory(&'static str),
    System(&'static str),

    ProcessProxy(&'static str),
    Topic(&'static str),
}

impl Display for JuizObjectClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("JuizObjectClass({})", self.as_str()))
    }
}

impl JuizObjectClass {

    pub fn as_str(&self) -> &'static str {
        match *self {
            JuizObjectClass::Process(_) => "process",
            JuizObjectClass::ProcessFactory(_) => "process_factory",
            JuizObjectClass::Container(_) => "container",
            JuizObjectClass::ContainerFactory(_) => "container_factory",
            JuizObjectClass::ContainerProcess(_) => "container_process",
            JuizObjectClass::ContainerProcessFactory(_) => "container_process_factory",
            JuizObjectClass::Connection(_) => "connection",
            JuizObjectClass::ExecutionContext(_) => "execution_context",
            JuizObjectClass::ExecutionContextFactory(_) => "execution_context_factory",

            JuizObjectClass::Broker(_) => "broker",
            JuizObjectClass::BrokerFactory(_) => "broker_factory",
            JuizObjectClass::BrokerProxy(_) => "broker_proxy",
            JuizObjectClass::BrokerProxyFactory(_) => "broker_proxy_factory",

            JuizObjectClass::System(_) => "system",
            JuizObjectClass::ProcessProxy(_) => "process_proxy",
            JuizObjectClass::Topic(_) => "topic", 
        }
    }
}

#[derive(Debug)]
pub struct ObjectCore {
    identifier: Identifier,
    class_name: JuizObjectClass,
    type_name: String,
    name: String,
    broker_type_name: String,
    broker_name: String,
}

impl Clone for ObjectCore {
    fn clone(&self) -> Self {
        Self { identifier: self.identifier.clone(), class_name: self.class_name.clone(), type_name: self.type_name.clone(), name: self.name.clone(), broker_type_name: self.broker_type_name.clone(), broker_name: self.broker_name.clone() }
    }
}

impl ObjectCore {
    
    pub fn new(identifier: Identifier, class_name: JuizObjectClass, type_name: &str, object_name: &str, broker_name: &str, broker_type_name: &str) -> ObjectCore{
        //let identifier = identifier_new(broker_type_name, broker_name, class_name.as_str(), type_name, object_name);
        ObjectCore { identifier, class_name, type_name: type_name.to_string(), name: object_name.to_string(), broker_type_name: broker_type_name.to_string(), broker_name: broker_name.to_string()}
    }

    pub fn create<T: ToString, D: ToString>(class_name: JuizObjectClass, type_name: T, object_name: D) -> ObjectCore{
        let identifier = identifier_new("core", "core", class_name.as_str(), type_name.to_string().as_str(), object_name.to_string().as_str());
        ObjectCore { identifier, class_name, type_name: type_name.to_string(), name: object_name.to_string(), broker_name: "core".to_string(), broker_type_name: "core".to_string()}
    }

    pub fn create_factory<T: ToString>(class_name: JuizObjectClass, type_name: T) -> ObjectCore{
        let identifier = identifier_new("core", "core", class_name.as_str(), type_name.to_string().as_str(), type_name.to_string().as_str());
        ObjectCore { identifier, class_name, type_name: type_name.to_string(), name: type_name.to_string(), broker_name: "core".to_string(), broker_type_name: "core".to_string()}
    }

    pub fn set_identifier(&mut self, id: Identifier) -> () {
        self.identifier = id
    }

    pub fn identifier(&self) -> Identifier {
        self.identifier.clone()
    }

    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "identifier": self.identifier,
            "class_name": self.class_name.as_str(),
            "type_name": self.type_name,
            "name": self.name,
            "broker_type_name": self.broker_type_name,
            "broker_name": self.broker_name,
        }))
    }
}

pub trait JuizObjectCoreHolder {
    fn core(&self) -> &ObjectCore;
}

pub trait JuizObject : JuizObjectCoreHolder {

    fn identifier(&self) -> Identifier {
        self.core().identifier()
    }

    fn profile_full(&self) -> JuizResult<Value>{
        Ok(jvalue!({
            "identifier": self.identifier(),
            "class_name": self.class_name().as_str(),
            "type_name": self.type_name(),
            "name": self.name(),
            "broker_type_name": self.broker_type(),
            "broker_name": self.broker_name(),
        }).into())
    }

    fn class_name(&self) -> &JuizObjectClass {
        &self.core().class_name
    }

    fn type_name(&self) -> &str {
        self.core().type_name.as_str()
    }

    fn name(&self) -> &str {
        self.core().name.as_str()
    }

    fn broker_type(&self) -> &str {
        self.core().broker_type_name.as_str()
    }

    fn broker_name(&self) -> &str {
        self.core().broker_name.as_str()
    }
}