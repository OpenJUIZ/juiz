//! Python から juiz を起動する入口(feature python-entry)のテスト。
//! 埋め込み Python(embed-python)で GIL を持った状態から呼び、GIL を解放して動くことを確かめる。
#![cfg(all(feature = "python-entry", feature = "embed-python"))]

use pyo3::Python;

#[test]
fn python_entry_help_returns_zero() {
    let code = Python::with_gil(|py| juiz_app::python::main_with_args(py, ["juiz", "--help"]));
    assert_eq!(code, 0);
}

#[test]
fn python_entry_bad_option_returns_error_code() {
    let code =
        Python::with_gil(|py| juiz_app::python::main_with_args(py, ["juiz", "--no-such-option"]));
    assert_eq!(code, 2);
}

#[test]
fn python_entry_loads_python_process() {
    // Python の Process を読み込んで作る(GIL を解放してから juiz_core が with_gil で読み込む)
    let code = Python::with_gil(|py| {
        juiz_app::python::main_with_args(py, ["juiz", "-f", "tests/python_entry.conf"])
    });
    assert_eq!(code, 0);
}
