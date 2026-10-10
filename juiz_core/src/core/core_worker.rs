
use std::{collections::HashMap, env::current_dir, path::PathBuf, sync::{Arc, Mutex}};

use juiz_sdk::{anyhow::Context, connection_identifier::ConnectionIdentifier, connections::{ConnectionManifest, ConnectionProfile}, manifests::ContainerIdentifier, manifests::{ContainerProfile, ProcessProfile}, process_identifier::ProcessIdentifier, utils::manifest_util::{construct_id, type_name}};
use uuid::Uuid;

use crate::{connections::connection_builder::connection_builder, containers::{ContainerProcessImpl, ContainerProxy}, core::system_builder::register_component, ecs::{execution_context_function::ExecutionContextFunction, execution_context_proxy::ExecutionContextProxy}, plugin::JuizObjectPlugin, prelude::*, topics::TopicPtr};

use super::{core_store::CoreStore, system_builder::{register_container_factory, register_container_process_factory, register_process_factory}};
use juiz_sdk::anyhow::anyhow;


// #[derive(Debug)]
pub struct CoreWorker {
    store: CoreStore,
    system_uuid: Uuid,
}

impl CoreWorker {
    pub fn new(uuid: Uuid, manifest: Value) -> Self {
        CoreWorker{store: CoreStore::new(manifest), system_uuid: uuid}
    }

    pub fn manifest(&self) -> Value {
        self.store.manifest()
    }

    pub fn manifest_mut(&mut self) -> &mut Value {
        self.store.manifest_mut()
    }

    

    pub fn store(&self) -> &CoreStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut CoreStore {
        &mut self.store
    }

    pub fn get_opt_mut(&mut self) -> &mut Value {
        self.store_mut().get_opt_mut()
    }

    pub fn reserve_master_broker(&mut self, master_info: Value) -> JuizResult<()> {
        log::trace!("reserve_master_broker({master_info:}) called");
        match self.store_mut().manifest_mut().as_object_mut() {
            Some(manif) => {
                if manif.contains_key("mastersystem") {
                    log::warn!("manifest already contains master system information. Reservation is skipped.");
                } else {
                    log::debug!("Master system ({master_info:}) is reserved.");
                    manif.insert("mastersystem".to_owned(), master_info);
                }
            }
            None => {
                log::error!("reserve_master_broker() failed. Can not get manifest map value.");
                panic!()
            }
        }
        Ok(())
    }

    pub fn process_from_identifier(&self, identifier: &ProcessIdentifier, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
        // println!("process_from_identifier({:?})", identifier.to_string());
        if identifier.broker_type_name == "core" {
            let p = Ok(self.store().processes.get(&identifier.to_string())?.clone());
            // println!("process_from_identifier() -> {p:?}");
            return p;
        }
        self.process_proxy_from_identifier(identifier, create_when_not_found)
    }

    pub fn process_from_typename_and_name(&self, type_name: &str, name: &str) -> JuizResult<ProcessPtr> {
        Ok(self.store().processes.get(&construct_id("process", type_name, name, "core", "core"))?.clone())
    }

    pub fn process_proxy_from_identifier(&self, identifier: &ProcessIdentifier, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
        log::trace!("process_proxy_from_identifier({identifier}) called");
        if identifier.broker_name == "core" && identifier.broker_type_name == "core" {
            return self.process_from_identifier(identifier, create_when_not_found)
        }
        let broker_proxy = self.broker_proxy(&identifier.broker_type_name, &identifier.broker_name, create_when_not_found)?;
        Ok(ProcessProxy::new(JuizObjectClass::Process("process_proxy"), identifier.clone(), broker_proxy)?)
    }

    // pub fn process_proxy_from_manifest(&mut self, manifest: ProcessManifest, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
    //     //self.process_proxy_from_identifier(&id_from_manifest_and_class_name(manifest, "Process")?, create_when_not_found)
    //     self.process_proxy_from_identifier(&manifest.identifier()?, create_when_not_found)
    // }

