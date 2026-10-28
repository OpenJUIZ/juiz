use std::{collections::HashMap, ffi::CString};

use pyo3::{prelude::*, types::IntoPyDict};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let m = HashMap::from([("key0", "value0"), ("key1", "value1")]);
    let py_foo = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/python_app/utils/foo.py"
    ));
    let py_app = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/python_app/app.py"));
    let py_foo = CString::new(py_foo)?;
    let py_app = CString::new(py_app)?;
    let from_python = Python::attach(|py| -> PyResult<Py<PyAny>> {
        PyModule::from_code(py, &py_foo, c"utils.foo", c"utils.foo")?;
        let app = PyModule::from_code(py, &py_app, c"", c"")?.getattr("run")?;
        //app.call0()
        Ok(app.call((), Some(&m.into_py_dict(py)?))?.unbind())
    });

    println!("py: {}", from_python?);
    Ok(())
}
