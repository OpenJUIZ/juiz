use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering}, 
    Arc, Mutex};
use std::time::{self, Duration};
use home::home_dir;
use juiz_sdk::anyhow::{self, anyhow, Context};
use juiz_sdk::manifests::ContainerIdentifier;
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::utils::manifest_util::manifest_merge;
use juiz_sdk::utils::yaml_conf_load::yaml_conf_load_with;

use crate::brokers::broker_ptr::BrokerPtr;
use crate::prelude::*;

use crate::brokers::{
        broker_proxy::SystemBrokerProxy,
        broker_factories_wrapper::BrokerFactoriesWrapper};

use super::system_builder;
use super::system_store::{SystemStore, SystemStorePtr};

type SpinCallbackFunctionType = dyn Fn() -> JuizResult<()>;

#[allow(dead_code)]
pub struct System {
    core: ObjectCore,
    core_broker: CoreBrokerPtr,
    store: SystemStorePtr,
    sleep_time: Duration,
    pub tokio_runtime: tokio::runtime::Runtime,
    spin_callback: Option<Box<SpinCallbackFunctionType>>,
    working_dir: Option<PathBuf>,
}

fn check_system_manifest(manifest: Value) -> JuizResult<Value> {
    if !manifest.is_object() {
        return Err(anyhow::Error::from(JuizError::ValueIsNotObjectError{value:manifest}).context("check_system_manifest failed."));
    }
    return Ok(manifest);
}

impl JuizObjectCoreHolder for System {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for System {
    fn profile_full(&self) -> JuizResult<Value> {
        let bf: Value = self.core_broker().lock()?.profile_full()?.try_into()?;
        let p = self.core.profile_full()?;
        Ok(obj_merge(p, &jvalue!({
            "core_broker": bf,
        }))?.into())
    }
}

impl System {

    pub fn new(manifest: Value) -> JuizResult<System> {
        log::trace!("【呼出】System::new({manifest})");
        let checked_manifest = check_system_manifest(manifest)?;
        let updated_manifest:Value = merge_home_manifest(checked_manifest)?;
        let store = SystemStorePtr::new(SystemStore::new());
        Ok(System {
            core: ObjectCore::create(JuizObjectClass::System("System"), "system", "system"),
            //manifest: updated_manifest.clone(),
            core_broker: CoreBrokerPtr::new(CoreBroker::new(updated_manifest, store.clone())?),
            sleep_time: time::Duration::from_millis(100),
            store,
            tokio_runtime: tokio::runtime::Builder::new_multi_thread().thread_name("juiz_core::System").worker_threads(4).enable_all().build().unwrap(),
            spin_callback: None,
            working_dir: None,
        })
    }


    pub fn set_spin_callback(&mut self, cb: Box<SpinCallbackFunctionType>) -> () {
        self.spin_callback = Some(cb);
    }

    pub fn set_spin_sleeptime(&mut self, duration: Duration) -> () {
        self.sleep_time = duration;
    }

    pub fn core_broker(&self) -> &CoreBrokerPtr {
        &self.core_broker
    }

    pub fn set_working_dir(mut self, path: &Path) -> Self {
        self.working_dir = Some(path.into());
        self
    }

    pub fn get_working_dir(&self) -> Option<PathBuf> {
        self.working_dir.clone()
    }


    // pub fn any_process_from_typename_and_name(&self, type_name: &str, name: &str) -> JuizResult<ProcessPtr> {
    //     let result = self.process_from_typename_and_name(type_name, name);
    //     if result.is_ok() {
    //         return result;
    //     }
    //     self.container_process_from_typename_and_name(type_name, name)
    // }

    // pub fn process_proxy(&self, id: &Identifier) -> JuizResult<ProcessPtr> {
    //     self.core_broker.lock_mut()?.process_proxy_from_identifier(id)
    // }

    // pub fn container_process_proxy(&self, id: &Identifier) -> JuizResult<ProcessPtr> {
    //     self.core_broker.lock_mut()?.container_process_proxy_from_identifier(id)
    // }

    // pub fn ec_proxy(&self, id: &Identifier) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
    //     self.core_broker.lock_mut()?.ec_proxy_from_identifier(id)
    // }

