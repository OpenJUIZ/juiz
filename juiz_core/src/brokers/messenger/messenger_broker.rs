use std::{
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

use juiz_sdk::anyhow;

use super::super::core_broker::CoreBrokerPtr;
use crate::brokers::Broker;
use crate::prelude::*;
use std::sync::atomic::Ordering::SeqCst;

use super::super::crud_broker::CRUDBroker;

#[allow(dead_code)]
#[derive(Debug)]
pub struct MessengerBroker {
    core: ObjectCore,
    thread_handle: Option<std::thread::JoinHandle<()>>,
    end_flag: Arc<Mutex<AtomicBool>>,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    messenger_core: Arc<Mutex<dyn MessengerBrokerCore>>,
}

/// サーバ側の処理が失敗したときの応答に付ける function_name。
///
/// エラーでも応答を返さないとクライアントはタイムアウトまで待つことになるので、
/// エラーの内容をこの名前の応答で返し、MessengerBrokerProxy で JuizError::BrokerProxyRemoteError に戻す (issue #231)
pub const ERROR_RESPONSE_FUNCTION_NAME: &str = "__error__";

pub trait MessengerBrokerCore: Send + std::fmt::Debug {
    fn receive_and_send(
        &self,
        timeout: Duration,
        func: Arc<Mutex<dyn Fn(CapsuleMap) -> JuizResult<CapsulePtr>>>,
    ) -> JuizResult<Capsule>;
}

pub trait MessengerBrokerCoreFactory {
    fn create(&self, manifest: &Value) -> JuizResult<Arc<Mutex<dyn MessengerBrokerCore>>>;
}

impl MessengerBroker {
    pub fn new<'a>(
        impl_class_name: &'static str,
        type_name: &'a str,
        object_name: &'a str,
        core_broker: CoreBrokerPtr,
        messenger: Arc<Mutex<dyn MessengerBrokerCore>>,
    ) -> JuizResult<Self> {
        let manifest = jvalue!({
            "class_name": impl_class_name.to_string(),
            "type_name": type_name,
            "name": object_name
        });
        Ok(MessengerBroker {
            core: ObjectCore::create(
                JuizObjectClass::Broker(impl_class_name),
                type_name,
                object_name,
            ),
            thread_handle: None,
            messenger_core: messenger,
            crud_broker: Arc::new(Mutex::new(CRUDBroker::new(core_broker.clone(), manifest)?)),
            end_flag: Arc::new(Mutex::new(AtomicBool::from(false))),
        })
    }
}

fn extract_method_name(args: &CapsuleMap) -> JuizResult<&String> {
    let err = |name: &str| {
        anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError {
            name: name.to_owned(),
        })
    };
    Ok(args
        .get_param("method_name")
        .ok_or_else(|| err("method_name"))?)
}

fn extract_class_name<'a>(args: &'a CapsuleMap) -> JuizResult<String> {
    // method_name, class_name, function_name, params
    let err = |name: &str| {
        anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError {
            name: name.to_owned(),
        })
    };
    let class_name = args
        .get_param("class_name")
        .ok_or_else(|| err("class_name"))?;
    Ok(class_name.to_owned())
}
fn extract_function_name<'a>(args: &'a CapsuleMap) -> JuizResult<&'a String> {
    let err = |name: &str| {
        anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError {
            name: name.to_owned(),
        })
    };
    let function_name = args
        .get_param("function_name")
        .ok_or_else(|| err("function_name"))?;
    Ok(function_name)
}

fn handle_function(
    crud_broker: Arc<Mutex<CRUDBroker>>,
    args: CapsuleMap,
) -> JuizResult<CapsulePtr> {
    log::info!("MessengerBroker::handle_function() called");
    let class_name = extract_class_name(&args)?;
    let function_name = extract_function_name(&args)?.to_owned();

    // ロックは dispatcher を取り出す間だけ握り、Process の実行中は外しておく (issue #356)
    let dispatcher = juiz_lock(&crud_broker)?.dispatcher();
    match extract_method_name(&args)?.as_str() {
        "CREATE" => {
            dispatcher.create_class(class_name.as_str(), function_name.as_str(), args)
        }
        "READ" => {
            dispatcher.read_class(class_name.as_str(), function_name.as_str(), args)
        }
        "UPDATE" => {
            dispatcher.update_class(class_name.as_str(), function_name.as_str(), args)
        }
        "DELETE" => {
            dispatcher.delete_class(class_name.as_str(), function_name.as_str(), args)
        }
        _ => Err(anyhow::Error::from(
            JuizError::CRUDBRokerCanNotFindMethodError {
                method_name: "".to_owned(),
            },
        )),
    }
}

