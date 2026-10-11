use std::sync::{Arc, Mutex, RwLock};

use juiz_core::{prelude::*, ExecutionContextFactory};

use juiz_core::{ExecutionContext, ExecutionContextCore};

pub struct OneShotEC {
    name: String,
}

impl OneShotEC {
    pub fn new(name: &str) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            name: name.to_string(),
        }))
    }
}

impl ExecutionContext for OneShotEC {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn type_name(&self) -> &str {
        "OneShotEC"
    }

    fn profile(&self) -> JuizResult<Value> {
        return Ok(jvalue!({"name": self.name}));
    }

    fn execute(&self, core: &Arc<Mutex<ExecutionContextCore>>) -> JuizResult<bool> {
        juiz_lock(&core)?.svc().and(Ok(false))
    }
}

struct OneShotECFactory {}

impl ExecutionContextFactory for OneShotECFactory {
    fn type_name(&self) -> &str {
        "one_shot_ec"
    }

    fn create(&self, manifest: Value) -> JuizResult<Arc<RwLock<dyn ExecutionContext>>> {
        let name = obj_get_str(&manifest, "name")?;
        Ok(OneShotEC::new(name))
    }
}

#[no_mangle]
pub unsafe extern "Rust" fn execution_context_factory(
) -> JuizResult<Arc<Mutex<dyn ExecutionContextFactory>>> {
    env_logger::init();
    Ok(Arc::new(Mutex::new(OneShotECFactory {})))
}
