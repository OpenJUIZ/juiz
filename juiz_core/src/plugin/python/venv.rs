//! 有効にした venv(環境変数 VIRTUAL_ENV)の site-packages を、juiz が埋め込む Python の sys.path に足す (#296)。
//!
//! 埋め込みの Python は、初期化のときに sys.prefix を決める手がかりが OS で違う。
//! Linux・macOS はプログラム名(python3)を PATH から探すので、venv を有効にしていれば venv の
//! pyvenv.cfg が見つかり、そのまま venv が使われる。Windows は実行ファイル(juiz.exe)の場所を使うので
//! venv が無視され、venv に入れた pyjuiz などを読み込めない。
//! そこで、Python のモジュールを最初に読み込む前に一度だけ、venv の site-packages を足す。
//!
//! pip で入れる juiz(feature "extension-module")は Python の中で動くので、何もしない。

#[cfg(feature = "embed-python")]
use juiz_sdk::prelude::log;
use pyo3::prelude::*;

/// venv の site-packages を sys.path に足す Python のコード。
///
/// venv がすでに使われている(sys.prefix が venv)ときは何もしない。
/// pyvenv.cfg の版が埋め込みの Python と違うときは、拡張モジュールの ABI が合わないので足さない。
/// 足した場所は、システムの site-packages より前に移す(venv の中のものを優先する)。
#[cfg(feature = "embed-python")]
const ADD_VENV_PY: &str = r#"
import os, sys, site

def add(venv):
    if not venv:
        return ''
    norm = lambda p: os.path.normcase(os.path.abspath(p))
    if norm(sys.prefix) == norm(venv):
        return ''
    version = None
    try:
        with open(os.path.join(venv, 'pyvenv.cfg'), encoding='utf-8') as f:
            for line in f:
                key, _, value = line.partition('=')
                if key.strip() in ('version', 'version_info'):
                    version = tuple(int(x) for x in value.strip().split('.')[:2])
                    break
    except (OSError, ValueError):
        pass
    if version is not None and version != sys.version_info[:2]:
        return 'warning: the venv %s is for Python %d.%d, but juiz embeds Python %d.%d; its packages are not used' % (
            (venv,) + version + sys.version_info[:2])
    if os.name == 'nt':
        sp = os.path.join(venv, 'Lib', 'site-packages')
    else:
        sp = os.path.join(venv, 'lib', 'python%d.%d' % sys.version_info[:2], 'site-packages')
    if not os.path.isdir(sp) or norm(sp) in [norm(p) for p in sys.path if p]:
        return ''
    before = list(sys.path)
    site.addsitedir(sp)
    added = [p for p in sys.path if p not in before]
    for p in added:
        sys.path.remove(p)
    index = next((i for i, p in enumerate(sys.path) if os.path.basename(p.rstrip('/\\')).lower() == 'site-packages'), len(sys.path))
    sys.path[index:index] = added
    return 'added ' + sp
"#;

/// VIRTUAL_ENV の venv の site-packages を sys.path に足し、何をしたかを返す(何もしなければ空文字列)。
#[cfg(feature = "embed-python")]
fn add_virtualenv_site_packages(py: Python, venv: Option<&str>) -> PyResult<String> {
    let module = PyModule::from_code(
        py,
        &std::ffi::CString::new(ADD_VENV_PY).unwrap(),
        c"juiz_venv.py",
        c"juiz_venv",
    )?;
    module
        .getattr("add")?
        .call1((venv.unwrap_or(""),))?
        .extract()
}

/// VIRTUAL_ENV の venv の site-packages を、プロセスの中で一度だけ sys.path に足す。
/// 失敗してもプラグインの読み込みは止めず、ログに残すだけにする。
///
/// 複数のスレッドから同時に呼ばれることがある(`python_check` や Python のプラグインの読み込み)。
/// `Once::call_once` を GIL を持ったまま待つと、先に入ったスレッドが中の Python のコードで GIL を
/// 手放したときに GIL を取り戻せず、互いに待ち合って止まる (#346)。そこで、待つ間は GIL を手放す
/// pyo3 の `call_once_py_attached` を使う。
pub(super) fn add_virtualenv_once(py: Python) {
    #[cfg(feature = "embed-python")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        add_virtualenv_once_with(py, &ONCE, || std::env::var("VIRTUAL_ENV").ok());
    }
    #[cfg(not(feature = "embed-python"))]
    let _ = py;
}

