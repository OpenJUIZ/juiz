use juiz_sdk::anyhow::Context;

use crate::{
    brokers::{
        broker_factories_wrapper::BrokerFactoriesWrapper,
        builtin_broker_factories::{builtin_broker_factories, builtin_cabi_brokers},
        cabi::{cabi_broker_factories, load_builtin_entry},
    },
    prelude::*,
};

/// register_builtin_broker_factory() で登録されたブローカーのファクトリを System に登録する。
/// 同じ type_name のファクトリがプラグインで読み込み済みなら、そちらを優先して登録しない。
pub(super) fn setup_builtin_broker_factories(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_builtin_broker_factories() called");
    for (broker_factory, broker_proxy_factory) in builtin_broker_factories() {
        let bf = broker_factory(system.core_broker().clone())
            .context("creating builtin broker factory failed")?;
        let type_name = juiz_lock(&bf)?.type_name().to_string();
        if system.has_broker_factory(&type_name)? {
            log::info!("BrokerFactory(type_name={type_name:}) is already loaded as a plugin. The builtin one is not used.");
            continue;
        }
        let bpf = broker_proxy_factory().context("creating builtin broker proxy factory failed")?;
        system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, bf, bpf)?)?;
        log::info!("BrokerFactory(type_name={type_name:}) (builtin) Loaded");
    }
    // C ABI のブローカー (docs/spec/broker-plugin-abi.md §6.4 の 2)
    for entry_fn in builtin_cabi_brokers() {
        let entry = load_builtin_entry(entry_fn).context("loading builtin C ABI broker failed")?;
        let type_name = entry.type_name().to_owned();
        if system.has_broker_factory(&type_name)? {
            log::info!("BrokerFactory(type_name={type_name:}) is already loaded as a plugin. The builtin one is not used.");
            continue;
        }
        let (bf, bpf) = cabi_broker_factories(entry, system.core_broker().clone())?;
        system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, bf, bpf)?)?;
        log::info!("BrokerFactory(type_name={type_name:}) (builtin, C ABI) Loaded");
    }
    Ok(())
}