    /// BrokerProxyを作成、もしくはキャッシュから読み出す
    /// 
    /// # Arguments
    /// * `broker_type_name` - 
    /// * `broker_name` - 
    /// * `create_when_not_found ` - 
    /// 
    pub fn broker_proxy(&self, broker_type_name: &str, broker_name: &str, create_when_not_found: bool) ->JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        log::trace!("broker_proxy({broker_type_name}, {broker_name}, {create_when_not_found})が呼ばれました。");
        let mut type_name = broker_type_name;
        if type_name == "core" { type_name = "local"; }

        let identifier = "core://core/BrokerProxy/".to_string() + broker_name + "::" + broker_type_name;
        log::debug!("【broker_proxy】broker_proxy({identifier})を探しています。");
        match self.store().broker_proxies.get_with_error_ignore(&identifier) {
            Ok(bp) => return Ok(bp),
            Err(_) => {}
        };
        
        if !create_when_not_found {
            log::error!("broker_proxy({broker_type_name}, {broker_name}) can not find broker_proxy.");
            return Err(anyhow!(JuizError::ObjectCanNotFoundByIdError { id: identifier }));
        }

        log::debug!("【broker_proxy】BrokerProxy(broker_type_name={broker_type_name}, broker_name={broker_name})が見つかりません。BrokerProxyを作成します。");
        let manifest = jvalue!({
            "type_name": type_name,
            "name": broker_name
        });
        let bf = self.store().broker_proxies.factory(type_name).or_else(|e| {
            log::error!("creating BrokerProxyFactory(type_name={type_name}) failed. Error ({e})");
            Err(e)
        })?;
        let bp = juiz_lock(&bf)?.create_broker_proxy(self, manifest).or_else(|e| {
            log::error!("creating BrokerProxy(type_name={type_name}) failed. Error ({e})");
            Err(e)
        })?;
        log::debug!("【broker_proxy】BrokerProxy(broker_type_name={broker_type_name}, broker_name={broker_name})の作成に成功しました。");
        self.store().broker_proxies.register(bp.clone())?;
        Ok(bp)
    }

    pub fn broker_proxy_from_manifest(&mut self, manifest: &Value, create_when_not_found: bool) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        let mut type_name = obj_get_str(manifest, "type_name")?;
        if type_name == "core" {
            type_name = "local";
        }

        let name = match obj_get_str(manifest, "name") {
            Ok(name) => name.to_string(),
            Err(_) => {
                let counter = 0;
                type_name.to_string() + counter.to_string().as_str()
            }
        };
        self.broker_proxy(type_name, name.as_str(), create_when_not_found)
    }

    pub fn create_process_ref(&mut self, manifest: &ProcessManifest) -> JuizResult<ProcessPtr> {
        log::trace!("【呼出】create_process_ref(manifest={:}) called", manifest);
        let arc_pf = match self.store().processes.factory(manifest.type_name.as_str())
            .with_context(||{format!("store.container_processes.factory({}) in create_container_process_ref() in core_worker.rs", manifest.type_name.as_str())}) {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("process_factory({}) not found", manifest.type_name);
                Err(e)
            }
        }?;
        let p = arc_pf.lock()?.create_process(manifest)?;
        Ok(self.store_mut().processes.register(&p.identifier().to_string(), p)?.clone())
    }

    pub fn destroy_process_ref(&mut self, identifier: &ProcessIdentifier) -> JuizResult<ProcessPtr> {
        log::trace!("CoreBroker::destroy_process(identifier={}) called", identifier);
        self.store_mut().processes.deregister_by_id(&identifier.to_string())
    }

    pub fn create_container_ref(&mut self, type_name: &str, name: &str, mut args: CapsuleMap) -> JuizResult<ContainerPtr> {
        log::trace!("CoreBroker::create_container(manifest={:?}) called", args);
        let arc_pf = match self.store().containers.factory(type_name)
            .with_context(||{format!("store.container_processes.factory({}) in create_container_process_ref() in core_worker.rs", type_name)}) {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("container_factory({}) not found", type_name);
                Err(e)
            }
        }?.clone();
        let factory_wrapper_manifest = arc_pf.lock()?.profile_full()?;
        match obj_get(&factory_wrapper_manifest, "container_factory") {
            Ok(cont_manif) => {
                match obj_get_array(cont_manif, "arguments") {
                    Ok(arg_array) => {
                        for arg_value in arg_array.iter() {
                            match obj_get_str(arg_value, "name") {
                                Ok(name_str) => {
                                    match obj_get(arg_value, "default") {
                                        Ok(default_value) => {
                                            match args.get(name_str) {
                                                Ok(_) => {}
                                                Err(_) => {
                                                    let mut cp = CapsulePtr::new();
                                                    cp.replace_with_value(default_value.clone());
                                                    args.insert(name_str.to_owned(), cp);
                                                }
                                            }
                                        }
                                        Err(_) => {}
                                    }
                                }
                                Err(_) => {}
                            }
                        }
                    },
                    Err(_) => {}
                }
            }
            Err(_) => {}
        }
        let p = arc_pf.lock()?.create_container(self, name.to_owned(), args)?;
        let id = p.identifier().clone();
        Ok(self.store_mut().containers.register(&id.to_string(), p)?.clone())
    }

    pub fn destroy_container_ref(&mut self, identifier: &ContainerIdentifier) -> JuizResult<ContainerProfile> {
        log::trace!("CoreBroker::destroy_container_ref(identifier={}) called", identifier);
        let cont = self.store().containers.get(&identifier.to_string())?.clone();
        let ids = cont.lock_mut()?.processes().iter().map(|cp|{
            cp.identifier()
        }).collect::<Vec<ProcessIdentifier>>();
        for pid in ids.iter() {
            self.destroy_container_process_ref(pid)?;
            //container_lock_mut(&mut cont.clone())?.purge_process(pid)?;
        }
        self.store_mut().containers.deregister_by_id(&identifier.to_string())?;
        let f = self.store().containers.factory(cont.identifier().type_name.as_str())?;
        log::trace!("container_destroy({}) exit", identifier);
        f.lock_mut()?.destroy_container(cont)
    }

    pub fn create_container_process_ref(&mut self, container: ContainerPtr, manifest: &ProcessManifest) -> JuizResult<ProcessPtr> {
        log::trace!("CoreBroker::create_container_process_ref(manifest={:?}) called", manifest);
        //let typ_name = type_name(&manifest)?;
        let arc_pf = match self.store().container_processes.factory(manifest.type_name.as_str())
            .with_context(||{format!("store.container_processes.factory({}) in create_container_process_ref() in core_worker.rs", manifest.type_name.as_str())}) {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("container_process_factory({}) not found", manifest.type_name);
                log::debug!("registered container_process_factories are {:?}", self.store().container_processes.factories());
                Err(e)
            }
        }?;
        let p = arc_pf.lock()?.create_container_process(container.clone(), manifest)?;
        container.lock_mut()?.register_process(p.clone())?;
        let id = p.identifier().clone();
        log::debug!("id: {:?}, {:?}", id, id.to_string());
        Ok(self.store_mut().container_processes.register(&id.to_string(), p)?.clone())
    }

    pub fn destroy_container_process_ref(&mut self, identifier: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        log::trace!("CoreBroker::destroy_container_process_ref(identifier={}) called", identifier);
        let process = self.store_mut().container_processes.deregister_by_id(&identifier.to_string())?;
        let c = process.downcast_and_then(|cp: &ContainerProcessImpl| {
            self.store().containers.get(&cp.identifier().to_string())
        })??;
        //let c = self.store().containers.get(con_id)?;
        c.lock_mut()?.purge_process(&identifier.to_string())?;
        process.lock_mut()?.purge()?;
        let f = self.store().container_processes.factory(process.identifier().type_name.as_str())?;
        let v = f.lock_mut()?.destroy_container_process(process);
        log::trace!("destroy_container_process_ref({}) exit", identifier);
        v
    }

    pub fn container_from_identifier(&mut self, id: &ContainerIdentifier) -> JuizResult<ContainerPtr> {
        if id.broker_type_name == "core" {
            return Ok(self.store().containers.get(&id.to_string())?.clone())
        }
        self.container_proxy_from_identifier(id)
       
    }

    pub fn container_from_typename_and_name(&self, type_name: &str, name: &str) -> JuizResult<ContainerPtr> {
        Ok(self.store().containers.get(&construct_id("Container", type_name, name, "core", "core"))?.clone())
    }

    // pub fn container_from_manifest(&mut self, manifest: &Value) -> JuizResult<ContainerPtr> {
    //     self.container_from_identifier(&id_from_manifest_and_class_name(manifest, "Container")?)
    // }

    pub fn container_process_from_id(&self, id: &ProcessIdentifier) -> JuizResult<ProcessPtr> {
        Ok(self.store().container_processes.get(&id.to_string())?.clone())
    }

    pub fn container_process_from_typename_and_name(&self, type_name: &str, name: &str) -> JuizResult<ProcessPtr> {
        Ok(self.store().container_processes.get(&construct_id("ContainerProcess", type_name, name, "core", "core"))?.clone())
    }

    pub fn container_proxy_from_identifier(&mut self, identifier: &ContainerIdentifier) -> JuizResult<ContainerPtr> {
        log::info!("CoreBroker::container_proxy_from_identifier({identifier}) called");
        //let id_struct = IdentifierStruct::try_from(identifier.clone())?;
        if identifier.broker_name == "core" && identifier.broker_type_name == "core" {
            return self.container_from_identifier(identifier)
        }
        let broker_proxy = self.broker_proxy(&identifier.broker_type_name, &identifier.broker_name, false)?;
        Ok(ContainerPtr::new(ContainerProxy::new(JuizObjectClass::Container("ContainerProxy"), identifier.clone(), broker_proxy)?))
    }
    
    pub fn container_processes_by_container(&self, _container: ContainerPtr) -> JuizResult<Vec<ProcessPtr>> {
        for _p in self.store().container_processes.objects().into_iter() {
            //let c = (p as Arc<RwLock<ContainerProcessImpl>>).container;
        }
        todo!();
        //self.container_process_from_id(&id_from_manifest_and_class_name(manifest, "ContainerProcess")?)
    }

    // pub fn container_process_from_manifest(&self, manifest: &Value) -> JuizResult<ProcessPtr> {
    //     self.container_process_from_id(&id_from_manifest_and_class_name(manifest, "ContainerProcess")?)
    // }

    pub fn any_process_from_identifier(&self, id: &ProcessIdentifier, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
        self.process_from_identifier(id, create_when_not_found).or_else(|_| { self.container_process_from_id(id) })
    }

    pub fn any_process_from_typename_and_name(&self, type_name: &str, name: &str) -> JuizResult<ProcessPtr> {
        self.process_from_typename_and_name(type_name, name).or_else(|_| {self.container_process_from_typename_and_name(type_name, name)})
    }

    // pub fn any_process_from_manifest(&self, manifest: &Value, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
    //     match id_from_manifest(manifest) {
    //         Ok(id) => {
    //             return self.any_process_from_identifier(&id, create_when_not_found);
    //         },
    //         Err(_) => {
    //             let type_name = obj_get_str(manifest, "type_name")?;
    //             let name = obj_get_str(manifest, "name")?;
    //             self.any_process_from_typename_and_name(type_name, name)
    //         }
    //     }
    // }



    pub fn container_process_proxy_from_identifier(&mut self, identifier: &ProcessIdentifier) -> JuizResult<ProcessPtr> {
        // let id_struct = IdentifierStruct::try_from(identifier.clone())?;
        if identifier.broker_name == "core" && identifier.broker_type_name == "core" {
            return self.container_process_from_id(identifier)
        }
        let broker_proxy = self.broker_proxy(&identifier.broker_type_name, &identifier.broker_name, false)?;
        Ok(ProcessProxy::new(JuizObjectClass::ContainerProcess("ProcessProxy"), identifier.clone(), broker_proxy)?)
    }

    // pub fn container_process_proxy_from_manifest(&mut self, manifest: &Value) -> JuizResult<ProcessPtr> {
    //     self.container_process_proxy_from_identifier(&id_from_manifest_and_class_name(manifest, "ContainerProcess")?)
    // }


    pub fn any_process_proxy_from_identifier(&mut self, identifier_ref: &ProcessIdentifier, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
        log::trace!("CoreBroker::any_process_proxy_from_identifier({identifier_ref}) called");
        let mut identifier = identifier_ref.clone();
        identifier.class_name = "process".to_owned();
        let p = self.process_proxy_from_identifier(&identifier, create_when_not_found);
        if p.is_ok() {
            // println!("Found: {identifier:?}");
            return p;
        } 
        //println!("Not Found: {identifier:?} / {p:?}");
        identifier.class_name = "container_process".to_owned();
        self.container_process_proxy_from_identifier(&identifier)
    }

    // pub fn any_process_proxy_from_manifest(&mut self, manifest: &Value, create_when_not_found: bool) -> JuizResult<ProcessPtr> {
    //     let identifier = identifier_from_manifest("core", "core", "Process", manifest)?;
    //     self.any_process_proxy_from_identifier(&identifier, create_when_not_found)
    // }

    

    pub fn create_ec_ref(&mut self, manifest: Value) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
        log::trace!("CoreBroker::create_ec_ref(manifest={}) called", manifest);
        let arc_pf = self.store().ecs.factory(type_name(&manifest)?).or_else(|e| {
            log::error!("create_ec_ref({manifest:}) failed. Searching factory failed. Error({e:})");
            Err(e)
        })?;
        let p = juiz_lock(arc_pf)?.create(precreate_check(manifest.clone())?).or_else(|e| {
            log::error!("create_ec_ref({:}) failed. Error({e})", manifest.clone());
            Err(e)
        })?;

        self.store_mut().ecs.register(p)
    }

    pub fn ec_from_id(&self, id: &Identifier) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
        self.store().ecs.get(id)
    }

    pub fn ec_proxy_from_identifier(&mut self, identifier: &Identifier) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
        log::info!("CoreBroker::ec_proxy_from_identifier({identifier}) called");
        let id_struct = IdentifierStruct::try_from(identifier.clone())?;
        if id_struct.broker_name == "core" && id_struct.broker_type_name == "core" {
            return self.ec_from_id(identifier)
        }
        let broker_proxy = self.broker_proxy(&id_struct.broker_type_name, &id_struct.broker_name, false)?;
        Ok(ExecutionContextProxy::new(JuizObjectClass::ExecutionContext("ExecutionContextProxy"),identifier, broker_proxy)?)
    }
    
    pub fn cleanup_ecs(&mut self) -> JuizResult<()> {
        for ec in self.store_mut().ecs.objects().values() {
            juiz_lock(&ec)?.stop()?;
        }
        self.store_mut().ecs.cleanup_objects()
    }


    pub fn create_topic(&mut self, topic_name_str: String) -> JuizResult<TopicPtr> {
        return Ok(if self.store().topics.contains_key(&topic_name_str) {
            // Topicがすでにある時
            self.store().topics.get(&topic_name_str).unwrap().clone()
        } else {
            // Topicがない時
            self.do_create_topic(topic_name_str)?
        })
    }

    fn do_create_topic(&mut self, topic_name: String) -> JuizResult<TopicPtr> {
        log::error!("do_create_topic({topic_name}) called");
        let uuid = self.system_uuid.clone();
        self.store_mut().topics.insert(topic_name.clone(), TopicPtr::new(topic_name.as_str(), uuid));
        Ok(self.store().topics.get(&topic_name).unwrap().clone())
    }


    pub fn process_publish_topic(&mut self, process: ProcessPtr, topic_info: TopicManifest) -> JuizResult<()> {
        log::error!("process_publish_topic({topic_info:?}) called");
        let topic_name = topic_info.name.as_str();
        let topic = self.create_topic(topic_name.to_owned())?;
        self.connect_to_topic(process, topic)?;
        Ok(())
    }

    pub fn process_subscribe_topic(&mut self, process: ProcessPtr, arg_name: &String, topic_info: TopicManifest) -> JuizResult<()> {
        log::error!("process_subscribe_topic({arg_name}, {topic_info:?}) called");
        let topic_name = topic_info.name.as_str();
        let topic = self.create_topic(topic_name.to_owned())?;
        //let p = self.process_from_id(&id)?.clone();
        self.connect_from_topic(process, arg_name, topic)?;
        Ok(())
    }

    fn connect_to_topic(&mut self, process: ProcessPtr, topic: TopicPtr) -> JuizResult<()> {
        log::error!("connect_to_topic");
        // let topic_publish_connection_manifest = jvalue!({
        //     "type": "push",
        // });
        let topic_publish_connection_manifest = ConnectionManifest::new(
            ConnectionType::Push,
            process.identifier(),
            "input".to_owned(),
            topic.process_ptr().identifier()
        );
        let _connection_profile = connection_builder::connect(process, topic.process_ptr(), &topic_publish_connection_manifest)?;
        Ok(())
    }

    fn connect_from_topic(&mut self, process: ProcessPtr, _arg_name: &String, topic: TopicPtr) -> JuizResult<()> {
        log::error!("connect_from_topic");
        // let topic_subscribe_connection_manifest = jvalue!({
        //     "type": "push",
        // });
        let topic_subscribe_connection_manifest = ConnectionManifest::new(
            ConnectionType::Push,
            topic.process_ptr().identifier(),
            "input".to_owned(),
            process.identifier()
        );
        let _connection_profile = connection_builder::connect(topic.process_ptr(), process, &topic_subscribe_connection_manifest)?;
        Ok(())
    }

    pub fn load_process_factory(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        log::trace!("【呼出】load_process_factory({language}, {filepath})");
        let plugin = match language.as_str() {
            "rust" => JuizObjectPlugin::new_rust(PathBuf::from(filepath))?,
            "python" => JuizObjectPlugin::new_python(PathBuf::from(filepath))?,
            "cpp" => JuizObjectPlugin::new_cpp(PathBuf::from(filepath), "manifest")?,
            _ => {
                panic!("invalid langauge {language}")
            }
        };
        let proc_factory_ptr = register_process_factory(self, current_dir().map_or_else(|_|{None}, |wd|{Some(wd)}), plugin, "process_factory", None)?;
        let p = proc_factory_ptr.lock()?.profile_full()?;
        Ok(p)
    }

    pub fn load_container_factory(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        log::trace!("【呼出】load_container_factory({language}, {filepath})");
        let plugin = match language.as_str() {
            "rust" => JuizObjectPlugin::new_rust(PathBuf::from(filepath))?,
            "python" => JuizObjectPlugin::new_python(PathBuf::from(filepath))?,
            "cpp" => JuizObjectPlugin::new_cpp(PathBuf::from(filepath), "manifest")?,
            _ => {
                panic!("invalid langauge {language}")
            }
        };
        // let plugin = JuizObjectPlugin::new_rust(PathBuf::from(filepath))?;
        let cont_factory_ptr = register_container_factory(self, current_dir().map_or_else(|_|{None}, |wd|{Some(wd)}), plugin, "container_factory", None)?;
        let p = cont_factory_ptr.lock()?.profile_full()?;
        Ok(p)
    }


    pub fn load_container_process_factory(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        log::trace!("【呼出】load_container_process_factory({language}, {filepath})");
        let plugin = match language.as_str() {
            "rust" => JuizObjectPlugin::new_rust(PathBuf::from(filepath))?,
            "python" => JuizObjectPlugin::new_python(PathBuf::from(filepath))?,
            "cpp" => JuizObjectPlugin::new_cpp(PathBuf::from(filepath), "manifest")?,
            _ => {
                panic!("invalid langauge {language}")
            }
        };
        //let plugin = JuizObjectPlugin::new_rust(PathBuf::from(filepath))?;
        let contproc_factory_ptr = register_container_process_factory(self, current_dir().map_or_else(|_|{None}, |wd|{Some(wd)}), plugin, "container_process_factory", None)?;
        let p = contproc_factory_ptr.lock()?.profile_full()?;
        Ok(p)
    }

    pub fn load_component(&mut self, language: String, filepath: String) -> JuizResult<ComponentManifest> {
        log::trace!("【呼出】load_component({language}, {filepath})");
        let plugin = match language.as_str() {
            "rust" => JuizObjectPlugin::new_rust(PathBuf::from(filepath))?,
            "python" => JuizObjectPlugin::new_python(PathBuf::from(filepath))?,
            "cpp" => JuizObjectPlugin::new_cpp(PathBuf::from(filepath), "component_manifest")?,
            _ => {
                panic!("invalid langauge {language}")
            }
        };
        register_component(self, current_dir().map_or_else(|_|{None}, |wd|{Some(wd)}), plugin)
    }

    pub fn create_connection(&mut self, connection_manifest: &ConnectionManifest) -> JuizResult<ConnectionProfile> {
        log::trace!("【呼出】create_connection({connection_manifest})");
        let source = self.any_process_proxy_from_identifier(&connection_manifest.source_process_id, true).with_context(||{format!("search or create source process({})", &connection_manifest.source_process_id)})?;
        let destination = self.any_process_proxy_from_identifier(&connection_manifest.destination_process_id, true).with_context(||{format!("search or create destination process({})", &connection_manifest.destination_process_id)})?;
        Ok(connection_builder::connect(source, destination, &connection_manifest)?)
    }

    pub fn destroy_connection(&mut self, _connection_identifier: &ConnectionIdentifier) -> JuizResult<ConnectionProfile> {
        todo!()
    }

    pub fn connection_profile(&self, connection_identifier: &ConnectionIdentifier, create_when_not_found: bool) -> JuizResult<ConnectionProfile> {
        //let source_id = &connection_identifier.source_identifier;
        //let destination_id = &connection_identifier.destination_identifier;
        let dst_proc = self.any_process_from_identifier(&connection_identifier.destination_identifier, create_when_not_found)?;
        for con in dst_proc.lock()?.source_connections()?.into_iter() {
            if con.identifier() == *connection_identifier {
                return Ok(con.profile())
            }
        }
        let src_proc = self.any_process_from_identifier(&connection_identifier.source_identifier, create_when_not_found)?;
        for con in src_proc.lock()?.destination_connections()?.into_iter() {
            if con.identifier() == *connection_identifier {
                return Ok(con.profile())
            }
        }
        Err(anyhow!(JuizError::ConnectionCanNotBeFoundError{identifier: connection_identifier.to_string()}))
    }

    pub fn connection_profile_list(&self) -> JuizResult<Vec<ConnectionProfile>> {
        log::trace!("connection_profile_list() called");
        let mut value_map: HashMap<String, ConnectionProfile> = HashMap::new();
        for (_k, p) in self.store().processes.objects().into_iter() {
            for sc in p.lock()?.source_connections()? {
                value_map.insert(sc.identifier().into(), sc.profile());
            }
            for dc in p.lock()?.destination_connections()? {
                value_map.insert(dc.identifier().into(), dc.profile());
            }
        }
        for (_k, p) in self.store().container_processes.objects().into_iter() {
            for sc in p.lock()?.source_connections()? {
                value_map.insert(sc.identifier().into(), sc.profile());
            }
            for dc in p.lock()?.destination_connections()? {
                value_map.insert(dc.identifier().into(), dc.profile());
            }
        }
        Ok(value_map.values().map(|v|{v.clone()}).collect::<Vec<ConnectionProfile>>())
    }

}