    pub fn setup(mut self) -> JuizResult<Self> {
        log::trace!("【呼出】System::setup()");
        let manifest_copied = self.core_broker().lock()?.worker().manifest();
        log::debug!("システムのセットアップを開始します。 {:}", manifest_copied);
        let option = self.get_opt();
        //log::info!("option: {option:}");
        let _ = when_contains_do_mut(&manifest_copied, "plugins", |v| {
            system_builder::setup_plugins(&mut self, v, &option).context("system_builder::setup_plugins in System::setup() failed")
        })?;

        system_builder::setup_objects(&mut self, &manifest_copied).context("setup_objects() failed in setup in system.rs")?;

        system_builder::setup_topic_synchronization(&mut self)?;

        system_builder::setup_ec_activation(&mut self)?;
        log::debug!("システムのセットアップに成功しました。");
        Ok(self)
    }

    fn cleanup(&mut self) -> JuizResult<()> {
        system_builder::cleanup_objects(self)
    }


    pub fn add_systemproxy_by_id(self, id_opt: Option<Identifier>) -> JuizResult<Self> {
        log::trace!("【呼出】add_systemproxy_by_id(id={id_opt:?})");
        if id_opt.is_none() {
            return Ok(self);
        }
        let id = id_opt.unwrap();
        let id_struct = IdentifierStruct::new_broker_id(id.clone())?;
        let _profile = id_struct.to_broker_manifest();
        let broker_type_name = id_struct.broker_type_name;
        let broker_name = id_struct.broker_name;
        let create_when_not_found = true;
        match self.core_broker().lock_mut().unwrap().worker_mut().broker_proxy(broker_type_name.as_str(), broker_name.as_str(), create_when_not_found) {
            Ok(_) => {},
            Err(e) => {
                log::error!("Error in add_systemproxy_by_id(id={id:?}). Error({e:})");
                return Err(anyhow!(e));
            }
        } 
        Ok(self)
    }

    pub fn add_subsystem_by_id(self, id_opt: Option<Identifier>) -> JuizResult<Self> {
        log::trace!("【呼出】add_subsystem_by_id(id={id_opt:?})");
        if id_opt.is_none() {
            return Ok(self);
        }
        let id = id_opt.unwrap();
        let id_struct = IdentifierStruct::new_broker_id(id.clone())?;
        let profile = id_struct.to_broker_manifest();
        match self.core_broker().lock_mut().unwrap().system_add_subsystem(profile) {
            Ok(_) => {},
            Err(e) => {
                log::error!("Error in add_subsystem_by_id(id={id:?}). Error({e:})");
                return Err(anyhow!(e));
            }
        } 
        Ok(self)
    }

    pub fn start_brokers(&mut self) -> JuizResult<()> {
        log::trace!("【呼出】start_brokers()");
        match self.store.lock_mut() {
            Ok(store) => {
                let profs = store.brokers.iter().map(|(type_name, broker)| {
                    log::debug!("ブローカ({type_name:})を開始します。");
                    //store.register_broker(broker.clone());
                    let p = match broker.lock_mut() {
                        Ok(mut b) =>{
                            b.start()?;
                            b.wait_until_started(Duration::from_secs_f64(3.0))?;
                            log::info!("ブローカー({type_name:?})が開始されました。");
                            Ok((b.profile_full()?, broker.clone()))
                        }
                        Err(e) => {
                            log::error!("Broker({type_name:}) lock failed. Error({e:?})");
                            Err(anyhow!(JuizError::ObjectLockError { target: format!("Broker({type_name})") }))
                        }
                    }?;
                    Ok(p)
                }).collect::<JuizResult<Vec<(Value, BrokerPtr)>>>()?;
                profs.into_iter().for_each(|(v, _b)| {
                    let type_name = v.as_object().unwrap().get("type_name").unwrap().as_str().unwrap().to_owned();
                    let _ = self.core_broker().lock_mut().unwrap().worker_mut().store_mut().register_broker_manifest(type_name.as_str(), v)
                        .or_else(|e| {
                        log::error!("【エラー】Store::register_broker({type_name})失敗。エラー({e:?})");
                        Err(e)
                    });
                });
                Ok(())
            },
            Err(e) => {
                Err(e)
            }
        }
    }

    pub fn wait_brokers_started(&self) -> JuizResult<()> {
        log::trace!("【呼出】wait_brokers_started()");
        // std::thread::sleep(Duration::from_secs_f64(3.0));
        Ok(())
    }

