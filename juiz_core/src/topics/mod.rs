use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{
    connections::ConnectionFactoryImpl, core::SubSystemProxy, prelude::*, processes::ProcessImpl,
};
use juiz_sdk::{anyhow::anyhow, identifier::split_namespace, types::TypeSchema};
use uuid::Uuid;

pub mod shared_memory;
pub use shared_memory::{SharedMemoryConfig, SharedMemoryStats, TopicSharedMemory};

pub type TopicName = String;

#[derive(Clone)]
#[allow(unused)]
pub struct Topic {
    name: TopicName,
    subsystem_proxies: Vec<SubSystemProxy>,
    /// 型付きの publish 側 (Process の ID → 戻り値の型)。接続時の型検査に使う (issue #193)
    typed_publishers: HashMap<String, TypeSchema>,
    /// 型付きの subscribe 側 ((Process の ID, 引数名) → 引数の型)
    typed_subscribers: HashMap<(String, String), TypeSchema>,
    /// 共有メモリ (B) の設定とリング。conf の topics: に shared_memory があるときだけ (docs/spec/large-data.md §6)
    shared_memory: Option<Arc<TopicSharedMemory>>,
}

#[allow(unused)]
impl Topic {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            subsystem_proxies: Vec::new(),
            typed_publishers: HashMap::new(),
            typed_subscribers: HashMap::new(),
            shared_memory: None,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(Clone)]
#[allow(unused)]
pub struct TopicPtr {
    name: String,
    system_uuid: Uuid,
    topic: Arc<RwLock<Topic>>,
    ptr: ProcessPtr,
    /// 共有メモリの数え上げ。送り手 (Topic) と受け手 (IPC ブローカー) が足す
    shm_stats: Arc<SharedMemoryStats>,
}

/// Topic は Value・Image・Native を通す。Empty はエラー (docs/spec/large-data.md §5)
fn check_topic_capsule(v: &CapsulePtr) -> JuizResult<()> {
    if v.is_empty()? {
        return Err(anyhow!(JuizError::ArgumentError {
            message: "Topic can not pass an empty value".to_owned()
        }));
    }
    Ok(())
}

