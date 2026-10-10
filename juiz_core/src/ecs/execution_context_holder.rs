use std::sync::{Mutex, Arc, RwLock, atomic::AtomicBool};
use juiz_sdk::anyhow;


use juiz_sdk::process_identifier::ProcessIdentifier;
use tokio::runtime;

use crate::prelude::*;
use crate::{ecs::execution_context_core::ExecutionContextState};

use super::{execution_context::ExecutionContext, execution_context_core::ExecutionContextCore, execution_context_function::ExecutionContextFunction};

pub struct ExecutionContextHolder{
    object_core: ObjectCore,
    core: Arc<Mutex<ExecutionContextCore>>,
    execution_context: Arc<RwLock<dyn ExecutionContext>>,
    thread_handle: Option<tokio::task::JoinHandle<JuizResult<()>>>,
    tokio_runtime: runtime::Runtime,
    end_flag: Arc<Mutex<AtomicBool>>,
    auto_start: bool,
}

impl ExecutionContextHolder {

    pub fn new(type_name: &str, ec: Arc<RwLock<dyn ExecutionContext>>, auto_start: bool) -> JuizResult<Arc<Mutex<ExecutionContextHolder>>> {
        Ok(Arc::new(Mutex::new(
            ExecutionContextHolder { 
                object_core: ObjectCore::create(JuizObjectClass::ExecutionContext("ExecutionContext"), 
                    type_name, 
                    juiz_borrow(&ec)?.name()), 
                core: ExecutionContextCore::new(), 
                execution_context: ec.clone(),
                tokio_runtime: runtime::Builder::new_multi_thread().thread_name("execution_context_holder").worker_threads(4).enable_all().build().unwrap(),
                thread_handle: None,
                end_flag: Arc::new(Mutex::new(AtomicBool::from(false))),
                auto_start,
             }
        )))
    }


    
    
    fn is_periodic(&self) -> JuizResult<bool> {
        let flag = match self.execution_context.read() {
            Ok(ec) => Ok(ec.is_periodic()),
            Err(_e) => Err(anyhow::Error::from(JuizError::ObjectLockError{target: "ExecutionContext".to_owned()}))
        }?;
        Ok(flag)
    }

