//! juiz が埋め込む(または juiz を動かしている)Python の版と、入っているパッケージを調べる。
//! `juiz run` がパッケージの `python.version` と `python.requires` を確かめるのに使う
//! (docs/spec/app-package.md §9 の段 7・8、#285)。

use crate::prelude::*;

/// この juiz が Python のプラグインを読み込めるか(feature "python" 付きでビルドされたか)
pub const PYTHON_ENABLED: bool = cfg!(feature = "python");

/// Python を調べた結果
#[derive(Debug, Clone)]
pub struct PythonCheck {
    /// Python の版(`3.12.1` など)
    pub version: String,
    /// `python.version` の指定を満たすか(指定が無ければ true)
    pub version_ok: bool,
    /// `python.requires` のうち満たされないもの
    pub missing: Vec<String>,
    /// pip を案内するときに使う Python の実行ファイル
    pub executable: String,
    /// その Python が externally-managed(PEP 668)で、pip で直接入れられないか(Homebrew の Python など、#321)
    pub externally_managed: bool,
    /// Python のプラグインに要る `juiz` モジュール(pyjuiz)が見つからないか(#321)
    pub pyjuiz_missing: bool,
}

#[cfg(feature = "python")]
const CHECK_PY: &str = r#"
import json, re, sys, os
import importlib.metadata as md

def _num(v):
    out = []
    for p in v.split('.'):
        m = re.match(r'\d+', p)
        out.append(int(m.group(0)) if m else 0)
    while len(out) > 1 and out[-1] == 0:
        out.pop()
    return tuple(out)

def _simple_match(spec, version):
    # packaging が無いときの簡易版(数字の比較だけ)
    for s in [x.strip() for x in spec.split(',') if x.strip()]:
        m = re.match(r'(~=|===|==|!=|<=|>=|<|>)\s*(.+)', s)
        if not m:
            return False
        op, v = m.group(1), m.group(2).strip()
        if op in ('==', '===') and v.endswith('.*'):
            pre = _num(v[:-2])
            if _num(version)[:len(pre)] != pre:
                return False
            continue
        a, b = _num(version), _num(v)
        ok = {'==': a == b, '===': a == b, '!=': a != b, '<=': a <= b, '>=': a >= b,
              '<': a < b, '>': a > b,
              '~=': a >= b and a[:max(len(b) - 1, 1)] == b[:max(len(b) - 1, 1)]}[op]
        if not ok:
            return False
    return True

def check(requires_json, python_version):
    requires = json.loads(requires_json)
    try:
        from packaging.requirements import Requirement
        from packaging.specifiers import SpecifierSet
        have_packaging = True
    except Exception:
        have_packaging = False
    version = '%d.%d.%d' % sys.version_info[:3]
    if not python_version:
        version_ok = True
    elif have_packaging:
        version_ok = SpecifierSet(python_version).contains(version, prereleases=True)
    else:
        version_ok = _simple_match(python_version, version)
    missing = []
    for r in requires:
        if have_packaging:
            req = Requirement(r)
            if req.marker is not None and not req.marker.evaluate():
                continue
            name, spec = req.name, req.specifier
        else:
            m = re.match(r'\s*([A-Za-z0-9][A-Za-z0-9._-]*)\s*(\[[^\]]*\])?\s*([^;]*)', r)
            if not m:
                missing.append(r)
                continue
            name, spec = m.group(1), m.group(3).strip()
        try:
            installed = md.version(name)
        except md.PackageNotFoundError:
            missing.append(r)
            continue
        if have_packaging:
            ok = spec.contains(installed, prereleases=True)
        else:
            ok = (not spec) or _simple_match(spec, installed)
        if not ok:
            missing.append(r)
    exe, in_venv = _executable()
    import importlib.util
    return json.dumps({'version': version, 'version_ok': version_ok, 'missing': missing, 'executable': exe,
                       'externally_managed': (not in_venv) and _externally_managed(),
                       'pyjuiz_missing': importlib.util.find_spec('juiz') is None})

