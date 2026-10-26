mod composite_process_factory;
mod inlet;
mod outlet;
mod process_factory_impl;
mod process_factory_wrapper;
mod process_impl;
mod process_proxy;

pub use process_factory_wrapper::ProcessFactoryWrapper;
pub use process_proxy::ProcessProxy;

//pub use process_impl::process_from_clousure_new_with_class_name;
//pub use process_impl::process_new;
pub use process_factory_impl::ProcessFactoryImpl;
pub use process_impl::process_new;
pub(crate) use process_impl::ProcessImpl;
pub use composite_process_factory::CompositeProcessFactory;
