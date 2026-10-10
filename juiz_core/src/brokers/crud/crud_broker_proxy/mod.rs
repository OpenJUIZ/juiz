use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use juiz_sdk::{
    anyhow::anyhow,
    connection_identifier::ConnectionIdentifier,
    connections::{ConnectionManifest, ConnectionProfile},
    manifests::ContainerIdentifier,
    manifests::{ContainerProfile, ProcessProfile},
    process_identifier::ProcessIdentifier,
    topic_identifier::TopicIdentifier,
};
use uuid::Uuid;

use crate::brokers::{
    broker_proxy::{
        BrokerBrokerProxy, ConnectionBrokerProxy, ContainerBrokerProxy,
        ContainerProcessBrokerProxy, ExecutionContextBrokerProxy, ProcessBrokerProxy,
        SystemBrokerProxy,
    },
    BrokerProxy,
};
use crate::{brokers::broker_proxy::TopicBrokerProxy, prelude::*};

pub trait CRUDBrokerProxy: Send + Sync {
    fn create(
        &self,
        class_name: &str,
        function_name: &str,
        payload: Value,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr>;
    fn delete(
        &self,
        class_name: &str,
        function_name: &str,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr>;
    fn read(
        &self,
        class_name: &str,
        function_name: &str,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr>;
    fn update(
        &self,
        class_name: &str,
        function_name: &str,
        payload: CapsuleMap,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr>;
}

pub struct CRUDBrokerProxyHolder {
    core: ObjectCore,
    broker: Box<dyn CRUDBrokerProxy>,
}

fn param(param_map: &[(&str, &str)]) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for (k, v) in param_map.iter() {
        if *k == "identifier" {
            map.insert((*k).to_owned(), modify_id(*v));
        } else {
            map.insert((*k).to_owned(), (*v).to_owned());
        }
    }
    map
}

fn topic_param(param_map: &[(&str, &str)]) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for (k, v) in param_map.iter() {
        map.insert((*k).to_owned(), (*v).to_owned());
    }
    map
}

fn modify_id(id: &str) -> String {
    let mut id_struct = IdentifierStruct::try_from(id.to_owned()).unwrap();
    id_struct.broker_name = "core".to_owned();
    id_struct.broker_type_name = "core".to_owned();
    id.into()
}

impl CRUDBrokerProxyHolder {
    pub fn new(
        impl_class_name: &'static str,
        type_name: &str,
        name: &str,
        broker_proxy: Box<dyn CRUDBrokerProxy>,
    ) -> JuizResult<Arc<Mutex<CRUDBrokerProxyHolder>>> {
        Ok(Arc::new(Mutex::new(CRUDBrokerProxyHolder {
            core: ObjectCore::create(
                JuizObjectClass::BrokerProxy(impl_class_name),
                type_name,
                name,
            ),
            broker: broker_proxy,
        })))
    }

    fn _convert_proccess_identifier_name(
        &self,
        mut id: ProcessIdentifier,
    ) -> JuizResult<ProcessIdentifier> {
        if id.broker_type_name == "core" {
            id.broker_type_name = self.type_name().to_owned();
            id.broker_name = self.name().to_owned();
        }
        Ok(id)
    }

    fn _convert_container_identifier_name(
        &self,
        mut id: ContainerIdentifier,
    ) -> JuizResult<ContainerIdentifier> {
        if id.broker_type_name == "core" {
            id.broker_type_name = self.type_name().to_owned();
            id.broker_name = self.name().to_owned();
        }
        Ok(id)
    }

    fn convert_identifier_name(&self, id: &Value) -> JuizResult<Value> {
        let id_str = id
            .as_str()
            .ok_or(anyhow!(JuizError::ValueIsNotStringError {}))?
            .to_owned();
        let mut id_struct = IdentifierStruct::try_from(id_str)?;
        if id_struct.broker_type_name == "core" {
            id_struct.broker_type_name = self.type_name().to_owned();
            id_struct.broker_name = self.name().to_owned();
        }
        Ok(id_struct.to_identifier().into())
    }

    fn _convert_identifier_names(&self, id_array: &Value) -> JuizResult<Value> {
        let mut ids: Vec<String> = Vec::new();
        for vid in get_array(id_array)?.iter() {
            let id = vid
                .as_str()
                .ok_or(anyhow!(JuizError::ValueIsNotStringError {}))?
                .to_owned();
            let mut id_struct = IdentifierStruct::try_from(id)?;
            if id_struct.broker_type_name == "core" {
                id_struct.broker_type_name = self.type_name().to_owned();
                id_struct.broker_name = self.name().to_owned();
            }
            ids.push(id_struct.into());
        }
        // log::trace!("convert_identifier_name({ids:?})");
        Ok(jvalue!(ids))
    }

    fn modify_profile(&self, capsule: CapsulePtr) -> CapsulePtr {
        let key_id = "identifier".to_owned();
        let _ = capsule.lock_modify_as_value(|v| {
            let map = get_hashmap_mut(v).unwrap();
            if map.contains_key(&key_id) {
                let id = map.get(&key_id).unwrap().as_str().unwrap().to_owned();
                let mut id_struct = IdentifierStruct::try_from(id).unwrap();
                id_struct.broker_name = self.name().to_owned();
                id_struct.broker_type_name = self.type_name().to_owned();
                let new_id: Identifier = id_struct.into();
                map.insert(key_id, jvalue!(new_id));
            }
        });
        capsule
    }
}

impl JuizObjectCoreHolder for CRUDBrokerProxyHolder {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for CRUDBrokerProxyHolder {
    // fn profile_full(&self) -> JuizResult<Value>{
    //     Ok(jvalue!({
    //         "identifier": self.identifier(),
    //         "class_name": self.class_name().as_str(),
    //         "type_name": self.type_name(),
    //         "name": self.name(),
    //         "broker_type_name": self.broker_type(),
    //         "broker_name": self.broker_name().to_owned() + "hogehogefoo",
    //     }).into())
    // }
}

impl ContainerProcessBrokerProxy for CRUDBrokerProxyHolder {
    fn container_process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        log::trace!("【呼出】container_process_profile_full({id})");
        let result = capsule_to_value(self.modify_profile(self.broker.read(
            "container_process",
            "profile_full",
            param(&[("identifier", Into::<String>::into(id.clone()).as_str())]),
        )?));
        log::trace!("CRUDBrokerProxy.container_process_profile_full({id}) = {result:?}");
        Ok(serde_json::from_value(result?)?)
    }

    fn container_process_list(
        &self,
        recursive: bool,
        _caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>> {
        log::trace!("【呼出】container_process_list({recursive})");
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), recursive.to_string());
        let v = self.broker.read("container_process", "list", param)?;
        log::debug!("CRUDBrokerProxyHolder::container_process_list() = {v:?}");
        let array_value = v
            .extract_value()?
            .as_array()
            .ok_or(JuizError::InvalidArgumentError {
                message: format!(
                    "crud_broker_proxy::container_process_list failed. Return value is not array."
                ),
            })?
            .clone();
        Ok(array_value
            .into_iter()
            .map(|v| serde_json::from_value(v))
            .collect::<serde_json::Result<Vec<ProcessIdentifier>>>()?)
    }

    fn container_process_call(
        &self,
        id: &ProcessIdentifier,
        args: CapsuleMap,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】container_process_call({id}, {args})");
        self.broker.update(
            "container_process",
            "call",
            args,
            param(&[("identifier", id.to_string().as_str())]),
        )
    }

    fn container_process_execute(&self, id: &ProcessIdentifier) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】container_process_execute({id})");
        self.broker.update(
            "container_process",
            "execute",
            CapsuleMap::new(),
            param(&[("identifier", id.to_string().as_str())]),
        )
    }