def _executable():
    # pip を案内する Python の実行ファイルと、それが venv の中のものか (#321)
    exe = sys.executable or ''
    if os.path.basename(exe).lower().startswith('python'):
        return exe, sys.prefix != sys.base_prefix
    # 埋め込みの Python では sys.executable が juiz 自身になる。
    # 有効にした venv があれば、juiz はその site-packages を使う (#296) ので、venv の python を案内する
    venv = os.environ.get('VIRTUAL_ENV')
    if venv:
        p = os.path.join(venv, 'Scripts', 'python.exe') if os.name == 'nt' else os.path.join(venv, 'bin', 'python')
        if os.path.isfile(p):
            return p, True
    # それ以外は、埋め込んでいる Python と同じ場所の実行ファイル
    if os.name == 'nt':
        candidates = [os.path.join(sys.base_prefix, 'python.exe')]
    else:
        candidates = [os.path.join(sys.base_prefix, 'bin', 'python%d.%d' % sys.version_info[:2]),
                      os.path.join(sys.base_prefix, 'bin', 'python3')]
    for p in candidates:
        if os.path.isfile(p):
            return p, False
    return ('python' if os.name == 'nt' else 'python3'), False

def _externally_managed():
    # PEP 668: 標準ライブラリの場所に EXTERNALLY-MANAGED があれば、pip はそこに入れられない
    try:
        import sysconfig
        return os.path.isfile(os.path.join(sysconfig.get_path('stdlib'), 'EXTERNALLY-MANAGED'))
    except Exception:
        return False
"#;

/// Python の版と `requires`(PEP 508 の要求の列)を調べる。
///
/// `packaging` が入っていればそれで比べ、無ければ数字だけを比べる簡易版で比べる。
#[cfg(feature = "python")]
pub fn python_check(requires: &[String], python_version: Option<&str>) -> JuizResult<PythonCheck> {
    use pyo3::prelude::*;
    use std::ffi::CString;
    let requires_json = juiz_sdk::serde_json::to_string(requires)?;
    let result: String = Python::attach(|py| -> PyResult<String> {
        // venv に入れたパッケージも数えるよう、先に venv の site-packages を足す (#296)
        super::venv::add_virtualenv_once(py);
        let code = CString::new(CHECK_PY).unwrap();
        let module =
            PyModule::from_code(py, &code, c"juiz_package_check.py", c"juiz_package_check")?;
        module
            .getattr("check")?
            .call1((requires_json, python_version.unwrap_or("")))?
            .extract()
    })
    .map_err(|e| juiz_sdk::anyhow::anyhow!("failed to check the Python environment: {e}"))?;
    let v: Value = juiz_sdk::serde_json::from_str(&result)?;
    Ok(PythonCheck {
        version: v["version"].as_str().unwrap_or_default().to_owned(),
        version_ok: v["version_ok"].as_bool().unwrap_or(false),
        missing: v["missing"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        executable: v["executable"].as_str().unwrap_or("python3").to_owned(),
        externally_managed: v["externally_managed"].as_bool().unwrap_or(false),
        pyjuiz_missing: v["pyjuiz_missing"].as_bool().unwrap_or(false),
    })
}

#[cfg(not(feature = "python"))]
pub fn python_check(
    _requires: &[String],
    _python_version: Option<&str>,
) -> JuizResult<PythonCheck> {
    Err(juiz_sdk::anyhow::anyhow!(
        "this juiz was built without Python support"
    ))
}

#[cfg(all(test, feature = "embed-python"))]
mod tests {
    use super::*;

    #[test]
    fn checks_version_and_requires() {
        let r = python_check(&[], None).unwrap();
        assert!(r.version_ok);
        assert!(r.missing.is_empty());
        assert!(r.version.starts_with('3'));

        let r = python_check(&[], Some(">=3")).unwrap();
        assert!(r.version_ok);
        let r = python_check(&[], Some("<3")).unwrap();
        assert!(!r.version_ok);

        let reqs = vec!["surely-not-installed-juiz-package>=1".to_owned()];
        let r = python_check(&reqs, None).unwrap();
        assert_eq!(r.missing, vec!["surely-not-installed-juiz-package>=1"]);

        // 埋め込みの Python でも、案内する実行ファイルは juiz 自身や PATH 頼みの名前ではなく、実在するパスにする (#321)。
        // python_check を別のテストから同時に呼ぶと止まることがあるので、同じテストの中で確かめる
        let exe = std::path::Path::new(&r.executable);
        assert!(exe.is_absolute() && exe.is_file(), "{}", r.executable);
        let name = exe.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(name.starts_with("python"), "{}", r.executable);
    }
}