    fn get_opt(&self) -> Value {
        let manif_copied = self.core_broker().lock().unwrap().worker().manifest();
        let manif_obj = manif_copied.as_object().unwrap();
        if manif_obj.contains_key("option") {
            manif_obj.get("option").unwrap().clone()
        } else {
            jvalue!({})
        }
    }

    pub fn start_http_broker(self, flag_start: bool) -> Self {
        log::trace!("【呼出】start_http_brokers(flag_start={flag_start})");
        match self.core_broker().lock_mut() {
            Ok(mut cb) => {
                let opt_value = cb.worker_mut().get_opt_mut().as_object_mut().unwrap();
                if opt_value.contains_key("http_broker") {
                    opt_value.get_mut("http_broker").unwrap().as_object_mut().unwrap().insert("start".to_owned(), jvalue!(flag_start));
                } else {
                    opt_value.insert("http_broker".to_owned(), jvalue!({"start": flag_start}));
                };
            }
            Err(_) => {
                panic!()
            }
        };
        self
    }

    pub fn cleanup_brokers(&mut self) -> JuizResult<()> {
        log::trace!("【呼出】System::cleanup_brokers()");
        self.store.lock_mut()?.brokers.clear();
        log::trace!("ブローカ生成");
        self.store.lock_mut()?.broker_factories.clear();
        log::trace!("ブローカファクトリー生成");
        
        log::trace!("【完了】System::cleanup_brokers()");
        Ok(())
    }