    fn container_process_create(
        &mut self,
        container_id: &ContainerIdentifier,
        manifest: &juiz_sdk::manifests::ProcessManifest,
    ) -> Result<ProcessProfile, juiz_sdk::anyhow::Error> {
        log::trace!("【呼出】container_process_create({container_id}, {manifest})");
        let value = capsule_to_value(self.broker.create(
            "container_process",
            "create",
            serde_json::to_value(manifest)?,
            param(&[("identifier", container_id.to_string().as_str())]),
        )?)?;
        Ok(serde_json::from_value(value)?)
    }

    fn container_process_destroy(
        &mut self,
        identifier: &ProcessIdentifier,
    ) -> Result<ProcessProfile, juiz_sdk::anyhow::Error> {
        log::trace!("【呼出】container_process_destroy({identifier})");
        let value = capsule_to_value(self.broker.delete(
            "container_process",
            "destroy",
            param(&[("identifier", identifier.to_string().as_str())]),
        )?)?;
        Ok(serde_json::from_value(value)?)
    }

    fn container_process_p_apply(
        &mut self,
        id: &ProcessIdentifier,
        arg_name: &str,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr> {
        let mut map = CapsuleMap::new();
        map.insert("arg_name".to_owned(), jvalue!(arg_name).into());
        map.insert("value".to_owned(), value);
        self.broker.update(
            "container_process",
            "p_apply",
            map,
            param(&[("identifier", id.to_string().as_str())]),
        )
    }

    fn container_process_openapi_spec(&self, identifier: &ProcessIdentifier) -> JuizResult<Value> {
        log::trace!("【呼出】container_process_openapi_spec({identifier})");
        capsule_to_value(
            self.broker
                .read(
                    "container_process",
                    "openapi_spec",
                    param(&[(
                        "identifier",
                        Into::<String>::into(identifier.clone()).as_str(),
                    )]),
                )
                .and_then(|v| {
                    log::trace!("【完了】container_process_openapi_spec({identifier})");
                    Ok(v)
                })
                .or_else(|e| {
                    log::error!(
                        "【エラー】container_process_openapi_spec({identifier})。エラーは ({e})"
                    );
                    Err(e)
                })?,
        )
    }
}

impl ContainerBrokerProxy for CRUDBrokerProxyHolder {
    fn container_profile_full(&self, id: &ContainerIdentifier) -> JuizResult<ContainerProfile> {
        let value = capsule_to_value(self.modify_profile(self.broker.read(
            "container",
            "profile_full",
            param(&[("identifier", id.to_string().as_str())]),
        )?))?;
        Ok(serde_json::from_value(value)?)
    }

