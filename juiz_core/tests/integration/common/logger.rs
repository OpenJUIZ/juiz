//! テスト全体で共有する logger
//!
//! 統合テストは 1 本のバイナリにまとめてあるので、logger はプロセスに 1 つしか置けない。
//! env_logger::try_init() を直接呼ぶテストと、ログを溜めて中身を確かめるテストが同居できるよう、
//! env_logger に流しつつ Info 以上のログを溜めておく logger をここで 1 度だけ設定する。
//! テストでは env_logger::try_init() ではなく `crate::common::logger::init()` を呼ぶこと
//! (先に env_logger が設定されると、ログを確かめるテストが失敗する)。

use std::sync::{Mutex, OnceLock};

use juiz_core::prelude::*;

struct TestLogger {
    env: env_logger::Logger,
    captured: Mutex<Vec<String>>,
}

impl log::Log for TestLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info || self.env.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if record.level() <= log::Level::Info {
            self.captured
                .lock()
                .unwrap()
                .push(record.args().to_string());
        }
        if self.env.matches(record) {
            self.env.log(record);
        }
    }

    fn flush(&self) {
        self.env.flush();
    }
}

static LOGGER: OnceLock<TestLogger> = OnceLock::new();

/// logger を設定する(何度呼んでもよい)。出力は env_logger と同じく RUST_LOG で選ぶ
pub fn init() {
    let mut first = false;
    let logger = LOGGER.get_or_init(|| {
        first = true;
        TestLogger {
            env: env_logger::Builder::from_default_env().build(),
            captured: Mutex::new(Vec::new()),
        }
    });
    if first {
        log::set_logger(logger).expect("logger が先に設定されています (env_logger::try_init() を直接呼んでいる箇所がないか確認してください)");
        log::set_max_level(logger.env.filter().max(log::LevelFilter::Info));
    }
}

/// これまでに溜まった Info 以上のログ
pub fn captured() -> Vec<String> {
    LOGGER
        .get()
        .map(|l| l.captured.lock().unwrap().clone())
        .unwrap_or_default()
}

/// 溜まったログを捨てる(前に走った別のテストのログを除くため、確かめる前に呼ぶ)
pub fn clear_captured() {
    if let Some(l) = LOGGER.get() {
        l.captured.lock().unwrap().clear();
    }
}