    ///
    /// SIGINTおよびSIGTERMを待つ。待つ間はsleep_time秒だけsleepするたびにself.spin()を呼ぶ。
    /// 
    pub fn wait_for_singal(&mut self) -> JuizResult<()> {
        let term = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term));
        let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term));
        while !term.load(Ordering::Relaxed) {
            self.spin();
            std::thread::sleep(self.sleep_time);
        }
        Ok(())
    }

    fn stop(&mut self) -> JuizResult<()> {

        for (type_name, broker) in self.store.lock()?.brokers.iter() {
            log::info!("ブローカ({type_name:})を停止します。");
            let _ = broker.lock_mut()?.stop()?;
        }

        Ok(())
    }

    ///
    /// run中およびrun_and_do中に呼ばれる周期実行関数。
    /// 
    fn spin(&mut self) -> () {
        // log::debug!("System::spin() called");
        if self.spin_callback.is_some() {
            let _ = self.spin_callback.as_ref().unwrap()();
        }
    }

    pub fn run(&mut self) -> JuizResult<()> {
        log::trace!("【呼出】System::run()");
        log::info!("【run】Juizシステム({})開始しました。", self.store.uuid()?);
        // self.setup().context("System::setup() in System::run() failed.")?;
        self.wait_for_singal().context("System::wait_for_signal() in System::run() failed.")?;
        self.stop()?;
        log::debug!("【終了】System::run()");
        self.cleanup()?;
        Ok(())
    }

    pub fn run_and_do(&mut self,  func: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
        log::trace!("【呼出】System::run_and_do()");
        // self.setup().context("System::setup() in System::run_and_do() failed.")?;
        log::info!("【run_and_do】Juizシステム({})スタートしました。", self.store.uuid()?);
        (func)(self).context("User function passed for System::run_and_do() failed.")?;
        self.wait_for_singal().context("System::wait_for_signal() in System::run_and_do() failed.")?;
        log::debug!("【終了】System::run_and_do()");
        self.stop()?;
        self.cleanup()?;
        Ok(())
    }

    pub fn run_and_do_once(&mut self, func: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()>  {
        log::trace!("【呼出】System::run_and_do_once()");
        // self.setup().context("System::setup() in System::run_and_do_once() failed.")?;
        log::info!("【run_and_do_once】Juizシステム({})スタートしました。", self.store.uuid()?);
        (func)(self).context("User function passed for System::run_and_do_once() failed.")?;
        //self.wait_for_singal().context("System::wait_for_signal() in System::run_and_do() failed.")?;
        self.stop()?;
        log::debug!("【終了】System::run_and_do_once()");
        self.cleanup()?;
        Ok(())
    }

    // pub fn broker_proxy(&self, manifest: &Value, create_when_not_found: bool) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
    //     log::trace!("System::broker_proxy({}) called", manifest);
    //     self.core_broker.lock_mut()?.broker_proxy_from_manifest(manifest, create_when_not_found)
    // }

    pub fn register_broker_factories_wrapper(&mut self, bf: Arc<Mutex<BrokerFactoriesWrapper>>) -> JuizResult<Arc<Mutex<BrokerFactoriesWrapper>>> {
        // log::trace!("【呼出】System::egister_broker_factories_wrapper()");
        let type_name = juiz_lock(&bf)?.type_name().to_string();
        log::trace!("【呼出】System::register_broker_factories_wrapper(BrokerFactory(type_name={:?})) called", type_name);
        if self.store.lock()?.broker_factories.contains_key(&type_name) {
            log::error!("system already contains broker factory with type_name='{type_name:}'.");
            return Err(anyhow::Error::from(JuizError::BrokerFactoryOfSameTypeNameAlreadyExistsError{type_name: type_name}));
        }
        self.store.lock_mut()?.broker_factories.insert(type_name.clone(), Arc::clone(&bf));
        self.core_broker().lock_mut()?.worker_mut().store_mut().register_broker_factory_manifest(type_name.as_str(), juiz_lock(&bf)?.profile_full()?.try_into()?)?;
        self.core_broker().lock_mut()?.worker_mut().store_mut().broker_proxies.register_factory(juiz_lock(&bf)?.broker_proxy_factory.clone())?;
        log::trace!("System::register_broker_factories_wrapper(BrokerFactory(type_name={:?})) exit", type_name);
        Ok(bf)
    }

    fn broker_factories_wrapper(&self, type_name: &str) -> JuizResult<Arc<Mutex<BrokerFactoriesWrapper>>> {
        match self.store.lock()?.broker_factories.get(type_name) {
            None => Err(anyhow::Error::from(JuizError::BrokerFactoryCanNotFoundError{type_name: type_name.to_string()})),
            Some(bf) => Ok(bf.clone())
        }
    }

    pub fn create_broker(&mut self, manifest: &Value) -> JuizResult<BrokerPtr> {
        log::trace!("【呼出】System::create_broker({manifest:}) called");
        let type_name = obj_get_str(manifest, "type_name")?;
        let bf = self.broker_factories_wrapper(type_name)?;
        let b = juiz_lock(&bf)?.create_broker(&manifest).context("BrokerFactoriesWrapper.create_broker() failed in System::create_broker()")?;
        self.register_broker(b)
//        Ok(b)
    }

    pub(crate) fn register_broker(&self, broker: BrokerPtr) -> JuizResult<BrokerPtr> {
        //let type_name = broker.lock()?.type_name().to_owned();
        //log::info!("System::register_broker(type_name={type_name:}) called");
        //self.store.lock_mut()?.brokers.insert(type_name.clone(), broker.clone());
        //let p: Value  = broker.lock()?.profile_full()?.try_into()?;
        self.store.lock_mut()?.register_broker(broker)
        //log::info!(" - profile: {p}");
        //self.core_broker().lock_mut()?.worker_mut().store_mut().register_broker_manifest(type_name.as_str(), p)?;
       //Ok(broker)
    }

    pub fn create_broker_proxy(&mut self, manifest: &Value) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        log::trace!("【呼出】System::create_broker_proxy({manifest:}) called");
        //self.core_broker().lock_mut().cre
        let bp = self.core_broker().lock()?.create_broker_proxy(manifest.clone())?;
        self.register_broker_proxy(bp)
    }
    
    pub(crate) fn register_broker_proxy(&mut self, broker_proxy: Arc<Mutex<dyn BrokerProxy>>) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        let type_name =juiz_lock(&broker_proxy).context("Locking broker to get type_name failed.")?.type_name().to_string();
        log::trace!("【呼出】System::register_broker(type_name={type_name:}) called");
        self.core_broker().lock_mut()?.worker_mut().store_mut().broker_proxies.register(broker_proxy.clone())?;
        Ok(broker_proxy)
    }

    pub fn process_list(&self, recursive: bool) -> JuizResult<Vec<ProcessIdentifier>> {
        log::trace!("【呼出】System::process_list({recursive})が呼ばれました。");
        let mut local_processes = self.core_broker().lock()?.worker().store().processes_id();
        log::debug!("【process_list】ローカルなプロセスのリストは {local_processes:?}");
        if recursive {
            for (_, proxy) in self.core_broker().lock()?.worker().store().broker_proxies.objects().iter() {
                // log::trace!("process_list for proxy ()");
                log::debug!("BrokerProxyに対してprocess_listを試みます。");
                for v in juiz_lock(proxy)?.process_list(recursive, None)?.iter() {
                    log::debug!(" - {v} が追加されます");
                    local_processes.push(v.clone());
                }
            }
        }
        log::debug!("ids: {local_processes:?}");    
        return Ok(local_processes);
    }

    pub fn container_list(&self, recursive: bool) -> JuizResult<Vec<ContainerIdentifier>> {
        log::trace!("【呼出】System::container_list() called");
        let mut local_containers = self.core_broker().lock()?.worker().store().containers_id();
        if recursive {
            for (_, proxy) in self.core_broker().lock()?.worker().store().broker_proxies.objects().iter() {
                for c in juiz_lock(proxy)?.container_list(recursive, None)?.iter() {
                    local_containers.push(c.clone());
                }
            }
        }
        log::debug!("ids: {local_containers:?}");    
        return Ok(local_containers);
    }

    pub fn container_process_list(&self, recursive: bool) -> JuizResult<Vec<ProcessIdentifier>> {
        log::trace!("【呼出】System::container_process_list() called");
        let mut local_processes = self.core_broker().lock()?.worker().store().container_processes_id();
        for (_, proxy) in self.core_broker().lock()?.worker().store().broker_proxies.objects().iter() {
            for v in juiz_lock(proxy)?.container_process_list(recursive, None)?.iter() {
                local_processes.push(v.clone());
            }
        }
        log::debug!("ids: {local_processes:?}");    
        Ok(local_processes)
    }

    pub fn any_process_list(&self, recursive: bool) -> JuizResult<Vec<ProcessIdentifier>> {
        log::trace!("【呼出】System::any_process_list() called");
        let mut ps = self.process_list(recursive)?;
        let mut cps = self.container_process_list(recursive)?;
        cps.append(&mut ps);
        return Ok(cps)
    }

    pub fn ec_list(&self, recursive: bool) -> JuizResult<Vec<Value>> {
        log::trace!("【呼出】System::ec_list() called");
        let mut local_ecs = self.core_broker().lock()?.worker().store().ecs.list_manifests()?;
        if recursive {
            for (_, proxy) in self.core_broker().lock()?.worker().store().broker_proxies.objects().iter() {
                log::trace!("ec_list for proxy ()");
                for v in get_array(&juiz_lock(proxy)?.ec_list(recursive)?)?.iter() {
                    local_ecs.push(v.clone());
                }
            }
        }
        log::debug!("ids: {local_ecs:?}");    
        return Ok(local_ecs);
    }

    //
    // pub fn ec_from_id(&self, id: &Identifier) -> JuizResult<Arc<Mutex<dyn ExecutionContextFunction>>> {
    //     log::trace!("System::ec_from_id(id={id:}) called");
    //     let s = IdentifierStruct::try_from(id.clone()).unwrap();
    //     if s.broker_type_name == "core" {
    //         return self.core_broker().lock()?.worker().store().ecs.get(id);
    //     }
    //     self.ec_proxy(id)
    // }
    
}


