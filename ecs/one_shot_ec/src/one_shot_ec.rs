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
    let _ = env_logger::try_init();
    Ok(Arc::new(Mutex::new(OneShotECFactory {})))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use juiz_core::anyhow;

    static ONCE_COUNT: AtomicI64 = AtomicI64::new(0);

    fn count_once(_v: CapsuleMap) -> JuizResult<Capsule> {
        Ok(jvalue!(ONCE_COUNT.fetch_add(1, Ordering::SeqCst) + 1).into())
    }

    fn fail(_v: CapsuleMap) -> JuizResult<Capsule> {
        Err(anyhow::Error::from(JuizError::InvalidValueError {
            message: "fail".to_owned(),
        }))
    }

    fn bound_core(
        name: &str,
        func: fn(CapsuleMap) -> JuizResult<Capsule>,
    ) -> Arc<Mutex<ExecutionContextCore>> {
        let core = ExecutionContextCore::new();
        let p = process_new(ProcessManifest::new("counter").name(name), func).unwrap();
        core.lock().unwrap().bind(ProcessPtr::new(p)).unwrap();
        core
    }

    #[test]
    fn factory_creates_one_shot_ec() {
        let factory = unsafe { execution_context_factory() }.unwrap();
        let factory = factory.lock().unwrap();
        assert_eq!(factory.type_name(), "one_shot_ec");
        let ec = factory.create(jvalue!({"name": "ec0"})).unwrap();
        let ec = ec.read().unwrap();
        assert_eq!(ec.name(), "ec0");
        assert_eq!(ec.type_name(), "OneShotEC");
        assert_eq!(ec.profile().unwrap(), jvalue!({"name": "ec0"}));
        // 周期実行の枠組みの上で動き、execute が false を返して 1 回で止まる
        assert!(ec.is_periodic());
    }

    #[test]
    fn factory_without_name_is_error() {
        let factory = unsafe { execution_context_factory() }.unwrap();
        assert!(factory.lock().unwrap().create(jvalue!({})).is_err());
    }

    #[test]
    fn bound_process_is_executed_only_once() {
        let ec = OneShotEC::new("ec0");
        let core = bound_core("once0", count_once);
        // ExecutionContextHolder の周期ループと同じく、execute が true の間だけ呼び続ける
        let mut calls = 0;
        loop {
            calls += 1;
            if !ec.read().unwrap().execute(&core).unwrap() {
                break;
            }
        }
        assert_eq!(calls, 1);
        assert_eq!(ONCE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execute_without_bound_process() {
        let ec = OneShotEC::new("ec0");
        let core = ExecutionContextCore::new();
        assert!(!ec.read().unwrap().execute(&core).unwrap());
    }

    #[test]
    fn process_error_is_returned() {
        let ec = OneShotEC::new("ec0");
        let core = bound_core("fail0", fail);
        assert!(ec.read().unwrap().execute(&core).is_err());
    }
}
