use std::{sync::{Arc, Mutex, RwLock}, time::Duration};
use juiz_core::{env_logger, log, anyhow};
use juiz_core::{ExecutionContext, ExecutionContextCore, ExecutionContextFactory};

use juiz_core::prelude::*;
pub struct TimerEC {
    rate: f64,
    name: String,
    timeout: Duration, 
}

impl TimerEC {
    pub fn new(name: &str, rate: f64) -> Arc<RwLock<TimerEC>> {
        let rate_sec: u64 = rate.floor() as u64;
        let rate_nsec: u32 = ((rate - rate.floor()) * (1000_000_000.0)) as u32;
        let timeout = Duration::new(rate_sec, rate_nsec);
        Arc::new( RwLock::new(TimerEC{
            //thread_handle: None,
            //end_flag: Arc::new(Mutex::new(AtomicBool::from(false))),
            rate,
            name: name.to_string(),
            timeout,
        }))
    }
}

impl ExecutionContext for TimerEC {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn type_name(&self) -> &str {
        "TimerEC"
    }

    fn profile(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "rate": self.rate,
        }))
    }

    fn execute(&self, core: &Arc<Mutex<ExecutionContextCore>>) -> JuizResult<bool> {
        log::trace!("TimerExecutionContext.execute called");
        std::thread::sleep(self.timeout);
        match core.lock() {
            Err(e) => {
                log::error!("Error({e:?}) in Locking ECServiceFunction");
                return Err(anyhow::Error::from(JuizError::ExecutionContextCanNotLockStateError{}));
            },
            Ok(svc_func) => { 
                let _ = svc_func.svc().map_err(|e| -> () {log::error!("Error({e:?}) in Service function in ExecutionContext."); }); 
            }
        }
        return Ok(true);
    }
}

struct TimerECFactory {

}

impl ExecutionContextFactory for TimerECFactory {
    fn type_name(&self) -> &str {
        "TimerEC"
    }

    fn create(&self, manifest: Value) -> JuizResult<Arc<RwLock<dyn ExecutionContext>>> {
        let name = obj_get_str(&manifest, "name")?;
        let rate = obj_get_f64(&manifest, "rate")?;
        Ok(
            TimerEC::new(name, rate)
        )
    }
}

#[no_mangle]
pub unsafe extern "Rust" fn execution_context_factory() -> JuizResult<Arc<Mutex<dyn ExecutionContextFactory>>> {
    let _ = env_logger::try_init();
    Ok(Arc::new(Mutex::new(TimerECFactory{})))
}
