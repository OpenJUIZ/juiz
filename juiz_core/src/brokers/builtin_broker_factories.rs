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

pub(crate) fn builtin_broker_factories() -> Vec<(BrokerFactoryFunction, BrokerProxyFactoryFunction)>
{
    BUILTIN_BROKER_FACTORIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// juiz の feature やプラグインで足す種類のブローカーが見つからないときの案内。知らない種類なら None
fn broker_factory_hint(type_name: &str) -> Option<&'static str> {
    match type_name {
        // docs/spec/zenoh-broker.md §8.2
        "zenoh" => Some(
            "The zenoh broker is not built into this juiz. \
             Build juiz with the feature `zenoh-broker` (cargo install juiz_app --features zenoh-broker), \
             or load the plugin with `plugins.broker_factories.zenoh_broker` in the conf.",
        ),
        "qmp" => Some(
            "The qmp broker is not built into this juiz. \
             Build juiz with the feature `qmp-broker` (on by default), \
             or load the plugin with `plugins.broker_factories.qmp_broker` in the conf.",
        ),
        _ => None,
    }
}

/// type_name のブローカーのファクトリが無いときのエラー。feature やプラグインで足せる種類なら、その案内を付ける。
/// `downcast_ref::<JuizError>()` で元のエラー (`error`) も取り出せる。
pub(crate) fn broker_factory_not_found(type_name: &str, error: JuizError) -> anyhow::Error {
    let e = anyhow::Error::from(error);
    match broker_factory_hint(type_name) {
        Some(hint) => {
            let message = format!("{e} {hint}");
            e.context(message)
        }
        None => e,
    }
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
        assert!(s.contains("--features zenoh-broker"), "{s}");
        assert!(s.contains("plugins.broker_factories.zenoh_broker"), "{s}");
        assert!(matches!(
            e.downcast_ref::<JuizError>(),
            Some(JuizError::BrokerFactoryCanNotFoundError { .. })
        ));
    }

    #[test]
    fn no_hint_for_unknown_type() {
        let e = broker_factory_not_found(
            "foo",
            JuizError::FactoryCanNotFoundError { type_name: "foo".to_owned() },
        );
        assert!(!e.to_string().contains("feature"), "{e}");
    }
}
