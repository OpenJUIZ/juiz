use crate::prelude::*;
use juiz_sdk::anyhow::{anyhow, Context};
use juiz_sdk::connection_identifier::ConnectionIdentifier;
use juiz_sdk::connections::{ConnectionManifest, ConnectionProfile};
use juiz_sdk::manifests::ContainerIdentifier;
use juiz_sdk::manifests::{ContainerProfile, ProcessProfile};
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::topic_identifier::TopicIdentifier;
use juiz_sdk::utils::check_corebroker_manifest;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

use crate::brokers::broker_proxy::{
    BrokerBrokerProxy, ConnectionBrokerProxy, ContainerBrokerProxy, ContainerProcessBrokerProxy,
    ExecutionContextBrokerProxy, ProcessBrokerProxy, SystemBrokerProxy, TopicBrokerProxy,
};
use crate::brokers::BrokerProxy;

use crate::core::CoreWorker;
use crate::core::SubSystemProxy;
use crate::core::SystemStorePtr;

#[allow(unused)]
// #[derive(Debug)]
pub struct CoreBroker {
    core: ObjectCore,
    manifest: Value,
    worker: CoreWorker,
    master_system_proxy: Option<SubSystemProxy>,
    subsystem_proxies: Vec<SubSystemProxy>,
    system_store: SystemStorePtr,
}

#[derive(Clone)]
pub struct CoreBrokerPtr {
    ptr: Arc<RwLock<CoreBroker>>,
}

unsafe impl Send for CoreBrokerPtr {}

impl CoreBrokerPtr {
    pub fn new(core_broker: CoreBroker) -> Self {
        Self {
            ptr: Arc::new(RwLock::new(core_broker)),
        }
    }
    pub fn lock(&self) -> JuizResult<RwLockReadGuard<CoreBroker>> {
        self.ptr.read().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "CoreBrokerPtr".to_owned()
            }))
        })
    }

    pub fn lock_mut(&self) -> JuizResult<RwLockWriteGuard<CoreBroker>> {
        self.ptr.write().or_else(|_| {
            Err(anyhow!(JuizError::ObjectLockError {
                target: "CoreBrokerPtr".to_owned()
            }))
        })
    }
}

impl CoreBroker {
    pub fn new(manifest: Value, system_store: SystemStorePtr) -> JuizResult<CoreBroker> {
        let uuid = system_store.uuid()?;
        Ok(CoreBroker {
            worker: CoreWorker::new(uuid, manifest.clone()),
            core: ObjectCore::create(JuizObjectClass::BrokerProxy("CoreBroker"), "core", "core"),
            manifest: check_corebroker_manifest(manifest)?,
            master_system_proxy: None,
            subsystem_proxies: Vec::new(),
            system_store,
        })
    }

    pub fn worker(&self) -> &CoreWorker {
        &self.worker
    }

    pub fn worker_mut(&mut self) -> &mut CoreWorker {
        &mut self.worker
    }

    pub fn system_store(&self) -> &SystemStorePtr {
        &self.system_store
    }

    pub fn system_store_mut(&mut self) -> &mut SystemStorePtr {
        &mut self.system_store
    }

    fn find_subsystem_by_uuid(&self, uuid: Uuid) -> Option<SubSystemProxy> {
        if self.master_system_proxy.is_some()
            && self.master_system_proxy.as_ref().unwrap().uuid() == &uuid
        {
            Some(self.master_system_proxy.as_ref().unwrap().clone())
        } else {
            for ssp in self.subsystem_proxies.iter() {
                // 呼び出し元のUUIDがサブシステムと一緒でなければ検査
                if ssp.uuid() == &uuid {
                    return Some(ssp.clone());
                }
            }
            None
        }
    }

    pub fn create_broker_proxy(
        &self,
        broker_manifest: Value,
    ) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
        self.system_store
            .create_broker_proxy(self.worker(), &broker_manifest)
    }

    pub fn reserve_master_broker(&mut self, master_info: Value) -> JuizResult<()> {
        log::trace!("reserve_master_broker({master_info:}) called");
        // let broker_type = obj_get_str(&master_info, "broker_type");
        self.worker_mut().reserve_master_broker(master_info)
    }
}

impl JuizObjectCoreHolder for CoreBroker {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for CoreBroker {
    fn profile_full(&self) -> JuizResult<Value> {
        let v = obj_merge(
            self.core.profile_full()?,
            &jvalue!({
                "core_store" : self.worker().store().profile_full()?,
            }),
        )?;
        let master_profile = if let Some(system) = self.master_system_proxy.as_ref() {
            system.profile_full()?
        } else {
            Value::Null
        };
        Ok(obj_merge(v, &jvalue!({
            "system_store" : self.system_store.profile_full()?,
            "mastersystem": master_profile,
            "subsystems": self.subsystem_proxies.iter().map(|p|{p.profile_full().unwrap()}).collect::<Vec<Value>>()
        }))?.into())
    }
}

impl SystemBrokerProxy for CoreBroker {
    fn system_profile_full(&self) -> JuizResult<Value> {
        log::trace!("【呼出】system_profile_full() called");
        let result = self.profile_full();
        result
    }

    fn system_filesystem_list(&self, path_buf: PathBuf) -> JuizResult<Value> {
        log::trace!("【呼出】system_filesystem_list({path_buf:?})");
        if path_buf.is_relative() {
            let cwd = std::env::current_dir()?;
            let mut entries = std::fs::read_dir(path_buf.clone())?
                .map(|res| {
                    res.map(|e| {
                        jvalue!({
                            "path": cwd.join(e.path().to_str().unwrap()),
                            "is_dir": e.path().is_dir()
                        })
                    })
                    .or::<JuizError>(Ok(jvalue!("Error")))
                    .unwrap()
                })
                .collect::<Vec<Value>>();
            entries.push(jvalue!({
                //"path": ".",
                "is_dir": true,
                "path": std::env::current_dir()?.join(path_buf)
            }));
            Ok(entries.into())
        } else {
            let entries = std::fs::read_dir(path_buf.clone())?
                .map(|res| {
                    res.map(|e| {
                        jvalue!({
                            "path": e.path().to_str().unwrap(),
                            "is_dir": e.path().is_dir()
                        })
                    })
                    .or::<JuizError>(Ok(jvalue!("Error")))
                    .unwrap()
                })
                .collect::<Vec<Value>>();
            Ok(entries.into())
        }
    }