/// `add_virtualenv_once` の本体。テストで新しい `Once` と venv を渡せるよう分けている。
#[cfg(feature = "embed-python")]
fn add_virtualenv_once_with(
    py: Python,
    once: &std::sync::Once,
    venv: impl FnOnce() -> Option<String>,
) {
    use pyo3::sync::OnceExt;
    once.call_once_py_attached(py, || {
        let venv = venv();
        match add_virtualenv_site_packages(py, venv.as_deref()) {
            Ok(msg) if msg.starts_with("warning: ") => {
                log::warn!("{}", &msg["warning: ".len()..])
            }
            Ok(msg) if !msg.is_empty() => log::info!("Python: {msg} (VIRTUAL_ENV)"),
            Ok(_) => {}
            Err(e) => {
                log::warn!("Python: failed to add the site-packages of VIRTUAL_ENV ({venv:?}): {e}")
            }
        }
    });
}

#[cfg(all(test, feature = "embed-python"))]
mod tests {
    use super::*;

    /// sys.path を書き換えるテストどうしを並べて動かさないためのロック。
    /// add() は sys.path を読んでから足し直すので、別のスレッドと同時に動くと足した場所を見失う。
    /// GIL を持つ前に取ること(GIL を持ったまま待つと、#346 と同じく止まる)。
    static SYS_PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// venv の形のディレクトリを作る(pyvenv.cfg と、モジュールを 1 つ入れた site-packages)
    fn fake_venv(name: &str, version: (u8, u8), module: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("juiz_venv_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sp = if cfg!(windows) {
            dir.join("Lib").join("site-packages")
        } else {
            dir.join("lib")
                .join(format!("python{}.{}", version.0, version.1))
                .join("site-packages")
        };
        std::fs::create_dir_all(&sp).unwrap();
        std::fs::write(
            dir.join("pyvenv.cfg"),
            format!("home = x\nversion = {}.{}.0\n", version.0, version.1),
        )
        .unwrap();
        std::fs::write(sp.join(format!("{module}.py")), "VALUE = 42\n").unwrap();
        dir
    }

    #[test]
    fn adds_site_packages_of_the_venv() {
        let _lock = SYS_PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Python::attach(|py| {
            let v = py.version_info();
            let dir = fake_venv("ok", (v.major, v.minor), "juiz_venv_test_module_ok");
            let msg = add_virtualenv_site_packages(py, dir.to_str()).unwrap();
            assert!(msg.starts_with("added "), "{msg}");
            let value: i64 = py
                .import("juiz_venv_test_module_ok")
                .unwrap()
                .getattr("VALUE")
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(value, 42);
            // 2 回目は足さない
            assert_eq!(add_virtualenv_site_packages(py, dir.to_str()).unwrap(), "");
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    #[test]
    fn skips_a_venv_of_another_python_version() {
        Python::attach(|py| {
            let dir = fake_venv("other", (2, 7), "juiz_venv_test_module_other");
            let msg = add_virtualenv_site_packages(py, dir.to_str()).unwrap();
            assert!(msg.starts_with("warning: "), "{msg}");
            assert!(py.import("juiz_venv_test_module_other").is_err());
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    #[test]
    fn does_nothing_without_venv() {
        Python::attach(|py| {
            assert_eq!(add_virtualenv_site_packages(py, None).unwrap(), "");
        });
    }

    /// 複数のスレッドから同時に呼んでも止まらない (#346)。
    /// venv がある(Python のコードが長めに動き、途中で GIL を手放す)と、以前の実装ではほぼ毎回止まった。
    #[test]
    fn once_does_not_deadlock_with_threads() {
        let _lock = SYS_PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let v = Python::attach(|py| {
            let v = py.version_info();
            (v.major, v.minor)
        });
        let dir = fake_venv("threads", v, "juiz_venv_test_module_threads");
        let once = std::sync::Arc::new(std::sync::Once::new());
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..8 {
            let (tx, once, dir) = (tx.clone(), once.clone(), dir.clone());
            std::thread::spawn(move || {
                Python::attach(|py| {
                    add_virtualenv_once_with(py, &once, || Some(dir.to_string_lossy().into_owned()))
                });
                let _ = tx.send(());
            });
        }
        drop(tx);
        for _ in 0..8 {
            rx.recv_timeout(std::time::Duration::from_secs(30))
                .expect("add_virtualenv_once did not return within 30 seconds (deadlock?)");
        }
        assert!(once.is_completed());
        Python::attach(|py| assert!(py.import("juiz_venv_test_module_threads").is_ok()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