    fn container_list(
        &self,
        recursive: bool,
        _caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ContainerIdentifier>> {
        log::trace!("CRUDBrokerProxyHolder::container_list({recursive}) called");
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), recursive.to_string());
        let vs = self
            .broker
            .read("container", "list", param)?
            .extract_value()?;
        log::debug!("CRUDBrokerProxyHolder::container_list() returns '{vs:?}'");
        Ok(vs
            .as_array()
            .ok_or(JuizError::InvalidArgumentError {
                message: format!(
                    "Invalid Result type for CRUD_broker_proxy.container_list(). Not array."
                ),
            })?
            .into_iter()
            .map(|v| serde_json::from_value(v.clone()))
            .collect::<serde_json::Result<Vec<ContainerIdentifier>>>()?)
    }

    fn container_create(
        &mut self,
        manifest: &ContainerManifest,
        mut args: CapsuleMap,
    ) -> JuizResult<ContainerProfile> {
        args.insert(
            "__manifest__".to_owned(),
            serde_json::to_value(manifest)?.into(),
        );
        let value = capsule_to_value(self.broker.create(
            "container",
            "create",
            args.into(),
            HashMap::new(),
        )?)?;
        Ok(serde_json::from_value(value)?)
    }

    fn container_destroy(
        &mut self,
        identifier: &ContainerIdentifier,
    ) -> JuizResult<ContainerProfile> {
        let value = capsule_to_value(self.broker.delete(
            "container",
            "destroy",
            param(&[("identifier", identifier.to_string().as_str())]),
        )?)?;
        Ok(serde_json::from_value(value)?)
    }
}