    /// サブシステムの追加
    ///
    ///
    ///
    fn system_add_subsystem(&mut self, profile: Value) -> JuizResult<Value> {
        log::trace!("【呼出】system_add_subsystem({profile})");
        // 相手方のBrokerProxyを作成
        //let bp = self.system_store.create_broker_proxy(self.worker(), &profile)?;
        // 相手のUUIDを得る。
        let (confirmation_request, uuid_value, bp) = match profile
            .as_object()
            .unwrap()
            .get("mastersystem")
        {
            Some(msv) => {
                log::debug!("【system_add_subsystem】渡されたprofileにmastersystem情報({msv:})が含まれていました。BrokerProxyを作成します。");
                let bp = self.system_store.create_broker_proxy(self.worker(), &msv)?;
                match msv.as_object().unwrap().get("uuid") {
                    Some(v) => Ok((true, v.clone(), bp)),
                    None => {
                        log::error!("【system_add_subsystem】UUID取得に失敗しました。");
                        Err(anyhow!(JuizError::InvalidArgumentError {
                            message: "system_add_subsystem failed".to_owned()
                        }))
                    }
                }
            }
            None => {
                log::debug!("【system_add_subsystem】関数に渡されたprofileにuuidが含まれていません。サブシステムから直接UUIDを取得します。");

                let bp = self
                    .system_store
                    .create_broker_proxy(self.worker(), &profile)?;
                let v = bp
                    .lock()
                    .or_else(|_e| {
                        Err(anyhow!(JuizError::ObjectLockError {
                            target: "system_store".to_owned()
                        }))
                    })
                    .and_then(|b| {
                        let v = b.system_uuid()?;
                        Ok(v)
                    })?;
                Ok((false, v, bp))
            }
        }?;
        // 相手のuuidをUuid型に変換
        let uuid_str = uuid_value.as_str().unwrap();
        let uuid: Uuid = Uuid::parse_str(uuid_str).unwrap();
        log::debug!("【system_add_subsystem】UUID ({uuid_str}) 取得に成功しました。");
        // ここですでにuuidが登録されているかを確認する。
        for subsystem_proxy in self.subsystem_proxies.iter() {
            if &uuid == subsystem_proxy.uuid() {
                log::error!(
                    "system_add_subsystem failed. Subsystem(uuid={uuid}) has already added."
                );
                return Err(anyhow!(JuizError::ObjectAlreadyRegisteredError {
                    message: format!(
                        "system_add_subsystem failed. Subsystem(uuid={uuid}) has already added."
                    )
                }));
            }
        }
        // さらにサブシステムのサブシステムまでこれから登録するUUIDがあるかみようとするけど、これは無意味かも。
        // ループ構造ができないようにする責任は設計者にある
        for subsystem_proxy in self.subsystem_proxies.iter() {
            let ss = subsystem_proxy.subsystems()?;
            log::warn!("WARNING: SUBSYSTEM's SUBSYSTEM mining.... But this is useless...");
            log::warn!("value is {ss:}");
        }

        // 自分のUUID
        let my_uuid = self.system_store.uuid()?;
        self.worker_mut()
            .store_mut()
            .broker_proxies
            .register(bp.clone())?; // 作った相手方のBrokerProxyを自分に登録しておく
        let subsystem_proxy = SubSystemProxy::new(uuid, bp.clone())?;
        let ssprofile = juiz_lock(&subsystem_proxy.broker_proxy())?
            .profile_full()
            .context("subsystem_proxy.broker_proxy().profile_full() in system_add_subsystem")?;
        // 相手方がどのIPアドレスを辿ってきたかがaccessed_broker_idでわかる
        let _accessed_broker_id = match profile.as_object() {
            Some(obj) => match obj.get("accessed_broker_id") {
                Some(accessed_broker_id) => {
                    accessed_broker_id.as_str().or_else(|| Some("")).unwrap()
                }
                None => "",
            },
            None => "",
        };
        log::info!("Added subsystem({})", ssprofile);
        //log::info!("accessed_broker_id = {}", accessed_broker_id);

        // 相手にこちら側のBrokerの名前を教えるために検索
        // 自分が保持しているbrokerの中に、サブシステム追加時にリクエストがあった
        // type_nameと同じtype_nameを持っているものがいるかどうかを検索する
        // つまり、リクエストされたプロトコルに対応しているかどうかを確認している。
        let broker_type = ssprofile
            .as_object()
            .unwrap()
            .get("type_name")
            .unwrap()
            .as_str()
            .unwrap();
        let mut broker_name: Option<String> = None;
        for prof in self.worker().store().brokers_profile_full()?.iter() {
            let broker_broker_type = prof.type_name.clone();
            log::debug!("system includes broker ({broker_broker_type})");
            if broker_broker_type == broker_type {
                broker_name = Some(prof.name.clone());
            }
        }
        if broker_name.is_none() {
            log::error!("Broker (type={broker_type}) can not be found.");
            return Err(anyhow!(JuizError::InvalidArgumentError {
                message: "system_add_subsystem() failed. Invalid argument".to_owned()
            }));
        }
        log::debug!("In system_add_subsystem({profile}), now connecting to subsystem({ssprofile}) with broker(type_name={broker_type})");
        // 確認のためのリクエストでなければ
        if !confirmation_request {
            log::debug!("In system_add_subsystem({profile}), this is NOT confirmation request.");
            // 相手にmaster側のproxyのbrokerのタイプや名前を教える
            let master_profile = jvalue!({
                "subsystem": {
                    "uuid": my_uuid.to_string(),
                    "broker_type": broker_type,
                    "broker_name": broker_name.unwrap(),
                }
            });
            log::info!("In system_add_subsystem({profile}), sending back master_profile: {master_profile:}");
            let _ = subsystem_proxy.broker_proxy().lock().or_else(|_e|{
                Err(anyhow!(JuizError::ObjectLockError { target: "system_proxy".to_owned() }))
            }).and_then(|mut bp|{
                bp.system_add_mastersystem(master_profile)
            }).or_else(|e|{
                log::error!("subsystem_proxy.broker_proxy().system_add_mastersystem() failed. Error: {e:?}");
                Err(e)
            })?;
        }
        // 最後にサブシステムのProxyを登録しておく。
        self.subsystem_proxies.push(subsystem_proxy);
        log::info!("In system_add_subsystem({profile}) successfully exit");
        Ok(profile)
    }

