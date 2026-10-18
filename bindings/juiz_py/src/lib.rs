//! PyPI パッケージ `juiz` の拡張モジュール(issue #88)。
//!
//! `juiz` コマンドは Python のエントリポイント(`juiz_cli:main`)からここの [`main`] を呼ぶ。
//! juiz は Python プロセスの中で動くので、Python の Process はそのプロセスの Python で読み込まれる。

use pyo3::prelude::*;

/// juiz を引数 `args`(先頭はコマンド名)で実行し、終了コードを返す。
#[pyfunction]
fn main(py: Python<'_>, args: Vec<String>) -> i32 {
    juiz_app::python::main_with_args(py, args)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(main, m)?)?;
    Ok(())
}
