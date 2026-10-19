//! Python から juiz を起動するための入口(feature `python-entry`、issue #88)。
//!
//! pip で入る `juiz` コマンドは、Python のエントリポイントからこのモジュールの
//! [`main_with_args`] を呼ぶ。Rust 側は Python プロセスの中で動くので、
//! Python の Process は、そのプロセスの Python でそのまま読み込める。

use pyo3::prelude::*;
use std::ffi::OsString;

use juiz_core::log;

/// GIL を解放してから juiz を実行し、終了コードを返す。
///
/// - GIL を握ったまま実行すると、Python の Process を呼ぶスレッド(HTTP ブローカーなど)の
///   `Python::with_gil` が待ち続けて止まるので、[`Python::allow_threads`] の中で実行する。
/// - Ctrl+C(SIGINT)は Rust 側で受ける。Python は起動時に SIGINT のハンドラを入れていて、
///   これは Python のコードが動くまで KeyboardInterrupt を起こさないため、そのままだと
///   juiz が止まらない。そこで実行中だけ SIGINT を既定の動作(プロセス終了)に戻し、
///   ネイティブの `juiz` と同じく、待機中は `System::wait_for_singal` が受けて片付けてから終わり、
///   それ以外のときはそのまま終了するようにする。終わったら元のハンドラに戻す。
pub fn main_with_args<I, T>(py: Python<'_>, args: I) -> i32
where
    I: IntoIterator<Item = T> + Send,
    T: Into<OsString> + Clone,
{
    let saved = default_sigint(py);
    let code = py.allow_threads(|| crate::main_with_args(args));
    if let Some(handler) = saved {
        restore_sigint(py, handler);
    }
    code
}

/// SIGINT のハンドラを既定の動作に戻し、元のハンドラを返す。
///
/// Python の `signal.signal` はメインスレッドからしか呼べない。メインスレッド以外
/// (テストなど)から呼ばれたときは何もしない。
fn default_sigint(py: Python<'_>) -> Option<PyObject> {
    let result = (|| -> PyResult<PyObject> {
        let signal = py.import_bound("signal")?;
        let sigint = signal.getattr("SIGINT")?;
        let saved = signal.call_method1("getsignal", (&sigint,))?;
        signal.call_method1("signal", (&sigint, signal.getattr("SIG_DFL")?))?;
        Ok(saved.unbind())
    })();
    match result {
        Ok(saved) => Some(saved),
        Err(e) => {
            log::debug!("SIGINT のハンドラを変えずに実行します: {e}");
            None
        }
    }
}

fn restore_sigint(py: Python<'_>, handler: PyObject) {
    let result = (|| -> PyResult<()> {
        let signal = py.import_bound("signal")?;
        signal.call_method1("signal", (signal.getattr("SIGINT")?, handler))?;
        Ok(())
    })();
    if let Err(e) = result {
        log::warn!("SIGINT のハンドラを元に戻せませんでした: {e}");
    }
}
