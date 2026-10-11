use super::execution_context::ExecutionContext;
use super::execution_context_core::ExecutionContextCore;
use crate::prelude::*;
use std::sync::{Arc, Mutex};

pub struct OneShotEC {
    name: String,
}

impl OneShotEC {
    //pub fn new(name: &str) -> Arc<Mutex<Self>> {
    //    Arc::new(Mutex::new(Self{
    //        name: name.to_string(),
    //    }))
    //}
}

impl ExecutionContext for OneShotEC {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn type_name(&self) -> &str {
        "OneShotEC"
    }

    fn profile(&self) -> JuizResult<Value> {
        todo!()
    }

    fn execute(&self, core: &Arc<Mutex<ExecutionContextCore>>) -> JuizResult<bool> {
        let _ = juiz_lock(core)?.svc().and(Ok(()))?;
        return Ok(false);
    }
}