impl ProcessBrokerProxy for CRUDBrokerProxyHolder {
    fn process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        log::info!("CRUDBrokerProxy.process_profile_full({id}) called");
        let id_str: String = id.clone().into();
        let result_value = capsule_to_value(self.modify_profile(self.broker.read(
            "process",
            "profile_full",
            param(&[("identifier", id_str.as_str())]),
        )?))?;
        log::trace!("CRUDBrokerProxy.process_profile_full({id}) = {result_value:?}");
        Ok(serde_json::from_value(result_value)?)
    }

    fn process_call(&self, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<CapsulePtr> {
        let id_str: String = id.clone().into();
        self.broker.update(
            "process",
            "call",
            args,
            param(&[("identifier", id_str.as_str())]),
        )
    }

    fn process_execute(&self, id: &ProcessIdentifier) -> JuizResult<CapsulePtr> {
        let id_str: String = id.clone().into();
        self.broker.update(
            "process",
            "execute",
            CapsuleMap::new(),
            param(&[("identifier", id_str.as_str())]),
        )
    }

    fn process_list(
        &self,
        recursive: bool,
        _caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>> {
        log::trace!(
            "CRUDBrokerProxyHolder({})::process_list(recursive={recursive})が呼ばれました。",
            self.name()
        );
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), recursive.to_string());
        let v = self
            .broker
            .read("process", "list", param)?
            .extract_value()?;
        log::debug!("【process_list()】得られたプロセスのリストは{v:}");
        let v_array = v.as_array().ok_or(JuizError::InvalidArgumentError { message: format!("Invalid argument for crud_broker_proxy.process_list. Return value is not array.") })?.clone();
        Ok(v_array
            .into_iter()
            .map(|v| serde_json::from_value(v))
            .collect::<serde_json::Result<Vec<ProcessIdentifier>>>()?)
    }

    fn process_push_by(
        &self,
        id: &ProcessIdentifier,
        arg_name: String,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("process_push_by({id}, {arg_name}, {value}");
        let mut cm: CapsuleMap = CapsuleMap::new();
        cm.insert("value".to_owned(), value);
        let cap = CapsulePtr::from(Into::<Value>::into(arg_name));
        cm.insert("arg_name".to_owned(), cap);

        self.broker.update(
            "process",
            "push_by",
            cm,
            param(&[("identifier", id.to_string().as_str())]),
        )
    }

    fn process_try_connect_to(
        &mut self,
        connection_manifest: &ConnectionManifest,
    ) -> Result<juiz_sdk::prelude::ConnectionManifest, juiz_sdk::anyhow::Error> {
        log::trace!("process_try_connect_to({connection_manifest})が呼ばれました");
        let v: JuizResult<Value> = self
            .broker
            .update(
                "process",
                "try_connect_to",
                CapsuleMap::try_from(serde_json::to_value(connection_manifest)?)?,
                HashMap::from([]),
            )?
            .extract_value();
        log::debug!("process_try_connect_to({connection_manifest}) returns {v:?}");
        Ok(serde_json::from_value(v?)?)
    }

    fn process_notify_connected_from(
        &mut self,
        connection_manifest: &ConnectionManifest,
    ) -> Result<ConnectionProfile, juiz_sdk::anyhow::Error> {
        let v = self
            .broker
            .update(
                "process",
                "notify_connected_from",
                CapsuleMap::try_from(serde_json::to_value(connection_manifest)?)?,
                HashMap::from([]),
            )?
            .extract_value();
        Ok(serde_json::from_value(v?)?)
    }

    fn process_p_apply(
        &mut self,
        id: &ProcessIdentifier,
        arg_name: &str,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr> {
        let mut map = CapsuleMap::new();
        map.insert("arg_name".to_owned(), jvalue!(arg_name).into());
        map.insert("value".to_owned(), value);
        self.broker.update(
            "process",
            "p_apply",
            map,
            param(&[("identifier", id.to_string().as_str())]),
        )
    }

    fn process_create(
        &mut self,
        manifest: &ProcessManifest,
    ) -> Result<ProcessProfile, juiz_sdk::anyhow::Error> {
        let v = capsule_to_value(self.broker.create(
            "process",
            "create",
            serde_json::to_value(manifest)?,
            HashMap::new(),
        )?)?;
        Ok(serde_json::from_value(v)?)
    }

    fn process_destroy(
        &mut self,
        identifier: &ProcessIdentifier,
    ) -> Result<ProcessProfile, juiz_sdk::anyhow::Error> {
        let v = capsule_to_value(self.broker.delete(
            "process",
            "destroy",
            param(&[("identifier", identifier.to_string().as_str())]),
        )?)?;
        Ok(serde_json::from_value(v)?)
    }

    fn process_openapi_spec(&self, id: &ProcessIdentifier) -> JuizResult<Value> {
        log::trace!("【呼出】process_openapi_spec({id})");
        capsule_to_value(
            self.broker
                .read(
                    "process",
                    "openapi_spec",
                    param(&[("identifier", Into::<String>::into(id.clone()).as_str())]),
                )
                .and_then(|v| {
                    log::trace!("【完了】process_openapi_spec({id})");
                    Ok(v)
                })
                .or_else(|e| {
                    log::error!("【エラー】process_openapi_spec({id})。エラーは ({e})");
                    Err(e)
                })?,
        )
    }
}

