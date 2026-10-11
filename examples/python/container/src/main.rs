use std::collections::HashMap;

use pyo3::{prelude::*, types::IntoPyDict};

fn main() -> PyResult<()> {
    let m = HashMap::from([("key0", "value0"), ("key1", "value1")]);
    let py_foo = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/python_app/utils/foo.py"
    ));
    let py_app = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/python_app/app.py"));
    let from_python = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
        PyModule::from_code_bound(py, py_foo, "utils.foo", "utils.foo")?;
        let app: Py<PyAny> = PyModule::from_code_bound(py, py_app, "", "")?
            .getattr("run")?
            .into();
        //app.call0(py)
        app.call_bound(py, (), Some(&m.into_py_dict_bound(py)))
    });

    println!("py: {}", from_python?);
    Ok(())
}
