//! juiz_core の外で作ったブローカーを、プラグインとして読み込まずに System へ組み込むための登録口。
//!
//! juiz_app が QUIC (qmp) ブローカー(クレート juiz_qmp_broker)を組み込むのに使う(issue #63)。
//! juiz_qmp_broker は juiz_core に依存するので、juiz_core からは直接呼べない。
//! そこで使う側(juiz_app)が System を作る前にここへ登録し、System::setup() が
//! HTTP ブローカーと同じ順番でファクトリを作って登録する。

use std::sync::{Arc, Mutex};

use crate::prelude::*;

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