fn param_map(juiz_homepath: PathBuf) -> HashMap<&'static str, String> {
    HashMap::from([("${HOME}", juiz_homepath.to_str().unwrap().to_owned())])
}

fn merge_home_manifest(manifest: Value) -> JuizResult<Value> {
    log::trace!("【呼出】merge_home_manifest({manifest}) called");
    match home_dir() {
        Some(homepath) => {
            log::debug!("HOME検出 ({homepath:?})");
            let juiz_homepath = homepath.join(".juiz");
            let juiz_conf_homepath = juiz_homepath.join("conf");
            let juiz_default_conf_filepath = juiz_conf_homepath.join("default.conf");
            if juiz_default_conf_filepath.exists() {
                log::debug!("検出$HOME/.juiz/conf/default.conf");
                let system_manifest = yaml_conf_load_with(juiz_default_conf_filepath.to_str().unwrap().to_owned(), param_map(juiz_homepath))?;
                log::debug!(" - system_manifest: {system_manifest:}");
                let merged_manifest =  manifest_merge(system_manifest, &manifest)?;
                log::debug!(" - merged manifest: {merged_manifest:}");
                return Ok(merged_manifest);
            } else {
                log::debug!("未検出：$HOME/.juiz/conf/default.conf");
            }
        }
        None => {}
    }

    Ok(manifest)
}