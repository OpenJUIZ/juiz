//! ネーミングサービスの API (docs/spec/naming.md §9.4、issue #618)
//!
//! - `naming/list[?check=true]`: この System がいる機械の登録の一覧 (`juiz name list --json` と同じ形)
//! - `naming/resolve?name=<登録名>`: 名前に当てはまる、生きている候補をすべて (選ばない)
//!
//! 登録ファイルを読み、死活を確かめる。相手 (自分のこともある) に問い合わせるので、CoreBroker のロックは
//! 死活の確かめ方を取り出すときだけ持つ。
use juiz_sdk::{anyhow::anyhow, log, prelude::*};

use crate::naming::{ListedRegistration, NameQuery, NameRegistry};

use super::CallbackContainerType;

fn is_true(v: Option<&String>) -> bool {
    v.is_some_and(|v| matches!(v.as_str(), "true" | "1" | "yes"))
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |cb, args| {
        log::debug!("[READ  ] naming/list called");
        let (probe, kinds) = cb.lock()?.worker().naming_probe();
        let listed =
            NameRegistry::open()?.list(is_true(args.get_param("check")), probe.as_ref(), &kinds);
        let v: Vec<Value> = listed.iter().map(ListedRegistration::to_json).collect();
        Ok(jvalue!(v).into())
    });
    cbs.insert("resolve", |cb, args| {
        log::debug!("[READ  ] naming/resolve called");
        let name = args.get_param("name").cloned().ok_or_else(|| {
            anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "name".to_owned()
            })
        })?;
        let query = NameQuery::parse(&name)?;
        let (probe, kinds) = cb.lock()?.worker().naming_probe();
        let candidates = NameRegistry::open()?.candidates(&query, probe.as_ref(), &kinds);
        let v: Vec<Value> = candidates
            .into_iter()
            .map(|c| {
                ListedRegistration {
                    registration: c.registration,
                    alive: Some(true),
                }
                .to_json()
            })
            .collect();
        Ok(jvalue!(v).into())
    });
    cbs
}
