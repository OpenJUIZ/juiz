//! つながった System を再帰でたどるときの「通ってきた System」(issue #559)
//!
//! `*/list?recursive=true` と `system/profile?recursive=true` は、サブシステムだけでなく
//! マスターシステムもたどる。相手からはこちらもつながった System に見えるので、
//! 通ってきた System の UUID (`via`) を渡して戻らないようにする (Topic の #449 と同じ考え方)。
//!
//! 一覧 (`process_list` など) は BrokerProxy の引数を変えずに済むよう、`via` をスレッドローカルで運ぶ。
//! 受け側は CRUDBroker のコールバックを呼ぶ前に param の `via` をここへ置き ([`with_via`])、
//! CoreBroker は相手へ問い合わせる前に次の `via` を置く。BrokerProxy は送るときにここから読む ([`list_params`])。
//! コールバックも BrokerProxy の送信も同じスレッドで同期的に走るので、これで足りる。

use std::cell::RefCell;

use uuid::Uuid;

use crate::prelude::*;
use crate::topics::{via_from_param, via_to_param};

thread_local! {
    static WALK_VIA: RefCell<Vec<Uuid>> = const { RefCell::new(Vec::new()) };
}

/// いま置かれている `via`。置かれていなければ空
pub(crate) fn current_via() -> Vec<Uuid> {
    WALK_VIA.with(|v| v.borrow().clone())
}

/// `via` を置いて f を呼ぶ。戻ったら元に戻す
pub(crate) fn with_via<R>(via: &[Uuid], f: impl FnOnce() -> R) -> R {
    let prev = WALK_VIA.with(|v| std::mem::replace(&mut *v.borrow_mut(), via.to_vec()));
    let result = f();
    WALK_VIA.with(|v| *v.borrow_mut() = prev);
    result
}

/// param の `via` を読む (`,` 区切りの UUID)
pub(crate) fn via_from_args(args: &CapsuleMap) -> JuizResult<Vec<Uuid>> {
    via_from_param(args.get_param("via").map(|s| s.as_str()))
}

/// 一覧の問い合わせの param。recursive と、置かれていれば via
pub(crate) fn list_params(recursive: bool) -> Vec<(String, String)> {
    let mut params = vec![("recursive".to_owned(), recursive.to_string())];
    if recursive {
        let via = current_via();
        if !via.is_empty() {
            params.push(("via".to_owned(), via_to_param(&via)));
        }
    }
    params
}

/// System の profile (`system/profile?recursive=true` の答え) に入っている System の UUID を全部集める。
/// subsystems と mastersystem を入れ子までたどる
pub(crate) fn collect_system_uuids(profile: &Value, out: &mut Vec<Uuid>) {
    let uuid = profile
        .get("system_store")
        .and_then(|s| s.get("uuid"))
        .or_else(|| profile.get("system_uuid"))
        .and_then(|u| u.as_str())
        .and_then(|u| Uuid::parse_str(u).ok());
    if let Some(u) = uuid {
        if !out.contains(&u) {
            out.push(u);
        }
    }
    for key in ["mastersystem", "subsystems"] {
        if let Some(children) = profile.get(key).and_then(|c| c.as_array()) {
            for c in children {
                collect_system_uuids(c, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_sdk::value::jvalue;

    #[test]
    fn with_via_restores_previous() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        with_via(&[a], || {
            assert_eq!(current_via(), vec![a]);
            with_via(&[a, b], || assert_eq!(current_via(), vec![a, b]));
            assert_eq!(current_via(), vec![a]);
        });
        assert!(current_via().is_empty());
    }

    #[test]
    fn list_params_carry_via_only_when_recursive() {
        let a = Uuid::new_v4();
        with_via(&[a], || {
            assert_eq!(list_params(false).len(), 1);
            let p = list_params(true);
            assert_eq!(p[1], ("via".to_owned(), a.to_string()));
        });
        assert_eq!(list_params(true).len(), 1);
    }

    #[test]
    fn collect_uuids_walks_masters_and_subs() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let p = jvalue!({
            "system_store": {"uuid": a.to_string()},
            "mastersystem": [{"system_store": {"uuid": b.to_string()}, "subsystems": []}],
            "subsystems": [{"name": "x", "visited": true, "system_uuid": c.to_string()}],
        });
        let mut out = Vec::new();
        collect_system_uuids(&p, &mut out);
        assert_eq!(out, vec![a, b, c]);
    }
}