#[test]
fn basename_test() {
    let fps = "./target/debug/increment_process.dylib";
    let fp = PathBuf::from(fps);
    assert!(fp.file_stem().unwrap() == "increment_process");
}

fn precreate_check(manifest: Value) -> JuizResult<Value> {
    log::trace!("precreate_check(manifest={manifest:}) called");
    gen_identifier(gen_name_if_noname(check_has_type_name(manifest)?)?).or_else(|e| {
        log::trace!("precreate_check() failed. Error({e})");
        Err(e)
    })
}

fn gen_name_if_noname(mut manifest: Value) -> JuizResult<Value> {
    if manifest.get("name").is_some() {
        return Ok(manifest);
    }
    let name = type_name(&manifest)?.to_string() + "0";
    manifest.as_object_mut().unwrap().insert("name".to_string(), jvalue!(name));
    return Ok(manifest);
}

fn gen_identifier(mut manifest: Value) -> JuizResult<Value> {
    let name = obj_get_str(&manifest, "name")?;
    let type_name = obj_get_str(&manifest, "type_name")?;
    let id = "core://" .to_string()+ name + ":" + type_name;
    manifest.as_object_mut().unwrap().insert("identifier".to_string(), jvalue!(id));
    return Ok(manifest);
}

fn check_has_type_name(manifest: Value) -> JuizResult<Value> {
    let manifest_updated = manifest.clone();
    // let _ = obj_get_str(&manifest,"name")?;
    let _ = obj_get_str(&manifest, "type_name")?;
    return Ok(manifest_updated)
}