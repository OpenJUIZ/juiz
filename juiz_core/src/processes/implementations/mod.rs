

mod process_proxy;
mod process_impl;
mod process_factory_wrapper;
mod process_factory_impl;
mod inlet;
mod outlet;

pub use process_factory_wrapper::ProcessFactoryWrapper;
pub use process_proxy::ProcessProxy;

//pub use process_impl::process_from_clousure_new_with_class_name;
//pub use process_impl::process_new;
pub(crate) use process_impl::ProcessImpl;
pub use process_factory_impl::ProcessFactoryImpl;
pub use process_impl::process_new;