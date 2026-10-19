#[cfg(feature = "python")]
mod python_plugin;
//mod python_process_factory_impl;
//mod python_container_factory_impl;
//mod python_container_process_factory_impl;

#[cfg(feature = "python")]
pub use python_plugin::PythonPlugin;

#[cfg(not(feature = "python"))]
mod disabled;
#[cfg(not(feature = "python"))]
pub use disabled::PythonPlugin;