impl TopicPtr {
    pub fn new(name: &str, system_uuid: Uuid) -> Self {
        log::trace!("new(name={name}, uuid={system_uuid}) called");
        // name はネームスペースを含めた完全な名前。Topic の Process の ID もネームスペース付きにする (issue #149)
        let (namespace, short_name) = split_namespace(name);
        let manifest = ProcessManifest::new("topic")
            .namespace(namespace.as_str())
            .name(short_name.as_str())
            .use_memo(false)
            .add_object_arg("input", "", jvalue!({}));

        let topic = Arc::new(RwLock::new(Topic::new(name)));
        let my_topic_name = name.to_owned();
        let my_topic = topic.clone();
        let my_uuid = system_uuid.clone();
        // 受け取った CapsulePtr を、中身を複製せずに転送先のブローカーと購読者に渡す (docs/spec/large-data.md §5)
        let topic_func = move |mut arg: CapsuleMap| -> JuizResult<CapsulePtr> {
            log::trace!("Topic ({my_topic_name}) / topic_func called");
            let v = arg.take("input")?;
            check_topic_capsule(&v)?;
            match my_topic.read() {
                Ok(t) => {
                    log::trace!(" - my_topic.read() OK.");
                    // 共有メモリ (B) に書くのは、最初に IPC の転送先が見つかったときに 1 回だけ。
                    // None はまだ書いていない、Some(None) は B で送らない (A で送る)
                    #[cfg(feature = "ipc-broker")]
                    let mut shared: Option<Option<shared_memory::SharedPush>> = None;
                    // 送れない転送先(止まったサブシステムなど)があっても、残りの転送先には送る (issue #67)
                    for subsystem in t.subsystem_proxies.iter() {
                        log::trace!("- broker_proxy: subsystem={:?}", subsystem.uuid());
                        let r = juiz_lock(&subsystem.broker_proxy()).and_then(|bp| {
                            #[cfg(feature = "ipc-broker")]
                            if let Some(shm) = t.shared_memory.as_ref() {
                                if bp.supports_shared_memory() {
                                    if shared.is_none() {
                                        shared = Some(shm.prepare(&my_topic_name, &v).unwrap_or_else(|e| {
                                            log::error!("Topic({my_topic_name}): writing to shared memory failed. {e:#}");
                                            None
                                        }));
                                    }
                                    if let Some(Some(push)) = shared.as_ref() {
                                        return bp.topic_push_shared(my_topic_name.as_str(), push, Some(my_uuid));
                                    }
                                }
                            }
                            bp.topic_push(my_topic_name.as_str(), v.clone(), Some(my_uuid))
                        });
                        match r {
                            Ok(_) => log::trace!("SubsystemProxy.topic_push() success"),
                            Err(e) => log::error!(
                                "Error {e} occurred in SubsystemProxy(uuid={}).topic_push()",
                                subsystem.uuid()
                            ),
                        }
                    }
                }
                Err(_) => {
                    log::error!("my_topic.read() failed.");
                    return Err(anyhow!(JuizError::ObjectLockError {
                        target: "Topic".to_owned()
                    }));
                }
            }
            log::trace!("Topic ({my_topic_name}) / topic_func exit");
            Ok(v)
        };
        TopicPtr {
            name: name.to_owned(),
            system_uuid,
            topic,
            shm_stats: Arc::new(SharedMemoryStats::default()),
            ptr: ProcessPtr::new(
                //ProcessImpl::clousure_new_with_class_name(JuizObjectClass::Topic("Topic"), manifest, Box::new(topic_func)).unwrap()))
                ProcessImpl::new_passthrough_with_class_name(
                    JuizObjectClass::Topic("Topic"),
                    manifest,
                    topic_func,
                    Box::new(ConnectionFactoryImpl::new()),
                )
                .unwrap(),
            ),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn process_ptr(&self) -> ProcessPtr {
        self.ptr.clone()
    }

    pub fn profile(&self) -> JuizResult<Value> {
        let mut v = crate::core::process_profile_value(&self.ptr.lock()?.profile()?)?;
        if let (Some(shm), Some(obj)) = (self.shm_stats.profile(), v.as_object_mut()) {
            obj.insert("shared_memory".to_owned(), shm);
        }
        Ok(v)
    }

    /// 共有メモリ (B) の設定を当てる。None なら使わない
    pub fn set_shared_memory(&self, config: Option<SharedMemoryConfig>) -> JuizResult<()> {
        let shm = config.map(|c| Arc::new(TopicSharedMemory::new(c, self.shm_stats.clone())));
        self.write_topic()?.shared_memory = shm;
        Ok(())
    }

    pub fn shared_memory_stats(&self) -> Arc<SharedMemoryStats> {
        self.shm_stats.clone()
    }

    pub fn push(&self, capsule: CapsulePtr, pushed_system_uuid: Option<Uuid>) -> JuizResult<()> {
        log::trace!("push(uuid={pushed_system_uuid:?}) called");
        let r = self
            .ptr
            .lock()?
            .push_by("input", capsule)
            .and_then(|_| Ok(()));
        log::trace!("push(uuid={pushed_system_uuid:?}) exit");
        r
    }

    /// publish を始める Process の戻り値の型 (writer) を、今つながっている subscribe 側の引数の型 (reader)
    /// それぞれと比べる。互換でない組があれば `TypeIncompatibleError` (仕様 docs/spec/datatype.md §8、issue #193)
    pub fn check_publisher(&self, schema: Option<&TypeSchema>) -> JuizResult<()> {
        let Some(writer) = schema else {
            return Ok(());
        };
        let subscribers: Vec<(String, String)> = self
            .ptr
            .lock()?
            .destination_connections()?
            .iter()
            .map(|c| {
                let p = c.profile();
                (p.destination_process_id.to_string(), p.arg_name)
            })
            .collect();
        let t = self.read_topic()?;
        for key in subscribers.iter() {
            if let Some(reader) = t.typed_subscribers.get(key) {
                crate::types::check_connectable(Some(writer), Some(reader))
                    .map_err(|e| self.topic_type_error("publish", &key.0, e))?;
            }
        }
        Ok(())
    }

    /// subscribe を始める引数の型 (reader) を、今つながっている publish 側の戻り値の型 (writer) それぞれと比べる
    pub fn check_subscriber(&self, schema: Option<&TypeSchema>) -> JuizResult<()> {
        let Some(reader) = schema else {
            return Ok(());
        };
        let publishers: Vec<String> = self
            .ptr
            .lock()?
            .source_connections()?
            .iter()
            .map(|c| c.profile().source_process_id.to_string())
            .collect();
        let t = self.read_topic()?;
        for id in publishers.iter() {
            if let Some(writer) = t.typed_publishers.get(id) {
                crate::types::check_connectable(Some(writer), Some(reader))
                    .map_err(|e| self.topic_type_error("subscribe", id, e))?;
            }
        }
        Ok(())
    }

    /// publish 側の型を覚える。型なしなら覚えない (つながっていない Process の分は検査で使わない)
    pub fn set_publisher_type(
        &self,
        process_id: &str,
        schema: Option<TypeSchema>,
    ) -> JuizResult<()> {
        let mut t = self.write_topic()?;
        match schema {
            Some(s) => t.typed_publishers.insert(process_id.to_owned(), s),
            None => t.typed_publishers.remove(process_id),
        };
        Ok(())
    }

    pub fn set_subscriber_type(
        &self,
        process_id: &str,
        arg_name: &str,
        schema: Option<TypeSchema>,
    ) -> JuizResult<()> {
        let mut t = self.write_topic()?;
        let key = (process_id.to_owned(), arg_name.to_owned());
        match schema {
            Some(s) => t.typed_subscribers.insert(key, s),
            None => t.typed_subscribers.remove(&key),
        };
        Ok(())
    }

    fn topic_type_error(
        &self,
        action: &str,
        other: &str,
        e: juiz_sdk::anyhow::Error,
    ) -> juiz_sdk::anyhow::Error {
        log::error!(
            "Topic({}): can not {action} because the type does not match {other} ({e})",
            self.name
        );
        e
    }

    fn read_topic(&self) -> JuizResult<std::sync::RwLockReadGuard<'_, Topic>> {
        self.topic.read().map_err(|_| {
            anyhow!(JuizError::ObjectLockError {
                target: "Topic".to_owned()
            })
        })
    }

    fn write_topic(&self) -> JuizResult<std::sync::RwLockWriteGuard<'_, Topic>> {
        self.topic.write().map_err(|_| {
            anyhow!(JuizError::ObjectLockError {
                target: "Topic".to_owned()
            })
        })
    }

