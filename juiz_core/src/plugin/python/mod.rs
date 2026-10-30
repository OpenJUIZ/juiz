#[cfg(feature = "python")]
mod python_plugin;
//mod python_process_factory_impl;
//mod python_container_factory_impl;
//mod python_container_process_factory_impl;

#[cfg(feature = "python")]
pub use python_plugin::PythonPlugin;

// 有効にした venv の site-packages を埋め込みの Python に足す (#296)
#[cfg(feature = "python")]
mod venv;

#[cfg(not(feature = "python"))]
mod disabled;
#[cfg(not(feature = "python"))]
pub use disabled::PythonPlugin;

// juiz run がパッケージの Python の要求を確かめる (docs/spec/app-package.md §9、#285)
mod env;
pub use env::{python_check, PythonCheck, PYTHON_ENABLED};