    fn start_oneshot(&mut self) -> JuizResult<Value> {
        let type_name = self.type_name().to_string();
        log::trace!("ExecutionContextHolder::start(type_name={type_name}) called");

        let _  = juiz_borrow_mut(&mut self.execution_context)?.on_starting(&self.core)?;
        let _  = self.core.lock().and_then(|s| {
            s.state.store(ExecutionContextState::STARTED.to_i64(), std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }).or_else(|_e| {
            Err(anyhow::Error::from(JuizError::ObjectLockError { target: "ExecutionContextCore".to_owned() }))
        })?;
        Ok(jvalue!({}))
    }

    fn start_periodic(&mut self) -> JuizResult<Value> {
        log::trace!("ExecutionContextHolder::start(type_name={:}) called", self.type_name());
        {
            juiz_lock(&self.end_flag)?.swap(false, std::sync::atomic::Ordering::SeqCst);
        }        
        let core = self.core.clone();
        let mut ec = self.execution_context.clone();

        let end_flag = Arc::clone(&self.end_flag);

        self.thread_handle = Some(self.tokio_runtime.spawn_blocking(
            move || -> JuizResult<()> {

                juiz_borrow_mut(&mut ec)?.on_starting(&core)?;
                core.lock().or_else(|e|{Err(Into::<JuizError>::into(e))})?.state.store(ExecutionContextState::STARTED.to_i64(), std::sync::atomic::Ordering::SeqCst);

                loop {
                    if end_flag.lock().or_else(|e|{Err(Into::<JuizError>::into(e))})?
                        .load(std::sync::atomic::Ordering::SeqCst) {
                            log::debug!("Detect end_flag is raised in ExecutionContextHodler::routine()");
                            break;
                    }
                    if !juiz_borrow(&ec)?.execute(&core)? {
                        break;
                    }
                }

                juiz_borrow_mut(&mut ec)?.on_stopping(&core)?;
                core.lock().or_else(|e|{Err(Into::<JuizError>::into(e))})?
                    .state.store(ExecutionContextState::STOPPED.to_i64(), std::sync::atomic::Ordering::SeqCst);
                
                Ok(())
            }
        ));
        log::trace!("ExecutionContextHolder::start(type_name={:}) exit", self.type_name());
        Ok(jvalue!({}))
    }

    fn stop_periodic(&mut self) -> JuizResult<Value> {
        log::info!("ExecutionContextHolder::stop() called");
        if self.thread_handle.is_some() {
            juiz_lock(&self.end_flag)?.swap(true, std::sync::atomic::Ordering::SeqCst);
            let _ = futures::executor::block_on(self.thread_handle.take().unwrap())?;
            self.thread_handle = None;
        }
        Ok(jvalue!({}))
    }

    fn stop_oneshot(&mut self) -> JuizResult<Value> {
        let _ = juiz_borrow_mut(&mut self.execution_context)?.on_stopping(&self.core)?; 
        self.core.lock().and_then(|c| {
            c.state.store(ExecutionContextState::STOPPED.to_i64(), std::sync::atomic::Ordering::SeqCst);
            Ok(jvalue!({}))
        }).or_else(|e| {
            log::error!("ExecutionContextHolder.routine() error: {e:?}");
            Err(anyhow::Error::from(JuizError::ObjectLockError { target: "ExecutionContext".to_owned() }))
        })
    }


    
    // pub fn identifier(&self) -> &Identifier {
    //     self.object_core.identifier()
    // }

}

impl JuizObjectCoreHolder for ExecutionContextHolder {
    fn core(&self) -> &ObjectCore {
        &self.object_core
    }
}

impl JuizObject for ExecutionContextHolder {

    fn profile_full(&self) -> JuizResult<Value> {
        log::trace!("ExecutionContextHolder()::profile_full() called");
        let mut v = self.object_core.profile_full()?;
        let ecv = juiz_borrow(&self.execution_context)?.profile()?;
        obj_merge_mut(&mut v, &ecv)?;

        let cv = juiz_lock(&self.core)?.profile()?;
        obj_merge_mut(&mut v, &cv)?;
        obj_merge_mut(&mut v, &jvalue!({
            "auto_start": self.auto_start
        }))?;
        Ok(v.into())
    }
}


impl ExecutionContextFunction for ExecutionContextHolder {

    fn start(&mut self) -> JuizResult<Value> { 
        if self.is_periodic()? {
            return self.start_periodic();
        } else {
            return self.start_oneshot();
        }
    }
    
    fn stop(&mut self) -> JuizResult<Value> { 
        if self.is_periodic()? {
            return self.stop_periodic();
        } else {
            return self.stop_oneshot();
        }
    }

    fn get_state(&self) -> JuizResult<ExecutionContextState> {
        let s = juiz_lock(&self.core)?.state.load(std::sync::atomic::Ordering::SeqCst);
        Ok(ExecutionContextState::from(s))
    }

    fn bind(&mut self, target_process: ProcessPtr) -> JuizResult<()> {
        juiz_lock(&self.core)?.bind(target_process)
    }

    fn unbind(&mut self, target_process_id: ProcessIdentifier) -> JuizResult<()> {
        juiz_lock(&self.core)?.unbind(target_process_id)
    }

    fn on_load(&mut self, system: &mut System) -> () {
        match self.execution_context.write() {
            Ok(mut v) => {
                v.on_load(system, self.core.clone());
            },
            Err(e) => {
                log::error!("ExecutionContextHolder::on_load() failed. RwLock.write() failed. {:?}", e);
            }
        }
    }

}