impl SystemBrokerProxy for CRUDBrokerProxyHolder {
    fn system_profile_full(&self) -> JuizResult<Value> {
        capsule_to_value(self.modify_profile(self.broker.read(
            "system",
            "profile_full",
            HashMap::new(),
        )?))
    }

    fn system_filesystem_list(&self, path_buf: std::path::PathBuf) -> JuizResult<Value> {
        capsule_to_value(self.broker.read(
            "system",
            "profile_full",
            HashMap::from([("path".to_owned(), path_buf.to_str().unwrap().to_owned())]),
        )?)
    }

    fn system_add_subsystem(&mut self, profile: Value) -> JuizResult<Value> {
        let mut cp = CapsuleMap::new();
        cp.insert("profile".to_owned(), profile.into());
        capsule_to_value(
            self.broker
                .update("system", "add_subsystem", cp, HashMap::new())?,
        )
    }

    fn system_uuid(&self) -> JuizResult<Value> {
        let v = self.broker.read("system", "uuid", HashMap::new())?;
        log::trace!("system_uuid() returns {v:?}");
        return v.lock_as_str(|obj| jvalue!(obj));
    }

    fn system_add_mastersystem(&mut self, profile: Value) -> JuizResult<Value> {
        log::trace!("CRUDBroker::system_add_mastersystem(profile='{profile:}') called");
        let mut cp = CapsuleMap::new();
        cp.insert("profile".to_owned(), profile.into());
        capsule_to_value(
            self.broker
                .update("system", "add_mastersystem", cp, HashMap::new())?,
        )
    }

    fn system_load_process(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        let mut cp = CapsuleMap::new();
        cp.insert(
            "filepath".to_owned(),
            CapsulePtr::from(Value::from(filepath)),
        );
        cp.insert(
            "language".to_owned(),
            CapsulePtr::from(Value::from(language)),
        );
        capsule_to_value(
            self.broker
                .update("system", "load_process", cp, HashMap::new())?,
        )
    }

    fn system_load_container(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        let mut cp = CapsuleMap::new();
        cp.insert(
            "filepath".to_owned(),
            CapsulePtr::from(Value::from(filepath)),
        );
        cp.insert(
            "language".to_owned(),
            CapsulePtr::from(Value::from(language)),
        );
        capsule_to_value(
            self.broker
                .update("system", "load_container", cp, HashMap::new())?,
        )
    }

    fn system_load_container_process(
        &mut self,
        language: String,
        filepath: String,
    ) -> JuizResult<Value> {
        let mut cp = CapsuleMap::new();
        cp.insert(
            "filepath".to_owned(),
            CapsulePtr::from(Value::from(filepath)),
        );
        cp.insert(
            "language".to_owned(),
            CapsulePtr::from(Value::from(language)),
        );
        capsule_to_value(self.broker.update(
            "system",
            "load_container_process",
            cp,
            HashMap::new(),
        )?)
    }

    fn system_load_component(
        &mut self,
        language: String,
        filepath: String,
    ) -> Result<juiz_sdk::manifests::ComponentManifest, juiz_sdk::anyhow::Error> {
        let mut cp = CapsuleMap::new();
        cp.insert(
            "filepath".to_owned(),
            CapsulePtr::from(Value::from(filepath)),
        );
        cp.insert(
            "language".to_owned(),
            CapsulePtr::from(Value::from(language)),
        );
        let v = capsule_to_value(self.broker.update(
            "system",
            "load_component",
            cp,
            HashMap::new(),
        )?)?;
        Ok(serde_json::from_value(v)?)
    }
}

impl BrokerBrokerProxy for CRUDBrokerProxyHolder {
    fn broker_list(
        &self,
        recursive: bool,
    ) -> Result<Vec<std::string::String>, juiz_sdk::anyhow::Error> {
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), recursive.to_string());
        let v = capsule_to_value(self.broker.read("broker", "list", param)?)?;
        Ok(serde_json::from_value(v)?)
    }

    fn broker_profile_full(&self, id: &Identifier) -> JuizResult<BrokerProfile> {
        capsule_to_value(self.modify_profile(self.broker.read(
            "broker",
            "profile_full",
            param(&[("identifier", id)]),
        )?))?
        .try_into()
    }
}