    fn system_uuid(&self) -> JuizResult<Value> {
        Ok(jvalue!(self.system_store.uuid()?.to_string()))
    }

    /// マスターシステムの追加
    fn system_add_mastersystem(&mut self, profile: Value) -> JuizResult<Value> {
        log::trace!("【呼出】system_add_mastersystem({profile})");
        // まずは渡されたプロファイルを確認して、BrokerProxyへの参照を得る。
        let bp = match profile.as_object() {
            Some(prof_obj) => {
                match prof_obj.get("subsystem") {
                    Some(subsystem_value) => {
                        let broker_name = obj_get_str(subsystem_value, "broker_name")?;
                        let broker_type = obj_get_str(subsystem_value, "broker_type")?;
                        let id_str = IdentifierStruct::new_broker(broker_type, broker_name);
                        // 与えられたサブシステムのプロファイルが良好なので、brokerProxyを必要ならば作る関数を呼ぶ。
                        log::debug!("【system_add_mastersystem】profileに従ってBroker(broker_type={broker_type}, broker_name={broker_name}) を作成します。");
                        self.system_store
                            .create_broker_proxy(self.worker(), &id_str.to_broker_manifest())
                    }
                    None => {
                        log::error!(
                            "【system_add_mastersystem】引数profileにsubsystem情報がありません。"
                        );
                        Err(anyhow!(JuizError::InvalidIdentifierError {
                            message: "".to_owned()
                        }))
                    }
                }
            }
            None => {
                // そもそも渡されたValue型の値がobject型ではない。
                log::error!("【system_add_mastersystem】引数profileがobject型ではありません。");
                Err(anyhow!(JuizError::ValueIsNotObjectError {
                    value: profile.clone()
                }))
            }
        }?;
        // 渡されたプロファイルの中にuuidが含まれていたらそれを使う。なければ取ってくる。
        let uuid_value: Value = match obj_get_obj(&profile, "subsystem")?.get("uuid") {
            Some(v) => {
                log::debug!(
                    "【system_add_mastersystem】引数profileにuuid({v:})が含まれていました。"
                );
                Ok(v.clone())
            }
            None => {
                // サブシステムは未登録とみなしてUUIDを取得する
                log::debug!("【system_add_mastersystem】引数にuuidが含まれていません。これはアプリケーションから呼ばれた最初のリクエストと判断し、サブシステムに対してリクエストを送ります。");
                let my_uuid = self.system_store.uuid()?;
                let bprof = bp.lock().unwrap().profile_full()?;
                let broker_type_name = bprof
                    .as_object()
                    .unwrap()
                    .get("type_name")
                    .unwrap()
                    .as_str()
                    .unwrap();
                let broker_prof = self
                    .broker_list(false)?
                    .iter()
                    .find(|x| {
                        let idstruct: IdentifierStruct =
                            IdentifierStruct::from_broker_identifier(x).unwrap();
                        // log::debug!("In CoreBroker::system_add_mastersystem(), idstruct is {:?}", idstruct);
                        idstruct.broker_type_name == broker_type_name
                    })
                    .unwrap()
                    .clone();
                log::debug!("【system_add_mastersystem】broker_profileは{broker_prof:?}");
                let idstruct: IdentifierStruct =
                    IdentifierStruct::from_broker_identifier(&broker_prof).unwrap();

                let broker_name = idstruct.object_name;
                match juiz_lock(&bp) {
                    Ok(mut b) => {
                        let request = jvalue!({
                        "mastersystem": {
                            "uuid": my_uuid.to_string(),
                            "type_name": broker_type_name,
                            "name": broker_name,
                        }});
                        log::debug!("【system_add_mastersystem】BrokerProxy({bprof:}).system_add_subsystem({request:})を呼びます");
                        let r = match b.system_add_subsystem(request) {
                            Ok(r) => Ok(r),
                            Err(e) => {
                                log::error!("【system_add_mastersystem】system_add_subsystem()が失敗。エラーは({e:?})");
                                Err(e)
                            }
                        }?;
                        log::debug!("【system_add_mastersystem】BrokerProxy.system_add_subsystem()が成功。結果は{r:}。再度UUIDを取得します。");
                        b.system_uuid()
                    }
                    Err(_e) => {
                        todo!()
                    }
                }
                // juiz_lock(&bp)?.system_add_subsystem(jvalue!({
                //     "mastersystem": {
                //         "uuid": my_uuid.to_string(),
                //         "type_name": broker_type_name,
                //         "name": broker_name,
                //     }
                // }))?;
                // juiz_lock(&bp)?.system_uuid()
            }
        }?;
        let uuid_str = uuid_value.as_str().unwrap();
        let uuid: Uuid = Uuid::parse_str(uuid_str).unwrap();
        log::debug!("【system_add_mastersystem】UUID({uuid_str})の取得に成功しました。");
        self.worker_mut()
            .store_mut()
            .broker_proxies
            .register(bp.clone())?;
        log::info!(
            "mastersystem(uuid={uuid_str})を登録しました。マスターシステムのproxyを登録します。"
        );
        let subsystem_proxy = SubSystemProxy::new(uuid, bp)?;
        self.master_system_proxy = Some(subsystem_proxy);
        Ok(profile)
    }

