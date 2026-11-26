//! ネーミングサービスの API (docs/spec/naming.md §9.4、issue #618)
//!
//! - `naming/list[?check=true][&lan=true]`: この System がいる機械の登録の一覧 (`juiz name list --json` と同じ形)。
//!   `lan=true` で LAN (mDNS、§11) で見つけたものも
//! - `naming/resolve?name=<登録名>[&lan=true]`: 名前に当てはまる、生きている候補をすべて (選ばない)。
//!   この機械で見つからなければ LAN も見る。`lan=true` なら両方
//! - peers (§12): `lan=true` と `resolve` は conf の `option.naming.peers`・`JUIZ_NAMING_PEERS` にも聞く。
//!   `naming/list?connected=true` (peer として聞かれたとき) は、つないでいる別の機械の System の登録も足す
//!
//! 登録ファイルを読み、死活を確かめる。相手 (自分のこともある) に問い合わせるので、CoreBroker のロックは
//! 死活の確かめ方を取り出すときだけ持つ。
use juiz_sdk::{anyhow::anyhow, log, prelude::*};

use crate::naming::peers::{connected_registrations, peers_of_manifest};
use crate::naming::{find_candidates, list_all, LanLookup, ListedRegistration, NameQuery, Place};

use super::CallbackContainerType;

fn is_true(v: Option<&String>) -> bool {
    v.is_some_and(|v| matches!(v.as_str(), "true" | "1" | "yes"))
}

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |cb, args| {
        log::debug!("[READ  ] naming/list called");
        let lan = is_true(args.get_param("lan"));
        let connected = is_true(args.get_param("connected"));
        let (probe, kinds, peers, systems) = {
            let c = cb.lock()?;
            let (probe, kinds) = c.worker().naming_probe();
            let peers = if lan {
                peers_of_manifest(c.worker().store().manifest_ref())
            } else {
                vec![]
            };
            let systems = if connected {
                c.connected_system_brokers()
            } else {
                vec![]
            };
            (probe, kinds, peers, systems)
        };
        let listed = list_all(
            is_true(args.get_param("check")),
            lan,
            &peers,
            probe.as_ref(),
            &kinds,
        )?;
        let mut v: Vec<Value> = listed.iter().map(ListedRegistration::to_json).collect();
        if connected {
            // 別の機械に http でつないだ System の登録 (§12.4)。この機械の一覧にあるものは足さない
            let skip = listed.iter().map(|l| l.registration.uuid.clone()).collect();
            for r in connected_registrations(&systems, &skip) {
                let mut e = ListedRegistration {
                    registration: r,
                    alive: None,
                    place: Place::Peer,
                }
                .to_json();
                if let Some(m) = e.as_object_mut() {
                    m.insert("found".to_owned(), jvalue!("connected"));
                }
                v.push(e);
            }
        }
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
        let (probe, kinds, peers) = {
            let c = cb.lock()?;
            let (probe, kinds) = c.worker().naming_probe();
            (
                probe,
                kinds,
                peers_of_manifest(c.worker().store().manifest_ref()),
            )
        };
        let lan = LanLookup::from_flag(is_true(args.get_param("lan")));
        let (candidates, _) = find_candidates(&query, probe.as_ref(), &kinds, lan, &peers)?;
        let v: Vec<Value> = candidates
            .into_iter()
            .map(|c| {
                ListedRegistration {
                    registration: c.registration,
                    alive: Some(true),
                    place: c.place,
                }
                .to_json()
            })
            .collect();
        Ok(jvalue!(v).into())
    });
    cbs
}