    pub fn num_local_publishers(&self) -> JuizResult<usize> {
        Ok(self.ptr.lock()?.source_connections()?.len())
    }

    pub fn num_local_subscribers(&self) -> JuizResult<usize> {
        Ok(self.ptr.lock()?.destination_connections()?.len())
    }

    /// このTopicの値を転送する先のサブシステムの数
    pub fn num_subscriber_subsystems(&self) -> JuizResult<usize> {
        match self.topic.read() {
            Ok(t) => Ok(t.subsystem_proxies.len()),
            Err(_e) => Err(anyhow!(JuizError::ObjectLockError {
                target: "Topic".to_owned()
            })),
        }
    }

    pub fn register_subscriber_subsystem(&self, subsystem_proxy: SubSystemProxy) -> JuizResult<()> {
        log::trace!(
            "register_subscriber_subsystem(name={}, subsystem_proxy={}) called",
            self.name(),
            subsystem_proxy.uuid()
        );
        match self.topic.write() {
            Ok(mut t) => {
                // 同じ UUID、または同じアドレスで登録済みの転送先は置き換える。
                // 同じアドレスで UUID が違うのは、止まって再起動する前の古いサブシステム (issue #67)
                let new_bp_id = juiz_lock(&subsystem_proxy.broker_proxy())?.identifier();
                t.subsystem_proxies.retain(|ssp| {
                    let same = ssp.uuid() == subsystem_proxy.uuid()
                        || juiz_lock(&ssp.broker_proxy())
                            .map(|b| b.identifier() == new_bp_id)
                            .unwrap_or(false);
                    if same {
                        log::debug!(
                            "Topic({}): subscriber Subsystem(uuid={}) is replaced by Subsystem(uuid={}).",
                            self.name,
                            ssp.uuid(),
                            subsystem_proxy.uuid()
                        );
                    }
                    !same
                });
                t.subsystem_proxies.push(subsystem_proxy);
                Ok(())
            }
            Err(_e) => Err(anyhow!(JuizError::ObjectLockError {
                target: "Topic".to_owned()
            })),
        }
    }

    /*
    pub fn register_publisher_subsystem(&self, subsystem_proxy: SubSystemProxy) -> JuizResult<()> {
        log::trace!("register_publisher_subsystem(name={}, subsystem_proxy={}) called", self.name(), subsystem_proxy.uuid());
        match self.topic.write() {
            Ok(mut t) => {
                for subsystem in t.subsystem_proxies.iter() {
                    if subsystem.uuid() == subsystem_proxy.uuid() { // もうすでにpublish登録している
                        return Ok(())
                    }
                }
                t.subsystem_proxies.push(subsystem_proxy);
                Ok(())
            },
            Err(_e) => {
                Err(anyhow!(JuizError::ObjectLockError { target: "Topic".to_owned() }))
            }
        }
    }
    */
}