    fn system_load_process(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        log::trace!("【呼出】system_load_process({language}, {filepath})");
        self.worker_mut().load_process_factory(language, filepath)
    }

    fn system_load_container(&mut self, language: String, filepath: String) -> JuizResult<Value> {
        log::trace!("【呼出】system_load_container({language}, {filepath})");
        self.worker_mut().load_container_factory(language, filepath)
    }

    fn system_load_container_process(
        &mut self,
        language: String,
        filepath: String,
    ) -> JuizResult<Value> {
        log::trace!("【呼出】system_load_container_process({language}, {filepath})");
        self.worker_mut()
            .load_container_process_factory(language, filepath)
    }

    fn system_load_component(
        &mut self,
        language: String,
        filepath: String,
    ) -> JuizResult<ComponentManifest> {
        log::trace!("【呼出】system_load_component({language}, {filepath})");
        self.worker_mut().load_component(language, filepath)
    }
}

impl ProcessBrokerProxy for CoreBroker {
    fn process_call(&self, id: &ProcessIdentifier, args: CapsuleMap) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】process_call({id:}, {args})");
        if id.broker_type_name == "core" {
            self.worker()
                .store()
                .processes
                .get(&id.to_string())?
                .lock()?
                .call(args)
        } else {
            self.worker()
                .process_proxy_from_identifier(id, true)?
                .lock()?
                .call(args)
        }
    }