/// handle_function を呼び、失敗したらエラーの内容を応答のカプセルにして返す
fn handle_function_or_error_response(
    crud_broker: Arc<Mutex<CRUDBroker>>,
    args: CapsuleMap,
) -> JuizResult<CapsulePtr> {
    let class_name = args.get_param("class_name").cloned().unwrap_or_default();
    let function_name = args.get_param("function_name").cloned().unwrap_or_default();
    handle_function(crud_broker, args)
    .and_then(|mut capsule| {
        // MessengerBrokerProxy が応答の対応を確かめるための印。受け取った側で外す (issue #277)
        capsule
            .set_option("function_name", function_name.as_str())?
            .set_option("class_name", class_name.as_str())?;
        Ok(capsule)
    })
    .or_else(|e| {
        log::error!(
            "MessengerBroker failed to handle {class_name}/{function_name}. Error is {e:#}"
        );
        let mut capsule: CapsulePtr = jvalue!({"message": format!("{e:#}")}).into();
        capsule
            .set_option("function_name", ERROR_RESPONSE_FUNCTION_NAME)?
            .set_option("class_name", class_name.as_str())?;
        Ok(capsule)
    })
}

impl JuizObjectCoreHolder for MessengerBroker {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for MessengerBroker {}

impl Broker for MessengerBroker {
    fn start(&mut self) -> JuizResult<()> {
        let end_flag = Arc::clone(&self.end_flag);
        log::trace!("LocalBroker::start() called");
        let messenger_core = self.messenger_core.clone();

        let cb = self.crud_broker.clone();
        // 受信待ちも処理もブロッキングなので、tokio のタスクではなく普通のスレッドで回す。
        // tokio のタスク内だと、ec_destroy などで処理中にランタイムを drop したときに panic する (issue #231)
        self.thread_handle = Some(std::thread::Builder::new().name("messenger_broker".to_owned()).spawn(
        move || {
                let timeout = Duration::new(0, 100*1000*1000);

                loop {
                    let crud = cb.clone();
                    let func: Arc<Mutex<dyn Fn(CapsuleMap)->JuizResult<CapsulePtr>>> =
                        Arc::new(Mutex::new(move |value:CapsuleMap| -> JuizResult<CapsulePtr> {
                            handle_function_or_error_response(Arc::clone(&crud), value)
                        }));

                    std::thread::sleep(Duration::new(0, 10 * 1000));
                    match end_flag.lock() {
                        Err(e) => {
                            log::error!("Error({e:?}) in LocalBroker::routine()");
                            continue
                        },
                        Ok(f) => {
                            if f.load(SeqCst) {
                                log::debug!("Detect end_flag is raised in LocalBroker::routine()");
                                break;
                            }
                        }
                    };
                    //sender_receiver.get_mut()
                    //match juiz_lock(&sender_receiver) {
                    match messenger_core.lock() {
                        Err(_) => {},
                        Ok(sndr_recvr) => {
                            // log::trace!("In MessengerBroker::routine(), calling sndr_recvr.receive_and_send() funciton.");
                            match sndr_recvr.receive_and_send(timeout, func) {
                                    Err(e) => {
                                        log::error!("Error. Core.receive_and_send failed. in MessengerBroker::routine(). Error is {}", e);
                                        //log::trace!("In MessengerBroker::routine(), sndr_recvr.receive_and_send() exit.");
                                    }, Ok(_) => {
                                        //log::trace!("In MessengerBroker::routine(), sndr_recvr.receive_and_send() exit.");
                                    }
                            }
                        }
                    }
                }
                log::debug!("LocalBroker::routine() end!!!");
            }
        )?);
        Ok(())
    }

    fn stop(&mut self) -> JuizResult<()> {
        log::debug!("LocalBroker::stop() called");
        match self.end_flag.lock() {
            Err(_) => {}
            Ok(f) => {
                f.swap(true, SeqCst);
            }
        };
        if let Some(handle) = self.thread_handle.take() {
            handle.join().map_err(|_| {
                anyhow::Error::from(JuizError::BrokerStopFailedError {
                    type_name: self.type_name().to_owned(),
                })
            })?;
        }
        log::debug!("LocalBroker stopped.");
        Ok(())
    }

    fn wait_until_started(&mut self, _timeout: Duration) -> JuizResult<()> {
        Ok(())
    }

    fn identifier(&self) -> BrokerIdentifier {
        self.core.identifier().try_into().unwrap()
    }

    fn profile(&self) -> BrokerProfile {
        self.core.profile_full().unwrap().try_into().unwrap()
    }
}
