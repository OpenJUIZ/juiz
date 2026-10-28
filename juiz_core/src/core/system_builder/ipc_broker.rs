use crate::{
    brokers::{
        broker_factories_wrapper::BrokerFactoriesWrapper,
        ipc::{ipc_broker_factory, ipc_broker_proxy_factory},
    },
    prelude::*,
};

/// IPC ブローカーのファクトリを登録する (feature "ipc-broker"、docs/spec/ipc-broker.md §6)。
/// ブローカーそのものは conf の brokers に type_name: ipc を書いたときに作る
pub(super) fn setup_ipc_broker_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_ipc_broker_factory() called");
    if system.has_broker_factory(&"ipc".to_owned())? {
        log::info!("BrokerFactory(type_name=ipc) is already loaded as a plugin. The builtin one is not used.");
        return Ok(());
    }
    let bf = ipc_broker_factory(system.core_broker().clone())?;
    let bpf = ipc_broker_proxy_factory()?;
    system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, bf, bpf)?)?;
    Ok(())
}