    fn process_execute(&self, id: &ProcessIdentifier) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】process_execute({id:})");
        //let idstruct = IdentifierStruct::try_from(id.clone())?;
        if id.broker_type_name == "core" {
            self.worker()
                .store()
                .processes
                .get(&id.to_string())?
                .lock()?
                .execute()
        } else {
            self.worker()
                .process_proxy_from_identifier(id, true)?
                .lock()?
                .execute()
        }
    }

    fn process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        Ok(self
            .worker()
            .store()
            .processes
            .get(&id.to_string())?
            .lock()?
            .profile()?)
    }

    fn process_list(
        &self,
        recursive: bool,
        _caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>> {
        log::trace!("【呼出】process_list({recursive})");
        let mut ids = self.worker().store().processes_id();
        log::debug!("【process_list】ローカルなStoreにあるProcessは{ids:?}");
        if !recursive {
            return Ok(ids);
        }

        for ssp in self.subsystem_proxies.iter() {
            log::debug!("【process_list】Subsystem({ssp:})にあるprocessも調べます。");
            let plist = juiz_lock(&ssp.broker_proxy())?.process_list(recursive, None)?;

            log::debug!("【process_list】Subsystem({ssp:})にあるprocessは{plist:?}");
            for v in plist.into_iter() {
                let mut mut_pi = v.clone();
                match ssp.broker_proxy().lock().and_then(|bp| {
                    mut_pi.broker_name = bp.name().to_owned();
                    mut_pi.broker_type_name = bp.type_name().to_owned();
                    Ok(())
                }) {
                    Ok(_) => Ok(()),
                    Err(_e) => {
                        log::error!("【process_list】BrokerProxyをlockするのに失敗しました。");
                        Err(anyhow!(JuizError::ObjectLockError {
                            target: format!("BrokerProxy")
                        }))
                    }
                }?;
                log::debug!("【process_list】Process({mut_pi})を追加します。");
                ids.push(mut_pi);
            }
        }
        Ok(ids)
    }

    fn process_push_by(
        &self,
        id: &ProcessIdentifier,
        arg_name: String,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr> {
        self.worker()
            .store()
            .processes
            .get(&id.to_string())?
            .lock()?
            .push_by(arg_name.as_str(), value)
    }

    fn process_try_connect_to(
        &mut self,
        connection_manifest_ref: &ConnectionManifest,
    ) -> JuizResult<ConnectionManifest> {
        log::trace!(
            "【呼出】process_try_connect_to({})",
            connection_manifest_ref
        );
        let mut connection_manifest = connection_manifest_ref.clone();
        connection_manifest.source_process_id.broker_type_name = "core".to_owned();
        connection_manifest.source_process_id.broker_name = "core".to_owned();
        let destination_process = self
            .worker_mut()
            .any_process_proxy_from_identifier(&connection_manifest.destination_process_id, true)?;
        Ok(self
            .worker_mut()
            .any_process_proxy_from_identifier(&connection_manifest.source_process_id, true)?
            .lock_mut()?
            .try_connect_to(destination_process, &connection_manifest)
            .and_then(|v| {
                log::trace!(
                    "【完了】process_try_connect_to({})",
                    connection_manifest_ref
                );
                Ok(v)
            })
            .or_else(|e| {
                log::error!(
                    "【エラー】process_try_connect_to({})。エラーは ({e})",
                    connection_manifest_ref
                );
                Err(e)
            })?
            .into())
    }

    fn process_notify_connected_from(
        &mut self,
        connection_manifest_ref: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile> {
        log::trace!("【呼出】process_notify_connected_from({connection_manifest_ref})");
        let connection_manifest = connection_manifest_ref.clone();
        // connection_manifest.destination_process_id.broker_type_name = "core".to_owned();
        // connection_manifest.destination_process_id.broker_name = "core".to_owned();
        let source_process = self.worker_mut().any_process_proxy_from_identifier(&connection_manifest.source_process_id, true).or_else(|e|{
            log::error!("【process_notify_connected_from】any_process_proxy_from_identifier(source={:})が失敗しました。エラー({e})", connection_manifest.source_process_id);
            Err(e)
        })?; //self.store().processes.get(source_process_id)?;
        Ok(self.worker_mut().any_process_proxy_from_identifier(&connection_manifest.destination_process_id, true).or_else(|e|{
            log::error!("【process_notify_connected_from】any_process_proxy_from_identifier(destination={:})が失敗しました。エラー({e})", connection_manifest.destination_process_id);
            Err(e)
        })?
            .lock_mut()?.notify_connected_from(source_process, &connection_manifest).and_then(|v| {
            log::trace!("【完了】process_notify_connected_from({})", connection_manifest_ref);
            Ok(v)
        }).or_else(|e| {
            log::error!("【エラー】process_notify_connected_from({})。エラーは ({e})", connection_manifest_ref);
            Err(e)
        })?.into())
    }

    fn process_p_apply(
        &mut self,
        id: &ProcessIdentifier,
        arg_name: &str,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr> {
        Ok(self
            .worker()
            .store()
            .processes
            .get(&id.to_string())?
            .lock_mut()?
            .p_apply(arg_name, value)?
            .into())
    }

    fn process_create(&mut self, manifest: &ProcessManifest) -> JuizResult<ProcessProfile> {
        log::trace!("【呼出】process_create({})", manifest);
        self.worker_mut()
            .create_process_ref(manifest)
            .and_then(|v| {
                log::trace!("【完了】process_create({})", manifest);
                Ok(v)
            })
            .or_else(|e| {
                log::error!("【失敗】process_create({})。エラーは({e})", manifest);
                Err(e)
            })?
            .lock()?
            .profile()
    }

    fn process_destroy(&mut self, identifier: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        log::trace!("【呼出】process_destroy({})", identifier);
        match self
            .worker_mut()
            .destroy_process_ref(identifier)?
            .lock_mut()
        {
            Ok(mut p) => {
                let prof = p.profile()?;
                p.purge()?;
                log::trace!("【完了】process_destroy({})", identifier);
                Ok(prof)
            }
            Err(_) => todo!(),
        }
    }

    fn process_openapi_spec(&self, identifier: &ProcessIdentifier) -> JuizResult<Value> {
        log::trace!("【呼出】process_openapi_spec({})", identifier);
        self.worker()
            .store()
            .processes
            .get(&identifier.to_string())?
            .lock_mut()?
            .openapi_spec()
            .and_then(|v| {
                log::trace!("【完了】process_openapi_spec({})", identifier);
                Ok(v)
            })
            .or_else(|e| {
                log::error!(
                    "【失敗】process_openapi_spec({})。エラーは({e})",
                    identifier
                );
                Err(e)
            })
    }
}

impl ContainerBrokerProxy for CoreBroker {
    fn container_profile_full(&self, id: &ContainerIdentifier) -> JuizResult<ContainerProfile> {
        self.worker()
            .store()
            .containers
            .get(&id.to_string())?
            .clone()
            .lock()?
            .profile()
    }

    fn container_list(
        &self,
        recursive: bool,
        _caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ContainerIdentifier>> {
        log::trace!(
            "【呼出】container_list({}, caller={:?})",
            recursive,
            _caller_broker_profile
        );
        let mut ids = self
            .worker()
            .store()
            .containers
            .objects()
            .iter()
            .map(|(_k, c)| c.identifier())
            .collect::<Vec<ContainerIdentifier>>();
        if recursive {
            //for (_id, proxy) in self.store().broker_proxies.objects().iter() {
            for ssp in self.subsystem_proxies.iter() {
                let proxy = ssp.broker_proxy();
                let mut plist = juiz_lock(&proxy)?.container_list(recursive, None)?;
                ids.append(&mut plist);
            }
        }
        Ok(ids)
    }

    fn container_create(
        &mut self,
        manifest: &ContainerManifest,
        args: CapsuleMap,
    ) -> JuizResult<ContainerProfile> {
        log::trace!("【呼出】container_create({}, {})", manifest, args);
        let type_name: String = manifest.type_name.clone(); // manifest.get("type_name")?.try_into()?;
        let name = manifest
            .name
            .clone()
            .or(Some(format!("{}0", type_name)))
            .unwrap();
        self.worker_mut()
            .create_container_ref(type_name.as_str(), name.as_str(), args)
            .and_then(|v| {
                log::trace!("【完了】container_create({})", manifest);
                Ok(v)
            })
            .or_else(|e| {
                log::error!("【エラー】container_create({})", manifest);
                Err(e)
            })?
            .lock()?
            .profile()
    }

    fn container_destroy(
        &mut self,
        identifier: &ContainerIdentifier,
    ) -> JuizResult<ContainerProfile> {
        log::trace!("【呼出】container_destroy({})", identifier);
        self.worker_mut()
            .destroy_container_ref(identifier)
            .and_then(|v| {
                log::trace!("【完了】container_destroy({})", identifier);
                Ok(v)
            })
            .or_else(|e| {
                log::error!("【エラー】container_destroy({})", identifier);
                Err(e)
            })
    }
}

impl ContainerProcessBrokerProxy for CoreBroker {
    fn container_process_profile_full(&self, id: &ProcessIdentifier) -> JuizResult<ProcessProfile> {
        self.worker()
            .store()
            .container_processes
            .get(&id.to_string())?
            .lock()?
            .profile()
    }

    fn container_process_list(
        &self,
        recursive: bool,
        _caller_broker_profile: Option<Value>,
    ) -> JuizResult<Vec<ProcessIdentifier>> {
        let mut ids = self.worker().store().container_processes_id();
        if recursive {
            for ssp in self.subsystem_proxies.iter() {
                let proxy = ssp.broker_proxy();
                let mut plist = juiz_lock(&proxy)?.container_process_list(recursive, None)?;
                ids.append(&mut plist);
            }
        }
        Ok(ids)
    }

    fn container_process_call(
        &self,
        id: &ProcessIdentifier,
        args: CapsuleMap,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("【呼出】container_process_call(id={id:}, {args})");
        if id.broker_type_name == "core" {
            self.worker()
                .store()
                .container_processes
                .get(&id.to_string())?
                .lock()?
                .call(args)
                .and_then(|v| {
                    log::trace!("【完了】container_process_call(id={id:})");
                    Ok(v)
                })
                .or_else(|e| {
                    log::error!("【エラー】container_process_call(id={id:})。エラーは ({e})");
                    Err(e)
                })
        } else {
            self.worker()
                .process_proxy_from_identifier(id, true)?
                .lock()?
                .call(args)
                .and_then(|v| {
                    log::trace!("【完了】container_process_call(id={id:})");
                    Ok(v)
                })
                .or_else(|e| {
                    log::error!("【エラー】container_process_call(id={id:})。エラーは ({e})");
                    Err(e)
                })
        }
    }

    fn container_process_execute(&self, id: &ProcessIdentifier) -> JuizResult<CapsulePtr> {
        if id.broker_type_name == "core" {
            self.worker()
                .store()
                .container_processes
                .get(&id.to_string())?
                .lock()?
                .execute()
                .and_then(|v| {
                    log::trace!("【完了】container_process_execute(id={id:})");
                    Ok(v)
                })
                .or_else(|e| {
                    log::error!("【エラー】container_process_execute(id={id:})。エラーは ({e})");
                    Err(e)
                })
        } else {
            self.worker()
                .process_proxy_from_identifier(id, true)?
                .lock()?
                .execute()
                .and_then(|v| {
                    log::trace!("【完了】container_process_execute(id={id:})");
                    Ok(v)
                })
                .or_else(|e| {
                    log::error!("【エラー】container_process_execute(id={id:})。エラーは ({e})");
                    Err(e)
                })
        }
    }

    fn container_process_create(
        &mut self,
        container_id: &ContainerIdentifier,
        manifest: &ProcessManifest,
    ) -> JuizResult<ProcessProfile> {
        log::trace!(
            "【呼出】container_process_create({}, {})",
            container_id,
            manifest
        );
        let container = self.worker_mut().container_from_identifier(container_id)?;
        self.worker_mut()
            .create_container_process_ref(container, manifest)?
            .lock()?
            .profile()
    }

    fn container_process_destroy(
        &mut self,
        identifier: &ProcessIdentifier,
    ) -> JuizResult<ProcessProfile> {
        log::trace!("【呼出】container_process_destropy{})", identifier);
        self.worker_mut()
            .destroy_container_process_ref(identifier)
            .and_then(|v| {
                log::trace!("【完了】container_process_destroy({identifier:})");
                Ok(v)
            })
            .or_else(|e| {
                log::error!("【エラー】container_process_destroy({identifier:})。エラーは ({e})");
                Err(e)
            })
    }

    fn container_process_p_apply(
        &mut self,
        id: &ProcessIdentifier,
        arg_name: &str,
        value: CapsulePtr,
    ) -> JuizResult<CapsulePtr> {
        Ok(self
            .worker()
            .store()
            .container_processes
            .get(&id.to_string())?
            .lock_mut()?
            .p_apply(arg_name, value)?
            .into())
    }

    fn container_process_openapi_spec(&self, identifier: &ProcessIdentifier) -> JuizResult<Value> {
        log::trace!("【呼出】container_process_openapi_spec({})", identifier);
        self.worker()
            .store()
            .container_processes
            .get(&identifier.to_string())?
            .lock_mut()?
            .openapi_spec()
            .and_then(|v| {
                log::trace!("【完了】container_process_openapi_spec({})", identifier);
                Ok(v)
            })
            .or_else(|e| {
                log::error!(
                    "【失敗】container_process_openapi_spec({})。エラーは({e})",
                    identifier
                );
                Err(e)
            })
    }
}

impl BrokerBrokerProxy for CoreBroker {
    fn broker_list(&self, recursive: bool) -> JuizResult<Vec<String>> {
        println!("【broker_list】ローカルなStoreにあるBrokerを調べます。");
        let mut ids = self.worker().store().brokers_list_ids()?;
        if recursive {
            println!("【broker_list】サブシステムのBrokerも調べます。");
            for ssp in self.subsystem_proxies.iter() {
                let proxy = ssp.broker_proxy();
                let mut plist = juiz_lock(&proxy)?.broker_list(recursive)?;
                ids.append(&mut plist);
            }
        } else {
            println!("【broker_list】サブシステムのBrokerは調べません。");
        }
        println!("【broker_list】Brokerのリストを返します。ids: {ids:?}");
        Ok(ids)
    }

    fn broker_profile_full(&self, id: &Identifier) -> JuizResult<BrokerProfile> {
        self.worker().store().broker_profile_full(id)
    }
}

impl TopicBrokerProxy for CoreBroker {
    fn topic_list(&self) -> JuizResult<Vec<TopicIdentifier>> {
        let mut ids = self.worker().store().topics_list_ids()?;
        if true {
            //for (_, proxy ) in self.store().broker_proxies.objects().iter() {
            for ssp in self.subsystem_proxies.iter() {
                let proxy = ssp.broker_proxy();

                let mut plist = juiz_lock(&proxy)?.topic_list()?;
                ids.append(&mut plist);
            }
        }
        Ok(ids.into())
    }

    fn topic_push(
        &self,
        name: &str,
        capsule: CapsulePtr,
        pushed_system_uuid: Option<Uuid>,
    ) -> JuizResult<()> {
        log::trace!("topic_push(name={name}) called");
        match self.worker().store().topics.get(name) {
            Some(topic) => {
                let r = topic.push(capsule, pushed_system_uuid);
                log::trace!("topic_push(name={name}) exit");
                r
            }
            None => Err(anyhow!(JuizError::ObjectCanNotFoundByIdError {
                id: name.to_owned() + ":topic"
            })),
        }
    }

    fn topic_request_subscribe(
        &mut self,
        name: &str,
        opt_system_uuid: Option<Uuid>,
    ) -> JuizResult<Value> {
        log::trace!("topic_request_subscribe(name={name}) called");
        let mut do_subscribe = false;
        for (topic_name, topic) in self.worker().store().topics.iter() {
            if topic_name.as_str() == name {
                if topic.num_local_subscribers()? > 0 {
                    log::trace!("found subscriber of topic(name={name})");
                    do_subscribe = true;
                }
            }
        }
        let my_uuid = Uuid::parse_str(self.system_uuid()?.as_str().unwrap())?;
        // ここでシステムとマスターシステムに問い合わせて、subscribe要求があれば、自身にTopicを追加して、
        // Systemに対するProxyを新しく生成したTopicに登録してデータがリレーされるようにする
        if let Some(msp) = self.master_system_proxy.clone() {
            // 呼び出し元のUUIDがマスターと一緒でなければマスターを検査
            if opt_system_uuid.is_some() && (msp.uuid() != &opt_system_uuid.unwrap()) {
                let result_value =
                    juiz_lock(&msp.broker_proxy())?.topic_request_subscribe(name, Some(my_uuid))?;
                if obj_get_bool(&result_value, "subscribe")? {
                    // システムが購読を希望していたら、自分のlocalにTopicPtrを作り、それと相手システムを接続する
                    log::trace!("found subscriber of topic(name={name}) in master system");
                    let topic = self.worker_mut().create_topic(name.to_owned())?;
                    topic.register_subscriber_subsystem(msp.clone())?;
                    do_subscribe = true;
                }
            }
        }
        for ssp in self.subsystem_proxies.clone().iter() {
            // 呼び出し元のUUIDがサブシステムと一緒でなければ検査
            if opt_system_uuid.is_some() && (ssp.uuid() != &opt_system_uuid.unwrap()) {
                let result_value =
                    juiz_lock(&ssp.broker_proxy())?.topic_request_subscribe(name, Some(my_uuid))?;
                if obj_get_bool(&result_value, "subscribe")? {
                    log::trace!("found subscriber of topic(name={name}) in subsystem");
                    // システムが購読を希望していたら、自分のlocalにTopicPtrを作り、それと相手システムを接続する
                    let topic = self.worker_mut().create_topic(name.to_owned())?;
                    topic.register_subscriber_subsystem(ssp.clone())?;
                    do_subscribe = true;
                }
            }
        }
        Ok(jvalue!({"subscribe": do_subscribe}))
    }

    fn topic_request_publish(
        &mut self,
        name: &str,
        opt_system_uuid: Option<Uuid>,
    ) -> JuizResult<Value> {
        log::trace!("topic_request_publish(name={name}, uuid={opt_system_uuid:?}) called");

        let opt_parent_system = if opt_system_uuid.is_some() {
            let uuid = opt_system_uuid.unwrap();
            self.find_subsystem_by_uuid(uuid)
        } else {
            // log::trace!("topic_request_publish was NOT requested by subsystem.");
            None
        };

        //まず、自分がpublisherならばtrueを返す準備。
        let mut do_publish = false;
        for (topic_name, topic) in self.worker().store().topics.iter() {
            if topic_name.as_str() == name {
                if topic.num_local_publishers()? > 0 {
                    // 該当する名前をもつTopicをPublishするものを持っている。
                    do_publish = true;
                    if let Some(parent_system) = opt_parent_system.as_ref() {
                        topic.register_subscriber_subsystem(parent_system.clone())?;
                    }
                }
            }
        }

        // サブシステムについて確認する。サブシステムがpublisherとしてtrueを返してきたら、
        // 自分サイドのtopicを
        let my_uuid = Uuid::parse_str(self.system_uuid()?.as_str().unwrap())?;
        // ここでシステムとマスターシステムに問い合わせて、subscribe要求があれば、自身にTopicを追加して、
        // Systemに対するProxyを新しく生成したTopicに登録してデータがリレーされるようにする
        if let Some(msp) = self.master_system_proxy.clone() {
            // 呼び出し元のUUIDがマスターと一緒でなければマスターを検査
            if opt_system_uuid.is_some() && (msp.uuid() != &opt_system_uuid.unwrap()) {
                let uuid = msp.uuid();
                let result_value =
                    juiz_lock(&msp.broker_proxy())?.topic_request_publish(name, Some(my_uuid))?;
                if obj_get_bool(&result_value, "publish")? {
                    log::trace!("Subsystem({}) publishes topic({})", uuid, name);
                    // システムが出版を宣言していたら、自分のlocalにTopicPtrを作り、それと相手システムを接続する
                    let topic = self.worker_mut().create_topic(name.to_owned())?;
                    if let Some(parent_system) = opt_parent_system.as_ref() {
                        topic.register_subscriber_subsystem(parent_system.clone())?;
                    }
                    do_publish = true;
                }
            }
        }
        for ssp in self.subsystem_proxies.clone().iter() {
            // 呼び出し元のUUIDがサブシステムと一緒でなければ検査
            if opt_system_uuid.is_some() && (ssp.uuid() != &opt_system_uuid.unwrap()) {
                let result_value =
                    juiz_lock(&ssp.broker_proxy())?.topic_request_publish(name, Some(my_uuid))?;
                if obj_get_bool(&result_value, "publish")? {
                    let uuid = ssp.uuid();
                    log::trace!("Subsystem({}) publishes topic({})", uuid, name);
                    // システムが購読を希望していたら、自分のlocalにTopicPtrを作り、それと相手システムを接続する
                    let topic = self.worker_mut().create_topic(name.to_owned())?;
                    if let Some(parent_system) = opt_parent_system.as_ref() {
                        topic.register_subscriber_subsystem(parent_system.clone())?;
                    }
                    do_publish = true;
                }
            }
        }
        Ok(jvalue!({"publish": do_publish}))
    }
}

impl ExecutionContextBrokerProxy for CoreBroker {
    fn ec_list(&self, recursive: bool) -> JuizResult<Value> {
        //Ok(self.store().ecs.list_ids()?.into())

        let mut ids = self.worker().store().ecs.list_ids()?;
        let ids_arr = ids.as_array_mut().unwrap();
        if recursive {
            //for (_, proxy) in self.store().broker_proxies.objects().iter() {
            for ssp in self.subsystem_proxies.iter() {
                let proxy = ssp.broker_proxy();

                let plist = juiz_lock(&proxy)?.ec_list(recursive)?;
                for v in get_array(&plist)?.iter() {
                    let id = v.as_str().unwrap();
                    ids_arr.push(id.into());
                }
            }
        }
        Ok(ids)
    }

    fn ec_profile_full(&self, id: &Identifier) -> JuizResult<Value> {
        juiz_lock(&self.worker().store().ecs.get(id)?)
            .with_context(|| {
                format!("locking ec(id={id:}) in CoreBroker::ec_profile_full() function")
            })?
            .profile_full()
    }

    fn ec_get_state(&self, id: &Identifier) -> JuizResult<Value> {
        Ok(jvalue!(juiz_lock(&self.worker().store().ecs.get(id)?)
            .with_context(|| format!(
                "locking ec(id={id:}) in CoreBroker::ec_get_state() function"
            ))?
            .get_state()?
            .to_string())
        .into())
    }

    fn ec_start(&mut self, id: &Identifier) -> JuizResult<Value> {
        Ok(juiz_lock(&self.worker().store().ecs.get(id)?)
            .with_context(|| {
                format!("locking ec(id={id:}) in CoreBroker::ec_get_state() function")
            })?
            .start()?
            .into())
    }

    fn ec_stop(&mut self, id: &Identifier) -> JuizResult<Value> {
        Ok(juiz_lock(&self.worker().store().ecs.get(id)?)
            .with_context(|| {
                format!("locking ec(id={id:}) in CoreBroker::ec_get_state() function")
            })?
            .stop()?
            .into())
    }

    fn ec_create(&mut self, manifest: &Value) -> JuizResult<Value> {
        let ec = self.worker_mut().create_ec_ref(manifest.clone())?;
        juiz_lock(&ec.clone())?.profile_full()
    }

    fn ec_destroy(&mut self, _identifier: &Identifier) -> JuizResult<Value> {
        todo!()
    }
}

impl ConnectionBrokerProxy for CoreBroker {
    fn connection_list(&self, recursive: bool) -> JuizResult<Vec<ConnectionIdentifier>> {
        log::trace!("connection_list(recursive={recursive}) called");
        let cons = self.worker().connection_profile_list()?;
        let mut ids_arr: Vec<ConnectionIdentifier> = cons
            .into_iter()
            .map(|con_prof| -> ConnectionIdentifier {
                Into::<ConnectionIdentifier>::into(con_prof)
            })
            .collect();
        if recursive {
            for subsystem_proxy in self.subsystem_proxies.iter() {
                let plist =
                    juiz_lock(&subsystem_proxy.broker_proxy())?.connection_list(recursive)?;
                for v in plist.into_iter() {
                    ids_arr.push(v);
                }
            }
        }
        Ok(ids_arr)
    }

    fn connection_profile_full(&self, id: &ConnectionIdentifier) -> JuizResult<ConnectionProfile> {
        self.worker().connection_profile(id, true)
    }

    fn connection_create(
        &mut self,
        manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile> {
        log::trace!("CoreBroker::connection_create({manifest}) called");
        self.worker_mut().create_connection(manifest)
    }

    fn connection_destroy(&mut self, id: &ConnectionIdentifier) -> JuizResult<ConnectionProfile> {
        self.worker_mut().destroy_connection(id)
    }
}

// fn check_if_both_side_is_on_same_host(source_id: Identifier, destination_id: Identifier) -> JuizResult<(Identifier, Identifier)> {
//     log::trace!("check_if_both_side_is_on_same_host({source_id}, {destination_id}) called");
//     let mut source_id_struct = IdentifierStruct::try_from(source_id)?;
//     let mut destination_id_struct = IdentifierStruct::try_from(destination_id)?;
//     if (source_id_struct.broker_name == destination_id_struct.broker_name) &&
//         (source_id_struct.broker_type_name == destination_id_struct.broker_type_name) {
//         source_id_struct.broker_name = "core".to_owned();
//         source_id_struct.broker_type_name = "core".to_owned();
//         destination_id_struct.broker_name = "core".to_owned();
//         destination_id_struct.broker_type_name = "core".to_owned();
//     }
//     Ok((source_id_struct.to_identifier(), destination_id_struct.to_identifier()))
// }

// fn check_connection_source_destination(manifest: &Value) -> JuizResult<(Identifier, Identifier)> {
//     let source = obj_get(manifest, "source")?;
//     let destination = obj_get(manifest, "destination")?;

//     let source_id_result = obj_get_str(source, "identifier");
//     let destination_id_result = obj_get_str(destination, "identifier");

//     // まずIDが両方ともあったら、brokerが同じものを指していたらcore/coreに直して接続する
//     if source_id_result.is_ok() && destination_id_result.is_ok() {
//         return check_if_both_side_is_on_same_host(source_id_result.unwrap().to_owned(), destination_id_result.unwrap().to_owned());
//     }

//     // IDがない場合はProcessかContainerProcessかが曖昧だが一旦Processで
//     return Ok((id_from_manifest(source)?, id_from_manifest(destination)?))
// }

impl BrokerProxy for CoreBroker {
    fn is_in_charge_for_process(&self, id: &Identifier) -> JuizResult<bool> {
        Ok(self.worker().store().processes.get(id).is_ok())
    }
}

unsafe impl Send for CoreBroker {}