impl ExecutionContextBrokerProxy for CRUDBrokerProxyHolder {
    fn ec_list(&self, recursive: bool) -> JuizResult<Value> {
        log::trace!("CRUDBrokerProxyHolder::container_list() called");
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), recursive.to_string());
        let vs = self
            .broker
            .read("execution_context", "list", param)?
            .extract_value()?;
        vs.as_array()
            .ok_or(JuizError::InvalidArgumentError {
                message: format!("CRUDBrokerProxy.ec_list() error. result must be array."),
            })?
            .into_iter()
            .map(|v| self.convert_identifier_name(v))
            .collect::<JuizResult<Value>>()
    }

    fn ec_profile_full(&self, id: &Identifier) -> JuizResult<Value> {
        capsule_to_value(self.modify_profile(self.broker.read(
            "execution_context",
            "profile_full",
            param(&[("identifier", id)]),
        )?))
    }

    fn ec_get_state(&self, id: &Identifier) -> JuizResult<Value> {
        capsule_to_value(self.broker.read(
            "execution_context",
            "get_state",
            param(&[("identifier", id)]),
        )?)
    }

    fn ec_start(&mut self, id: &Identifier) -> JuizResult<Value> {
        capsule_to_value(self.broker.update(
            "execution_context",
            "start",
            CapsuleMap::new(),
            param(&[("identifier", id)]),
        )?)
    }

    fn ec_stop(&mut self, id: &Identifier) -> JuizResult<Value> {
        capsule_to_value(self.broker.update(
            "execution_context",
            "stop",
            CapsuleMap::new(),
            param(&[("identifier", id)]),
        )?)
    }

    fn ec_create(&mut self, manifest: &Value) -> JuizResult<Value> {
        capsule_to_value(self.broker.create(
            "execution_context",
            "create",
            manifest.clone(),
            HashMap::new(),
        )?)
    }

    fn ec_destroy(&mut self, identifier: &Identifier) -> JuizResult<Value> {
        capsule_to_value(self.broker.delete(
            "execution_context",
            "destroy",
            param(&[("identifier", identifier)]),
        )?)
    }
}

impl TopicBrokerProxy for CRUDBrokerProxyHolder {
    fn topic_list(&self) -> Result<Vec<TopicIdentifier>, juiz_sdk::anyhow::Error> {
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), true.to_string());
        let vs = capsule_to_value(self.broker.read("topic", "list", param)?)?;
        Ok(vs
            .as_array()
            .ok_or(JuizError::InvalidArgumentError {
                message: format!(
                    "CRudBrokerProxyHolder::topic_list() failed. Value must be array."
                ),
            })?
            .into_iter()
            .map(|v| serde_json::from_value(v.clone()))
            .collect::<serde_json::Result<Vec<TopicIdentifier>>>()?)
    }

    fn topic_push(
        &self,
        name: &str,
        capsule: CapsulePtr,
        pushed_system_uuid: Option<Uuid>,
    ) -> JuizResult<()> {
        log::trace!("topic_push({name}) called");
        let mut args = CapsuleMap::new();
        args.insert("input".to_owned(), capsule);
        let param_var = if let Some(system_uuid) = pushed_system_uuid {
            topic_param(&[
                ("topic_name", name),
                ("system_uuid", system_uuid.to_string().as_str()),
            ])
        } else {
            topic_param(&[("topic_name", name)])
        };
        //let uuid_str = if let Some(uuid) = pushed_system_uuid { uuid.to_string() } else { "".to_owned() };
        self.broker
            .update("topic", "push", args, param_var)
            .and_then(|_| Ok(()))
    }

    fn topic_request_subscribe(
        &mut self,
        name: &str,
        opt_system_uuid: Option<Uuid>,
    ) -> JuizResult<Value> {
        let param_var = if let Some(system_uuid) = opt_system_uuid {
            topic_param(&[
                ("topic_name", name),
                ("system_uuid", system_uuid.to_string().as_str()),
            ])
        } else {
            topic_param(&[("topic_name", name)])
        };
        self.broker
            .update("topic", "request_subscribe", CapsuleMap::new(), param_var)
            .and_then(|cp| Ok(cp.lock_as_value(|v| v.clone())?))
    }

    fn topic_request_publish(
        &mut self,
        name: &str,
        opt_system_uuid: Option<Uuid>,
    ) -> JuizResult<Value> {
        let param_var = if let Some(system_uuid) = opt_system_uuid {
            topic_param(&[
                ("topic_name", name),
                ("system_uuid", system_uuid.to_string().as_str()),
            ])
        } else {
            topic_param(&[("topic_name", name)])
        };
        self.broker
            .update("topic", "request_publish", CapsuleMap::new(), param_var)
            .and_then(|cp| Ok(cp.lock_as_value(|v| v.clone())?))
    }
}

