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
pub(super) fn add_virtualenv_once(py: Python) {
    #[cfg(feature = "embed-python")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let venv = std::env::var("VIRTUAL_ENV").ok();
            match add_virtualenv_site_packages(py, venv.as_deref()) {
                Ok(msg) if msg.starts_with("warning: ") => {
                    log::warn!("{}", &msg["warning: ".len()..])
                }
                Ok(msg) if !msg.is_empty() => log::info!("Python: {msg} (VIRTUAL_ENV)"),
                Ok(_) => {}
                Err(e) => log::warn!(
                    "Python: failed to add the site-packages of VIRTUAL_ENV ({venv:?}): {e}"
                ),
            }
        });
    }
    #[cfg(not(feature = "embed-python"))]
    let _ = py;
}

#[cfg(all(test, feature = "embed-python"))]
mod tests {
    use super::*;

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
}
