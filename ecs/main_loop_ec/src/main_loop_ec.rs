use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use juiz_core::prelude::*;
use juiz_core::{
    ExecutionContext, ExecutionContextCore, ExecutionContextFactory, ExecutionContextState,
};

use juiz_core::env_logger;
use juiz_core::log;

pub struct MainLoopEC {
    name: String,
    rate: f64,
}

impl MainLoopEC {
    pub fn new(name: &str, rate: f64) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            name: name.to_owned(),
            rate,
        }))
    }
}

impl ExecutionContext for MainLoopEC {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn type_name(&self) -> &str {
        "main_loop_ec"
    }

    fn profile(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "rate": self.rate,
        }))
    }

    fn execute(&self, core: &Arc<Mutex<ExecutionContextCore>>) -> JuizResult<bool> {
        log::trace!("MainLoopEC({:}).execute() called", self.name);
        juiz_lock(&core)?.svc().and(Ok(false))
    }

    fn on_load(&mut self, system: &mut System, core: Arc<Mutex<ExecutionContextCore>>) -> () {
        log::debug!("MainLoopEC({:}).on_load() called", self.name);
        let c = core.clone();
        let func: Box<dyn Fn() -> JuizResult<()>> = Box::new(move || -> JuizResult<()> {
            log::trace!("MainLoopEC().execute_func() called");
            match juiz_lock(&c) {
                Err(e) => return Err(e),
                Ok(cc) => match cc.get_state() {
                    ExecutionContextState::STARTED => cc.svc().and(Ok(())),
                    _ => Ok(()),
                },
            }
        });
        system.set_spin_callback(func);
        system.set_spin_sleeptime(Duration::from_micros((1000000.0 / self.rate) as u64));
    }

    fn is_periodic(&self) -> bool {
        return false;
    }
}

struct MainLoopECFactory {}

impl ExecutionContextFactory for MainLoopECFactory {
    fn type_name(&self) -> &str {
        "main_loop_ec"
    }

    fn create(&self, manifest: Value) -> JuizResult<Arc<RwLock<dyn ExecutionContext>>> {
        let name = obj_get_str(&manifest, "name")?;
        let rate = obj_get_f64(&manifest, "rate")?;
        Ok(MainLoopEC::new(name, rate))
    }
}

#[no_mangle]
pub unsafe extern "Rust" fn execution_context_factory(
) -> JuizResult<Arc<Mutex<dyn ExecutionContextFactory>>> {
    let _ = env_logger::try_init();
    Ok(Arc::new(Mutex::new(MainLoopECFactory {})))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use juiz_core::anyhow;

    static EXECUTE_COUNT: AtomicI64 = AtomicI64::new(0);
    static SPIN_COUNT: AtomicI64 = AtomicI64::new(0);

    fn count_execute(_v: CapsuleMap) -> JuizResult<Capsule> {
        Ok(jvalue!(EXECUTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1).into())
    }

    fn count_spin(_v: CapsuleMap) -> JuizResult<Capsule> {
        Ok(jvalue!(SPIN_COUNT.fetch_add(1, Ordering::SeqCst) + 1).into())
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

    /// ExecutionContextHolder の start / stop と同じく core の状態を切り替える
    fn set_state(core: &Arc<Mutex<ExecutionContextCore>>, state: ExecutionContextState) {
        core.lock()
            .unwrap()
            .state
            .store(state.to_i64(), Ordering::SeqCst);
    }

    #[test]
    fn factory_creates_main_loop_ec() {
        let factory = unsafe { execution_context_factory() }.unwrap();
        let factory = factory.lock().unwrap();
        assert_eq!(factory.type_name(), "main_loop_ec");
        let ec = factory
            .create(jvalue!({"name": "ec0", "rate": 10.0}))
            .unwrap();
        let ec = ec.read().unwrap();
        assert_eq!(ec.name(), "ec0");
        assert_eq!(ec.type_name(), "main_loop_ec");
        assert_eq!(ec.profile().unwrap(), jvalue!({"rate": 10.0}));
        // 自前のスレッドではなく System のメインループ (spin) で動く
        assert!(!ec.is_periodic());
    }

    #[test]
    fn factory_without_name_or_rate_is_error() {
        let factory = unsafe { execution_context_factory() }.unwrap();
        let factory = factory.lock().unwrap();
        assert!(factory.create(jvalue!({"rate": 10.0})).is_err());
        assert!(factory.create(jvalue!({"name": "ec0"})).is_err());
    }

    #[test]
    fn execute_runs_bound_process_once() {
        let ec = MainLoopEC::new("ec0", 10.0);
        let core = bound_core("exec0", count_execute);
        assert!(!ec.read().unwrap().execute(&core).unwrap());
        assert_eq!(EXECUTE_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execute_returns_process_error() {
        let ec = MainLoopEC::new("ec0", 10.0);
        let core = bound_core("fail0", fail);
        assert!(ec.read().unwrap().execute(&core).is_err());
    }

    #[test]
    fn spin_runs_bound_process_only_while_started() {
        let ec = MainLoopEC::new("ec0", 1000.0);
        let core = bound_core("spin0", count_spin);
        let mut system = System::new(jvalue!({"name": "test_system"})).unwrap();
        ec.write().unwrap().on_load(&mut system, core.clone());

        // start 前は spin しても実行されない
        system.spin();
        assert_eq!(SPIN_COUNT.load(Ordering::SeqCst), 0);

        // start 後は spin のたびに 1 回ずつ実行される
        set_state(&core, ExecutionContextState::STARTED);
        system.spin();
        system.spin();
        assert_eq!(SPIN_COUNT.load(Ordering::SeqCst), 2);

        // stop 後はまた実行されない
        set_state(&core, ExecutionContextState::STOPPED);
        system.spin();
        assert_eq!(SPIN_COUNT.load(Ordering::SeqCst), 2);
    }
}