impl ConnectionBrokerProxy for CRUDBrokerProxyHolder {
    fn connection_list(&self, recursive: bool) -> JuizResult<Vec<ConnectionIdentifier>> {
        log::trace!("connection_list(recursive={recursive}) called");
        let mut param: HashMap<String, String> = HashMap::new();
        param.insert("recursive".to_owned(), recursive.to_string());
        let value = self
            .broker
            .read("connection", "list", param)?
            .extract_value()?;
        Ok(value
            .as_array()
            .ok_or(JuizError::InvalidArgumentError {
                message: format!(
                    "Invalid argument for CRUDBrokerHolder::connnection_list(). not array"
                ),
            })?
            .into_iter()
            .map(|v| serde_json::from_value(v.clone()))
            .collect::<serde_json::Result<Vec<ConnectionIdentifier>>>()?)

        // let id_vec = get_array(&connection_list_value)?.into_iter().map(|v| {
        //     let id_str = v.as_str().ok_or(JuizError::ValueIsNotStringError {  })?.to_owned();
        //     let (src_id, dst_id, arg_name) = connection_identifier_split(id_str)?;
        //     let mut src_id_struct = IdentifierStruct::try_from(src_id)?;
        //     let mut dst_id_struct = IdentifierStruct::try_from(dst_id)?;
        //     //log::warn!("CONNECTIN: {src_id_struct:?}, {dst_id_struct:?}");
        //     if src_id_struct.broker_type_name == "core" {
        //         src_id_struct.broker_name = self.broker_name().to_owned();
        //         src_id_struct.broker_type_name = self.broker_type().to_owned();
        //         //log::warn!(" SRC: {dst_id_struct:?}");
        //     }
        //     if dst_id_struct.broker_type_name == "core" {

        //         dst_id_struct.broker_name = self.name().to_owned();
        //         dst_id_struct.broker_type_name = self.type_name().to_owned();
        //         //log::warn!(" SELF: {self:?}");
        //         log::warn!(" DST : {dst_id_struct:?}");
        //     }
        //     //let connection_id = connection_identifier_new(&src_id_struct.to_identifier(), &dst_id_struct.to_identifier(), arg_name.as_str());
        //     Ok(ConnectionIdentifier::new(src_id_struct.to_identifier(), arg_name.as_str(), dst_id_struct.to_identifier()))
        //     //Ok(connection_id)
        // }).collect::<JuizResult<Vec<ConnectionIdentifier>>>()?;
        // Ok(id_vec.into())
        // Ok(connection_list_value)
    }

    fn connection_profile_full(&self, id: &ConnectionIdentifier) -> JuizResult<ConnectionProfile> {
        capsule_to_value(self.broker.read(
            "connection",
            "profile_full",
            param(&[("identifier", id.to_string().as_str())]),
        )?)?
        .try_into()
    }

    fn connection_create(
        &mut self,
        manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile> {
        capsule_to_value(self.broker.create(
            "connection",
            "create",
            manifest.clone().into(),
            HashMap::new(),
        )?)?
        .try_into()
    }

    fn connection_destroy(&mut self, id: &ConnectionIdentifier) -> JuizResult<ConnectionProfile> {
        capsule_to_value(self.broker.delete(
            "connection",
            "destroy",
            param(&[("identifier", id.to_string().as_str())]),
        )?)?
        .try_into()
    }
}

impl BrokerProxy for CRUDBrokerProxyHolder {
    fn is_in_charge_for_process(&self, _id: &Identifier) -> JuizResult<bool> {
        todo!()
    }
}
