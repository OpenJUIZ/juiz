use futures::Future;
use std::sync::{Arc, Mutex};
use std::time::{self, Duration};

use super::super::core_broker::CoreBrokerPtr;
use crate::brokers::{Broker, CRUDBroker};
use crate::prelude::*;

use juiz_sdk::anyhow::anyhow;
use tokio::runtime;
#[allow(unused)]
pub struct CRUDBrokerHolder<F, Fut>
where
    F: Fn(Value, Arc<Mutex<CRUDBroker>>) -> Fut + Send + Sync + Copy + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    // core: ObjectCore,
    impl_class_name: String,
    type_name: String,
    manifest: Value,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    thread_handle: Option<tokio::task::JoinHandle<()>>,
    tokio_runtime: runtime::Runtime,
    on_start_function: F,
    identifier: String,
}

impl<F, Fut> std::fmt::Debug for CRUDBrokerHolder<F, Fut>
where
    F: Fn(Value, Arc<Mutex<CRUDBroker>>) -> Fut + Send + Sync + Copy + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CRUDBrokerHolder")
            .field("impl_class_name", &self.impl_class_name)
            .field("type_name", &self.type_name)
            .field("manifest", &self.manifest)
            .field("crud_broker", &self.crud_broker)
            .field("thread_handle", &self.thread_handle)
            .field("tokio_runtime", &self.tokio_runtime)
            // .field("on_start_function", &self.on_start_function)
            .field("identifier", &self.identifier)
            .finish()
    }
}

impl<F, Fut> CRUDBrokerHolder<F, Fut>
where
    F: Fn(Value, Arc<Mutex<CRUDBroker>>) -> Fut + Send + Sync + Copy + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    pub fn new(
        impl_class_name: &'static str,
        type_name: &'static str,
        core_broker: CoreBrokerPtr,
        on_start_function: F,
        manifest: Value,
    ) -> JuizResult<Self> {
        // let object_name = obj_get_str(&manifest, "name")?;
        Ok(CRUDBrokerHolder {
            impl_class_name: impl_class_name.to_owned(),
            type_name: type_name.to_owned(),
            identifier: "".to_owned(),
            // core: ObjectCore::create(JuizObjectClass::Broker(impl_class_name), type_name, object_name),
            crud_broker: Arc::new(Mutex::new(CRUDBroker::new(core_broker, manifest.clone())?)),
            thread_handle: None,
            tokio_runtime: runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            on_start_function,
            manifest,
        })
    }

    // fn profile(&self) -> JuizResult<Value> {
    //     log::trace!("CRUDBrokerHolder::profile_full() called");
    //     let name = self.crud_broker.lock().unwrap().name();
    //     let identifier = self.crud_broker.lock().unwrap().identifier();
    //     Ok(jvalue!({
    //         "identifier": identifier,
    //         "class_name": self.class_name().as_str(),
    //         "type_name": self.type_name(),
    //         "name": name,
    //     })
    //     .into())
    // }
}

impl<F, Fut> Broker for CRUDBrokerHolder<F, Fut>
where
    F: Fn(Value, Arc<Mutex<CRUDBroker>>) -> Fut + Send + Sync + Copy + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn start(&mut self) -> JuizResult<()> {
        let type_name = self.identifier().type_name;
        log::trace!("CRUDBrokerHolder::start(type_name={type_name}) called");

        let crud = self.crud_broker.clone();

        let manifest = self.crud_broker.lock().unwrap().manifest();
        let on_start = self.on_start_function; //.take().unwrap();
        self.thread_handle = Some(self.tokio_runtime.spawn(async move {
            on_start(manifest, crud).await;
        }));
        log::trace!("CRUDBrokerHolder::start(type_name={type_name}) exit");
        Ok(())
    }

    fn wait_until_started(&mut self, timeout: Duration) -> JuizResult<()> {
        let start_time = time::Instant::now();
        loop {
            match self.crud_broker.lock() {
                Ok(crud) => {
                    if crud.is_started() {
                        return Ok(());
                    }

                    let duration = start_time.elapsed();
                    if duration > timeout {
                        log::error!("wait_until_started failed. Timeout");
                        return Err(anyhow!(JuizError::TimeoutError {}));
                    }
                }
                Err(e) => {
                    panic!("Error lock is poisoned (Error{e:?})")
                }
            }
        }
    }

    fn stop(&mut self) -> JuizResult<()> {
        let type_name = self.identifier().type_name.clone();
        log::trace!("CRUDBrokerHolder::stop(type_name={type_name}) called");
        self.thread_handle.take().unwrap().abort();
        log::trace!("CRUDBrokerHolder::stop(type_name={type_name}) exit");
        Ok(())
    }

    fn identifier(&self) -> BrokerIdentifier {
        log::trace!("CRUDBrokerHolder::identifier() called");
        self.crud_broker.lock().unwrap().identifier()
    }

    fn profile(&self) -> BrokerProfile {
        log::trace!("CRUDBrokerHolder::profile() called");
        self.crud_broker.lock().unwrap().profile()
    }
}
