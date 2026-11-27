//! juiz_core の外で作ったブローカーを、プラグインとして読み込まずに System へ組み込むための登録口。
//!
//! juiz_app が QUIC (qmp) ブローカー(クレート juiz_qmp_broker)を組み込むのに使う(issue #63)。
//! juiz_qmp_broker は juiz_core に依存するので、juiz_core からは直接呼べない。
//! そこで使う側(juiz_app)が System を作る前にここへ登録し、System::setup() が
//! HTTP ブローカーと同じ順番でファクトリを作って登録する。

use std::sync::{Arc, Mutex};

use crate::prelude::*;
use juiz_sdk::anyhow;

/// ブローカーのファクトリを作る関数。プラグインの broker_factory シンボルと同じ形
pub type BrokerFactoryFunction = fn(CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>>;

/// ブローカープロキシのファクトリを作る関数。プラグインの broker_proxy_factory シンボルと同じ形
pub type BrokerProxyFactoryFunction = fn() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>;

static BUILTIN_BROKER_FACTORIES: Mutex<Vec<(BrokerFactoryFunction, BrokerProxyFactoryFunction)>> =
    Mutex::new(Vec::new());

/// これ以降に setup() するすべての System に、このブローカーの種類を組み込む。
///
/// 設定ファイルの `plugins.broker_factories` で同じ type_name のプラグインを読み込んだ場合は、
/// プラグインのほうを使い、組み込みのものは登録しない。
pub fn register_builtin_broker_factory(
    broker_factory: BrokerFactoryFunction,
    broker_proxy_factory: BrokerProxyFactoryFunction,
) {
    BUILTIN_BROKER_FACTORIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((broker_factory, broker_proxy_factory));
}

/// C ABI のブローカー (docs/spec/broker-plugin-abi.md) の入口 `broker_factory_cabi`
pub use juiz_sdk::broker_cabi::JuizBrokerEntryFn;

static BUILTIN_CABI_BROKERS: Mutex<Vec<JuizBrokerEntryFn>> = Mutex::new(Vec::new());

/// これ以降に setup() するすべての System に、C ABI のブローカーを組み込む (§6.4 の 2)。
/// entry はプラグインのクレートを rlib としてリンクしたときの `broker_factory_cabi`。
/// [`register_builtin_broker_factory`] と同じく、conf で同じ type_name のプラグインを読み込んだらそちらを使う
pub fn register_builtin_cabi_broker(entry: JuizBrokerEntryFn) {
    BUILTIN_CABI_BROKERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(entry);
}

pub(crate) fn builtin_cabi_brokers() -> Vec<JuizBrokerEntryFn> {
    BUILTIN_CABI_BROKERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub(crate) fn builtin_broker_factories() -> Vec<(BrokerFactoryFunction, BrokerProxyFactoryFunction)>
{
    BUILTIN_BROKER_FACTORIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// `juiz broker install <名前>` で入れられる種類 (docs/spec/broker-plugin-abi.md §10.3 の名前の表)。
/// 表そのもの (取得先のリポジトリ) は juiz_app が持つ。ここは案内の文を選ぶためだけに使う
pub const INSTALLABLE_BROKER_TYPES: &[&str] = &["zenoh"];

/// ブローカーの種類が見つからないときの案内 (broker-plugin-abi.md §6.4)。
/// qmp は juiz_app の feature (既定でオン) で組み込む種類なので、その案内を付ける
fn broker_factory_hint(type_name: &str) -> String {
    if type_name == "qmp" {
        return "The qmp broker is not built into this juiz. \
             Build juiz with the feature `qmp-broker` (on by default), \
             or load the plugin with `plugins.broker_factories.qmp_broker` in the conf."
            .to_owned();
    }
    if INSTALLABLE_BROKER_TYPES.contains(&type_name) {
        format!("The broker type '{type_name}' is not installed. Install it with 'juiz broker install {type_name}'.")
    } else {
        format!(
            "The broker type '{type_name}' is not built in or installed. \
             Load its plugin with plugins.broker_factories in the conf, \
             or install it with 'juiz broker install {type_name} --from <URL>'."
        )
    }
}

/// type_name のブローカーのファクトリが無いときのエラー。conf のプラグイン・`juiz broker install` の案内を付ける。
/// `downcast_ref::<JuizError>()` で元のエラー (`error`) も取り出せる。
pub(crate) fn broker_factory_not_found(type_name: &str, error: JuizError) -> anyhow::Error {
    let e = anyhow::Error::from(error);
    let message = format!("{e} {}", broker_factory_hint(type_name));
    e.context(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_for_zenoh() {
        let e = broker_factory_not_found(
            "zenoh",
            JuizError::BrokerFactoryCanNotFoundError { type_name: "zenoh".to_owned() },
        );
        let s = e.to_string();
        assert!(s.contains("BrokerFactory(zenoh) can not be found."), "{s}");
        assert!(
            s.contains("The broker type 'zenoh' is not installed. Install it with 'juiz broker install zenoh'."),
            "{s}"
        );
        assert!(matches!(
            e.downcast_ref::<JuizError>(),
            Some(JuizError::BrokerFactoryCanNotFoundError { .. })
        ));
    }

    #[test]
    fn hint_for_unknown_type() {
        let e = broker_factory_not_found(
            "foo",
            JuizError::FactoryCanNotFoundError { type_name: "foo".to_owned() },
        );
        let s = e.to_string();
        assert!(s.contains("plugins.broker_factories"), "{s}");
        assert!(s.contains("'juiz broker install foo --from <URL>'"), "{s}");
        assert!(broker_factory_not_found("qmp", JuizError::FactoryCanNotFoundError { type_name: "qmp".to_owned() })
            .to_string()
            .contains("qmp-broker"));
    }
}
