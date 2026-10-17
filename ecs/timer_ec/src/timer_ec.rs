use juiz_core::{anyhow, env_logger, log};
use juiz_core::{ExecutionContext, ExecutionContextCore, ExecutionContextFactory};
use std::{
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use juiz_core::prelude::*;

/// rate [Hz] で周期的に svc を呼ぶ実行コンテキスト。
///
/// 起動時刻を開始時刻から period ずつ進めた絶対時刻で管理するので、svc の処理時間で周期はずれない。
/// svc が周期を超えた場合は、過ぎてしまった起動時刻を飛ばして次の起動時刻を待つ。
pub struct TimerEC {
    rate: f64,
    name: String,
    period: Duration,
    next_wakeup: Arc<Mutex<Option<Instant>>>,
}

impl TimerEC {
    pub fn new(name: &str, rate: f64) -> JuizResult<Arc<RwLock<TimerEC>>> {
        let period = period_from_rate(rate)?;
        Ok(Arc::new(RwLock::new(TimerEC {
            rate,
            name: name.to_string(),
            period,
            next_wakeup: Arc::new(Mutex::new(None)),
        })))
    }

    pub fn period(&self) -> Duration {
        self.period
    }
}

/// rate [Hz] から周期を求める。rate は正の有限値でなければならない。
pub fn period_from_rate(rate: f64) -> JuizResult<Duration> {
    if !rate.is_finite() || rate <= 0.0 {
        return Err(anyhow::Error::from(JuizError::InvalidValueError {
            message: format!("TimerEC: rate must be a positive number in Hz, but got {rate}"),
        }));
    }
    match Duration::try_from_secs_f64(1.0 / rate) {
        Ok(period) if !period.is_zero() => Ok(period),
        _ => Err(anyhow::Error::from(JuizError::InvalidValueError {
            message: format!("TimerEC: rate {rate} Hz can not be converted to a period"),
        })),
    }
}

/// 起動時刻 `wakeup` に処理した後の、次の起動時刻を返す。
///
/// 通常は `wakeup + period`。処理が周期を超えて `now` がそれを過ぎている場合は、
/// 過ぎた起動時刻を飛ばし、`wakeup + n * period` のうち `now` より後で最も早い時刻を返す。
pub fn next_wakeup(wakeup: Instant, period: Duration, now: Instant) -> Instant {
    let next = wakeup + period;
    if next > now {
        return next;
    }
    let behind = now.duration_since(next);
    let skipped = (behind.as_nanos() / period.as_nanos()) as u32 + 1;
    log::debug!("TimerEC: svc overran the period. {skipped} wakeup(s) skipped.");
    next + period * skipped
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

    fn on_starting(&mut self, _core: &Arc<Mutex<ExecutionContextCore>>) -> JuizResult<()> {
        *juiz_lock(&self.next_wakeup)? = Some(Instant::now());
        Ok(())
    }

    fn execute(&self, core: &Arc<Mutex<ExecutionContextCore>>) -> JuizResult<bool> {
        log::trace!("TimerExecutionContext.execute called");
        let wakeup = juiz_lock(&self.next_wakeup)?.unwrap_or_else(Instant::now);
        let now = Instant::now();
        if wakeup > now {
            std::thread::sleep(wakeup - now);
        }
        match core.lock() {
            Err(e) => {
                log::error!("Error({e:?}) in Locking ECServiceFunction");
                return Err(anyhow::Error::from(
                    JuizError::ExecutionContextCanNotLockStateError {},
                ));
            }
            Ok(svc_func) => {
                let _ = svc_func.svc().map_err(|e| -> () {
                    log::error!("Error({e:?}) in Service function in ExecutionContext.");
                });
            }
        }
        *juiz_lock(&self.next_wakeup)? = Some(next_wakeup(wakeup, self.period, Instant::now()));
        return Ok(true);
    }
}

struct TimerECFactory {}

impl ExecutionContextFactory for TimerECFactory {
    fn type_name(&self) -> &str {
        "TimerEC"
    }

    fn create(&self, manifest: Value) -> JuizResult<Arc<RwLock<dyn ExecutionContext>>> {
        let name = obj_get_str(&manifest, "name")?;
        let rate = obj_get_f64(&manifest, "rate")?;
        Ok(TimerEC::new(name, rate)?)
    }
}

#[no_mangle]
pub unsafe extern "Rust" fn execution_context_factory(
) -> JuizResult<Arc<Mutex<dyn ExecutionContextFactory>>> {
    let _ = env_logger::try_init();
    Ok(Arc::new(Mutex::new(TimerECFactory {})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_is_frequency_in_hz() {
        assert_eq!(period_from_rate(100.0).unwrap(), Duration::from_millis(10));
        assert_eq!(period_from_rate(0.5).unwrap(), Duration::from_secs(2));
        let ec = TimerEC::new("timer", 1000.0).unwrap();
        assert_eq!(ec.read().unwrap().period(), Duration::from_millis(1));
    }

    #[test]
    fn non_positive_rate_is_error() {
        for rate in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e300] {
            assert!(period_from_rate(rate).is_err(), "rate={rate}");
            assert!(TimerEC::new("timer", rate).is_err(), "rate={rate}");
        }
    }

    #[test]
    fn next_wakeup_advances_by_period_from_previous_wakeup() {
        let start = Instant::now();
        let period = Duration::from_millis(10);
        // 処理時間 3ms でも、次の起動時刻は前の起動時刻 + period
        assert_eq!(
            next_wakeup(start, period, start + Duration::from_millis(3)),
            start + period
        );
    }

    #[test]
    fn next_wakeup_skips_missed_wakeups_on_overrun() {
        let start = Instant::now();
        let period = Duration::from_millis(10);
        // 25ms かかったら 10ms, 20ms を飛ばして 30ms
        assert_eq!(
            next_wakeup(start, period, start + Duration::from_millis(25)),
            start + Duration::from_millis(30)
        );
        // ちょうど 20ms のときも、過ぎた時刻なので 30ms
        assert_eq!(
            next_wakeup(start, period, start + Duration::from_millis(20)),
            start + Duration::from_millis(30)
        );
    }

    #[test]
    fn periodic_execution_does_not_drift() {
        let ec = TimerEC::new("timer", 100.0).unwrap();
        let core = ExecutionContextCore::new();
        ec.write().unwrap().on_starting(&core).unwrap();
        let begin = Instant::now();
        for _ in 0..10 {
            ec.read().unwrap().execute(&core).unwrap();
        }
        // 1 回目は即時、以降 10ms ごとなので 10 回目はおよそ 90ms 後
        let elapsed = begin.elapsed();
        assert!(elapsed >= Duration::from_millis(89), "elapsed={elapsed:?}");
        assert!(elapsed < Duration::from_millis(200), "elapsed={elapsed:?}");
    }
}
