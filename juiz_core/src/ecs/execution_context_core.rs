

use std::sync::{Mutex, Arc, atomic::AtomicI64};



use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::prelude::*;


pub enum ExecutionContextState {
    STARTED = 1,
    STOPPED = 2,
    UNKNOWN = 99,
}

impl ExecutionContextState {

    pub fn to_i64(&self) -> i64 {
        match *self {
            ExecutionContextState::STARTED => {1},
            ExecutionContextState::STOPPED => {2},
            ExecutionContextState::UNKNOWN => {99}
        }
    }

    pub fn from(i: i64) -> Self {
        match i {
            1 => ExecutionContextState::STARTED,
            2 => ExecutionContextState::STOPPED,
            _ => ExecutionContextState::UNKNOWN,
        }
    }
}

impl ToString for ExecutionContextState {
    fn to_string(&self) -> String {
        match *self {
            ExecutionContextState::STARTED => {"STARTED".to_owned()},
            ExecutionContextState::STOPPED => {"STOPPED".to_owned()},
            ExecutionContextState::UNKNOWN => {"UNKNOWN".to_owned()}
        }
    }
}

pub struct ExecutionContextCore {
    target_processes: Vec<ProcessPtr>,
    pub state: AtomicI64,
}

impl ExecutionContextCore {

    pub fn new() -> Arc<Mutex<ExecutionContextCore>> {
        Arc::new(Mutex::new(ExecutionContextCore{
            target_processes: Vec::new(),
            state: AtomicI64::new(ExecutionContextState::STOPPED.to_i64()),
        }))
    }

    pub fn bind(&mut self, target_process: ProcessPtr) -> JuizResult<()> {
        self.target_processes.push(target_process);
        Ok(())
    }

    pub fn unbind(&mut self, target_process_id: ProcessIdentifier) -> JuizResult<()> {
        let mut remove_index: Option<usize> = None;
        for (i, p) in self.target_processes.iter().enumerate() {
            if target_process_id == p.identifier() {
                remove_index = Some(i);
            }
        }
        if let Some(index) = remove_index {
            let _pp = self.target_processes.remove(index);
        }
        Ok(())
    }


    pub fn get_state(&self) -> ExecutionContextState {
        ExecutionContextState::from(self.state.load(std::sync::atomic::Ordering::SeqCst))
    }

    ///
    /// 実行コンテキストの周期処理のコア部分。この中でターゲットプロセスすべてのexecuteを呼ぶ。
    pub fn svc(&self) -> JuizResult<Value> {
        for tp in self.target_processes.iter() {
            let _ = tp.lock()?.execute()?;
        }
        Ok(jvalue!({}))
    }

    pub fn profile(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "targets": self.target_processes.iter().map(|tp| { serde_json::to_value(tp.identifier()) }).collect::<serde_json::Result<Vec<Value>>>()?,
            "state": ExecutionContextState::from(self.state.load(std::sync::atomic::Ordering::SeqCst)).to_string()
        }))
    }
}

