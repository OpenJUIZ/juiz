use std::{sync::{atomic::AtomicBool, Arc, Mutex}, time::Duration};

use tokio::runtime;

use juiz_sdk::anyhow;

use super::super::core_broker::CoreBrokerPtr;
use crate::{prelude::*};
use crate::brokers::Broker;
use std::sync::atomic::Ordering::SeqCst;

use super::super::crud_broker::CRUDBroker;

#[allow(dead_code)]
#[derive(Debug)]
pub struct MessengerBroker {
    core: ObjectCore, 
    thread_handle: Option<tokio::task::JoinHandle<()>>,
    end_flag: Arc<Mutex<AtomicBool>>,
    crud_broker: Arc<Mutex<CRUDBroker>>, 
    messenger_core: Arc<Mutex<dyn MessengerBrokerCore>>,
    tokio_runtime: Option<runtime::Runtime>,
}

pub trait MessengerBrokerCore : Send + std::fmt::Debug {
    fn receive_and_send(&self, timeout: Duration, func: Arc<Mutex<dyn Fn(CapsuleMap)->JuizResult<CapsulePtr >>>) -> JuizResult<Capsule>;
}

pub trait MessengerBrokerCoreFactory {
    fn create(&self, manifest: &Value) -> JuizResult<Arc<Mutex<dyn MessengerBrokerCore>>>;
}

impl MessengerBroker {

    pub fn new<'a>(impl_class_name: &'static str, type_name: &'a str, object_name: &'a str, core_broker: CoreBrokerPtr, messenger: Arc<Mutex<dyn MessengerBrokerCore>> ) -> JuizResult<Self>{
        let manifest = jvalue!({
            "class_name": impl_class_name.to_string(),
            "type_name": type_name,
            "name": object_name
        });
        Ok(MessengerBroker{
                core: ObjectCore::create(JuizObjectClass::Broker(impl_class_name), type_name, object_name),
                thread_handle: None,
                messenger_core: messenger,
                crud_broker: Arc::new(Mutex::new(CRUDBroker::new(core_broker.clone(), manifest)?)),
                end_flag: Arc::new(Mutex::new(AtomicBool::from(false))),
                //tokio_runtime: Some(runtime::Builder::new_multi_thread().enable_all().build().unwrap()),
                tokio_runtime: Some(tokio::runtime::Builder::new_multi_thread().thread_name("messenger_broker").worker_threads(4).enable_all().build().unwrap()), 
           })
    }
}

fn extract_method_name(args: & CapsuleMap) -> JuizResult<&String> {
    let err = |name: &str | anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError{ name: name.to_owned() });
    Ok(args.get_param("method_name").ok_or_else( || err("method_name") )?)
}

fn extract_class_name<'a>(args: &'a CapsuleMap) -> JuizResult<String> {
    // method_name, class_name, function_name, params
    let err = |name: &str | anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError{ name: name.to_owned() });
    let class_name = args.get_param("class_name").ok_or_else( || err("class_name") )?;
    Ok(class_name.to_owned())
}
fn extract_function_name<'a>(args: &'a CapsuleMap) -> JuizResult<&'a String> {
    let err = |name: &str | anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError{ name: name.to_owned() });
    let function_name = args.get_param("function_name").ok_or_else( || err("function_name") )?;
    Ok(function_name)
}


fn handle_function(crud_broker: Arc<Mutex<CRUDBroker>>, args: CapsuleMap) -> JuizResult<CapsulePtr> {
    log::info!("MessengerBroker::handle_function() called");
    let class_name = extract_class_name(&args)?;
    let function_name = extract_function_name(&args)?.to_owned();

    match extract_method_name(&args)?.as_str() {
        "CREATE" => juiz_lock(&crud_broker)?.create_class(class_name.as_str(), function_name.as_str(), args),
        "READ" =>  juiz_lock(&crud_broker)?.read_class(class_name.as_str(), function_name.as_str(), args),
        "UPDATE" =>  juiz_lock(&crud_broker)?.update_class(class_name.as_str(), function_name.as_str(), args),
        "DELETE" => juiz_lock(&crud_broker)?.delete_class(class_name.as_str(), function_name.as_str(), args),
        _ => {
            Err(anyhow::Error::from(JuizError::CRUDBRokerCanNotFindMethodError{method_name: "".to_owned()}))
        }
    }
}

impl JuizObjectCoreHolder for MessengerBroker {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for MessengerBroker {
}

impl Broker for MessengerBroker {

    fn start(&mut self) -> JuizResult<()> {
        let end_flag = Arc::clone(&self.end_flag);
        log::trace!("LocalBroker::start() called");
        let messenger_core = self.messenger_core.clone();

        let cb = self.crud_broker.clone();
        self.thread_handle = Some(self.tokio_runtime.as_mut().unwrap().spawn(
        async move {
                let timeout = Duration::new(0, 100*1000*1000);

                loop {
                    let crud = cb.clone();
                    let func: Arc<Mutex<dyn Fn(CapsuleMap)->JuizResult<CapsulePtr>>> = 
                        Arc::new(Mutex::new(move |value:CapsuleMap| -> JuizResult<CapsulePtr> {
                            handle_function(Arc::clone(&crud), value) 
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
        ));
        Ok(())
    }

    fn stop(&mut self) -> JuizResult<()> {
        log::debug!("LocalBroker::stop() called");
        match self.end_flag.lock() {
            Err(_) => {

            },
            Ok(f) => {
                f.swap(true, SeqCst);
            }
        };
        let _ = futures::executor::block_on(self.thread_handle.take().unwrap())?;
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