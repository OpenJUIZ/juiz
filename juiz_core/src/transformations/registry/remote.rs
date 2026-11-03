//! System をまたぐ Transform (docs/spec/transformations.md §19、Step 1 #414)
//!
//! - 写しの始点 P': 別 System の Container P の代わりのノード。P' → C (C は自分の System の Container) が
//!   System をまたぐ Transform。P' は Container ではない (`container/list` に出ない)。
//! - 写しの Anchor B': P の PoseState の項目 B ごとに置く Anchor。B' の直下の子は P' で、
//!   その辺 B' → P' (`imported` の Transform) の値が取り込んだ pose_B(P)。P 自身が Anchor なら P' = B'。
//! - 取り込み: 写しの始点ごとのスレッドが、相手の System の `container/pose_state` を [`PoseStateFetcher`] で呼ぶ。
//!   通信は登録所のロックの外 (別のスレッド) で行い、値を当てはめるときだけ登録所のロックを取る (§19.4)。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::geometry::{Pose, Time};
use juiz_sdk::manifests::{transform_identifier, AnchorManifest};
use juiz_sdk::transformations::ContainerPoseHandle;

use super::super::api_types::{PoseStateQuery, PoseStateResponse};
use super::super::tree::{compute_tree, out_edges, RemoteParams, TransformEntry, Tree};
use super::{
    cross_system_error, fire, is_not_found, not_found, now_after, remove_entry, write_entry,
    AnchorEntry, Inner, Node, Transformations,
};
use crate::prelude::*;

/// 相手の System の `container/pose_state` を呼ぶ口 (§19.4)。
///
/// System は BrokerProxy で呼ぶものを置く。テストでは同じプロセスの別の登録所に回すものを置ける。
/// 登録所のロックの外 (取り込み用の別スレッド) から呼ばれる。
pub trait PoseStateFetcher: Send + Sync {
    /// `source` は写しの始点 P' の ID (ブローカー付き)。`query.identifier` は相手の System での core の ID
    fn fetch(&self, source: &ContainerIdentifier, query: &PoseStateQuery) -> JuizResult<Value>;
}

impl<F> PoseStateFetcher for F
where
    F: Fn(&ContainerIdentifier, &PoseStateQuery) -> JuizResult<Value> + Send + Sync,
{
    fn fetch(&self, source: &ContainerIdentifier, query: &PoseStateQuery) -> JuizResult<Value> {
        self(source, query)
    }
}

/// 写しのノード (P'・B') の、相手の System での姿 (§19.5)
pub(super) struct RemoteNode {
    /// 相手の System (B' なら Anchor を本当に持つ System) での core の ID
    pub origin: ContainerIdentifier,
    /// その System の UUID (P' は最初に取り込むまで分からない)
    pub origin_system: Option<String>,
}

/// 写しの Anchor B' (§19.3)
pub(super) struct ReplicaAnchor {
    pub origin_system: String,
    pub origin_anchor: String,
    /// 直下の子の写しの始点 P' (P' = B' のときは自分も入る)
    pub children: BTreeSet<String>,
}

/// Container の PoseState を取りに来ている別 System (`container/profile_full` の `remote_children`、§19.10)
pub(super) struct Subscriber {
    destinations: Vec<String>,
    last_access: Instant,
    last_access_time: Time,
    timeout: f64,
}

/// つながらなくなって消した写しの Anchor (§19.6 のエラー文用)
pub(super) struct LostAnchor {
    pub origin: ContainerIdentifier,
    /// 取り込んでいた写しの始点
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteState {
    /// 取れている
    Ok,
    /// 失敗中 (timeout 前、最後の値を持っている)
    Stale,
    /// 写しを消した
    Lost,
}

impl RemoteState {
    fn as_str(&self) -> &'static str {
        match self {
            RemoteState::Ok => "ok",
            RemoteState::Stale => "stale",
            RemoteState::Lost => "lost",
        }
    }
}

/// 写しの始点 P' ごとの取り込みの状態 (§19.4・§19.6)
pub(super) struct RemoteSource {
    /// P' の ID (ブローカー付き)
    identifier: ContainerIdentifier,
    /// 相手の System での P の ID (core の ID)
    origin: ContainerIdentifier,
    /// いちばん大きい rate [Hz]
    rate: f64,
    /// いちばん小さい timeout [秒]
    timeout: f64,
    pub thread: Option<ImportThread>,
    remote_system: Option<String>,
    last_version: Option<u64>,
    /// この P' から取り込んでいる写しの Anchor B' の ID → 取り込んだ項目の版番号
    anchors: BTreeMap<String, u64>,
    /// 取り込みを始めた時刻 (1 度も取れないときの timeout の起点)
    started_at: Instant,
    last_success: Option<(Instant, Time)>,
    failing: bool,
    state: RemoteState,
}

fn core_form(id: &ContainerIdentifier) -> ContainerIdentifier {
    let mut c = id.clone();
    c.broker_type_name = "core".to_owned();
    c.broker_name = "core".to_owned();
    c
}

fn is_core(id: &ContainerIdentifier) -> bool {
    id.broker_type_name == "core"
}

impl RemoteSource {
    pub fn new(identifier: &ContainerIdentifier) -> Self {
        Self {
            identifier: identifier.clone(),
            origin: core_form(identifier),
            rate: juiz_sdk::manifests::DEFAULT_REMOTE_RATE,
            timeout: juiz_sdk::manifests::DEFAULT_REMOTE_TIMEOUT,
            thread: None,
            remote_system: None,
            last_version: None,
            anchors: BTreeMap::new(),
            started_at: Instant::now(),
            last_success: None,
            failing: false,
            state: RemoteState::Stale,
        }
    }

    fn period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.rate)
    }

    /// `transform/profile_full` の `remote` (§19.10)
    pub fn profile(&self, params: &RemoteParams) -> Value {
        jvalue!({
            "system": self.remote_system,
            "rate": params.rate,
            "timeout": params.timeout,
            "state": self.state.as_str(),
            "last_success": self.last_success.map(|(_, t)| Value::from(t)),
        })
    }

    fn describe_system(&self) -> String {
        match &self.remote_system {
            Some(uuid) => format!("{} (uuid={uuid})", self.identifier.broker_name),
            None => self.identifier.broker_name.clone(),
        }
    }
}

// ---- 取り込みのスレッド ----

enum ImportMsg {
    Stop,
    /// すぐ取りに行く (`anchor/update`、§19.10)
    Wake,
    Fetched(u64, JuizResult<Value>),
}

pub(super) struct ImportThread {
    tx: mpsc::Sender<ImportMsg>,
    handle: JoinHandle<()>,
}

impl ImportThread {
    pub fn wake(&self) {
        let _ = self.tx.send(ImportMsg::Wake);
    }
}

/// 取り込みのスレッドを止める。呼び出しの最中のものは待たない (別スレッドで呼んでいるので)
pub(super) fn join_imports(threads: Vec<ImportThread>) {
    let me = std::thread::current().id();
    for t in threads.into_iter() {
        let _ = t.tx.send(ImportMsg::Stop);
        if t.handle.thread().id() != me {
            let _ = t.handle.join();
        }
    }
}

/// 1 回の取り込みに要るもの (登録所のロックの中で集める)
struct ImportRequest {
    source: ContainerIdentifier,
    query: PoseStateQuery,
    timeout: Duration,
    fetcher: Option<Arc<dyn PoseStateFetcher>>,
}

/// 相手での Anchor の ID を、P' のブローカーで書き換える (§19.5)。ブローカー付き (相手の写し) ならそのまま
fn replica_anchor_id(source: &ContainerIdentifier, anchor: &str) -> JuizResult<String> {
    let mut a = ContainerIdentifier::try_from(anchor.to_owned())?;
    if is_core(&a) {
        a.broker_type_name = source.broker_type_name.clone();
        a.broker_name = source.broker_name.clone();
    }
    Ok(a.to_string())
}

/// 名前 (`<path>` か `<path>::<type_name>`) か ID が、相手の System での ID `origin` に当たるか (§19.5 の 2)
fn matches_origin(origin: &ContainerIdentifier, reference: &str, namespace: &str) -> bool {
    if reference.contains("://") {
        return ContainerIdentifier::try_from(reference.to_owned())
            .map(|r| r.to_string() == origin.to_string())
            .unwrap_or(false);
    }
    let (path, type_name) = match reference.split_once("::") {
        Some((p, t)) => (p, Some(t)),
        None => (reference, None),
    };
    let Ok(full) = juiz_sdk::topic_identifier::resolve_topic_name(namespace, path) else {
        return false;
    };
    let (ns, name) = juiz_sdk::identifier::split_namespace(&full);
    origin.namespace == ns
        && origin.name == name
        && type_name.map_or(true, |t| origin.type_name == t)
}

impl Inner {
    // ---- 名前の解決 (§19.5) ----

    /// 写しの Anchor・写しの始点から探す (§19.5 の 2・3)
    pub(super) fn resolve_remote(&self, reference: &str, namespace: &str) -> JuizResult<String> {
        if reference.contains("://") {
            let id = ContainerIdentifier::try_from(reference.to_owned())?.to_string();
            if self.nodes.get(&id).is_some_and(|n| n.is_remote()) {
                return Ok(id);
            }
        }
        let found = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.remote
                    .as_ref()
                    .is_some_and(|r| matches_origin(&r.origin, reference, namespace))
            })
            .map(|(k, _)| k.clone())
            .collect::<Vec<String>>();
        match found.len() {
            0 => Err(not_found(reference)),
            1 => Ok(found.into_iter().next().unwrap()),
            _ => Err(anyhow!(JuizError::InvalidValueError {
                message: format!(
                    "Replica name '{reference}' is ambiguous ({found:?}). Use the identifier instead."
                ),
            })),
        }
    }

    /// 写しのうち、相手の System が `system` (UUID) のものだけから探す (§19.5 の 4)。
    /// 自分の System の Container は探さない
    pub(super) fn resolve_in_system(
        &self,
        reference: &str,
        namespace: &str,
        system: &str,
    ) -> JuizResult<String> {
        let found = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.remote.as_ref().is_some_and(|r| {
                    r.origin_system.as_deref() == Some(system)
                        && matches_origin(&r.origin, reference, namespace)
                })
            })
            .map(|(k, _)| k.clone())
            .collect::<Vec<String>>();
        match found.len() {
            0 => Err(anyhow!(JuizError::NoCommonAnchorError {
                container: reference.to_owned(),
                reference: reference.to_owned(),
            })
            .context(format!(
                "this System has no replica of Container({reference}) of the System (uuid={system}). \
                 Only the Containers imported as replicas can be the reference (§19.8)."
            ))),
            1 => Ok(found.into_iter().next().unwrap()),
            _ => Err(anyhow!(JuizError::InvalidValueError {
                message: format!(
                    "Replica name '{reference}' is ambiguous ({found:?}). Use the identifier instead."
                ),
            })),
        }
    }

    /// Transform の始点がブローカー付きの、別 System の Container の ID なら、その ID (§19.2 の 2)
    pub(super) fn remote_source_of(&self, source: &str) -> JuizResult<Option<ContainerIdentifier>> {
        if !source.contains("://") {
            return Ok(None);
        }
        let id = ContainerIdentifier::try_from(source.to_owned())?;
        if is_core(&id) {
            return Ok(None);
        }
        if id.class_name != "container" {
            return Err(cross_system_error(format!(
                "source of a Transform must be a Container, but {source}"
            )));
        }
        if let Some(n) = self.nodes.get(&id.to_string()) {
            if !n.is_remote() {
                return Err(cross_system_error(format!(
                    "Container ({source}) is not on another System."
                )));
            }
        }
        Ok(Some(id))
    }

    /// 写しの始点 P' のノードを置く。新しく置いたら true
    pub(super) fn add_source_node(&mut self, identifier: &ContainerIdentifier) -> bool {
        let id = identifier.to_string();
        if self.nodes.contains_key(&id) {
            return false;
        }
        self.nodes.insert(
            id,
            Node {
                identifier: identifier.clone(),
                handle: ContainerPoseHandle::new(identifier.clone()),
                remote: Some(RemoteNode {
                    origin: core_form(identifier),
                    origin_system: None,
                }),
            },
        );
        true
    }

    /// Transform の端点 (始点が自分の System のとき・終点) を自分の System の Container に解決する。
    /// 別 System の Container・写しなら `CrossSystemTransformError` (§19.2 の 6・§19.3)
    pub(super) fn resolve_endpoint(
        &self,
        reference: &str,
        namespace: &str,
        what: &str,
    ) -> JuizResult<String> {
        match self.resolve_local(reference, namespace) {
            Ok(id) => Ok(id),
            Err(e) if is_not_found(&e) => {
                if reference.contains("://") {
                    if let Ok(id) = ContainerIdentifier::try_from(reference.to_owned()) {
                        if !is_core(&id) {
                            return Err(cross_system_error(format!(
                                "{what} ({reference}) is a Container of another System. \
                                 A Transform across Systems is owned by the System of its destination, \
                                 and its source is the Container of another System (§19.2)."
                            )));
                        }
                    }
                }
                if self.resolve_remote(reference, namespace).is_ok() {
                    return Err(cross_system_error(format!(
                        "{what} ({reference}) is a replica of a Container of another System. \
                         Write the identifier with the broker to make a Transform across Systems (§19.2)."
                    )));
                }
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    // ---- 取り込みの設定 ----

    /// 写しの始点ごとに、それを始点にする Transform のいちばん大きい rate・いちばん小さい timeout を使う (§19.4)
    pub(super) fn refresh_source_params(&mut self) {
        let Inner {
            sources,
            transforms,
            ..
        } = self;
        for (p, src) in sources.iter_mut() {
            let params = transforms
                .values()
                .filter(|t| !t.imported && &t.source == p)
                .filter_map(|t| t.remote)
                .collect::<Vec<RemoteParams>>();
            if params.is_empty() {
                continue;
            }
            src.rate = params.iter().map(|p| p.rate).fold(f64::MIN, f64::max);
            src.timeout = params.iter().map(|p| p.timeout).fold(f64::MAX, f64::min);
        }
    }

    /// それを始点にする Transform がなくなった写しの始点を外し、止めるスレッドを返す (§19.6)
    pub(super) fn remove_unused_sources(&mut self) -> Vec<ImportThread> {
        let unused = self
            .sources
            .keys()
            .filter(|p| {
                !self
                    .transforms
                    .values()
                    .any(|t| !t.imported && &t.source == *p)
            })
            .cloned()
            .collect::<Vec<String>>();
        let mut threads = Vec::new();
        for p in unused.iter() {
            self.drop_replicas(p, None);
            if let Some(src) = self.sources.remove(p) {
                threads.extend(src.thread);
            }
            self.lost.retain(|_, l| &l.source != p);
            if !self.anchors.contains_key(p) {
                self.nodes.remove(p);
            }
        }
        if !unused.is_empty() {
            if let Err(e) = self.rebuild_trees() {
                log::error!("rebuilding Transform trees after removing replicas failed: {e}");
            }
        }
        threads
    }

    // ---- 写しの Anchor ----

    fn ensure_replica_anchor(&mut self, bid: &str, key: &(String, String)) -> JuizResult<()> {
        if let Some(a) = self.anchors.get(bid) {
            return match a.remote {
                Some(_) => Ok(()),
                None => Err(cross_system_error(format!(
                    "replica Anchor ({bid}) collides with an Anchor of this System."
                ))),
            };
        }
        match self.nodes.get_mut(bid) {
            Some(n) => match n.remote.as_mut() {
                Some(r) => {
                    r.origin_system.get_or_insert_with(|| key.0.clone());
                }
                None => {
                    return Err(cross_system_error(format!(
                        "replica Anchor ({bid}) collides with a Container of this System."
                    )))
                }
            },
            None => {
                let identifier = ContainerIdentifier::try_from(bid.to_owned())?;
                let origin = ContainerIdentifier::try_from(key.1.clone())?;
                self.nodes.insert(
                    bid.to_owned(),
                    Node {
                        identifier: identifier.clone(),
                        handle: ContainerPoseHandle::new(identifier),
                        remote: Some(RemoteNode {
                            origin,
                            origin_system: Some(key.0.clone()),
                        }),
                    },
                );
            }
        }
        self.anchors.insert(
            bid.to_owned(),
            AnchorEntry {
                manifest: AnchorManifest::static_anchor(),
                tree: Tree::default(),
                cache: HashMap::new(),
                thread: None,
                remote: Some(ReplicaAnchor {
                    origin_system: key.0.clone(),
                    origin_anchor: key.1.clone(),
                    children: BTreeSet::new(),
                }),
            },
        );
        self.replica_index.insert(key.clone(), bid.to_owned());
        self.lost.remove(bid);
        Ok(())
    }

    /// 写しの Anchor `bid` を消す。PoseState からも消す (§10 の Anchor の削除と同じ)
    fn remove_replica_anchor(&mut self, bid: &str) -> Option<ContainerIdentifier> {
        let a = self.anchors.remove(bid)?;
        let r = a.remote?;
        for n in self.nodes.values() {
            remove_entry(&n.handle, bid);
        }
        self.transforms
            .retain(|_, t| !(t.imported && t.source == bid));
        self.replica_index
            .remove(&(r.origin_system.clone(), r.origin_anchor.clone()));
        let origin = self
            .nodes
            .get(bid)
            .and_then(|n| n.remote.as_ref())
            .map(|r| r.origin.clone());
        if !self.sources.contains_key(bid) {
            self.nodes.remove(bid);
        }
        origin
    }

    /// P' を B' の直下の子から外す。B' に子がなくなれば消し、その相手での ID を返す
    fn detach(&mut self, p: &str, bid: &str) -> Option<ContainerIdentifier> {
        if let Some(s) = self.sources.get_mut(p) {
            s.anchors.remove(bid);
        }
        if bid != p {
            self.transforms.remove(&transform_identifier(bid, p));
        }
        let r = self.anchors.get_mut(bid)?.remote.as_mut()?;
        r.children.remove(p);
        if !r.children.is_empty() {
            return None;
        }
        self.remove_replica_anchor(bid)
    }

    /// P' からの写しをすべて消す (§19.6)。`reason` があれば、消した B' をエラー文用に覚える
    fn drop_replicas(&mut self, p: &str, reason: Option<&str>) {
        let anchors = match self.sources.get_mut(p) {
            Some(s) => {
                s.last_version = None;
                s.anchors.keys().cloned().collect::<Vec<String>>()
            }
            None => return,
        };
        for bid in anchors.iter() {
            if let Some(origin) = self.detach(p, bid) {
                if let Some(reason) = reason {
                    self.lost.insert(
                        bid.clone(),
                        LostAnchor {
                            origin,
                            source: p.to_owned(),
                            reason: reason.to_owned(),
                        },
                    );
                }
            }
        }
        if let Err(e) = self.rebuild_trees() {
            log::error!("rebuilding Transform trees after dropping replicas of {p} failed: {e}");
        }
    }

    /// 木の規則 (§5.2) を破る写しの Anchor を作らない (WARN は 1 回)。そのあと木を作り直す
    fn rebuild_trees_dropping_bad_replicas(&mut self) {
        loop {
            let bad = {
                let out = out_edges(&self.transforms);
                self.anchors
                    .iter()
                    .filter(|(_, a)| a.remote.is_some())
                    .filter_map(|(id, _)| compute_tree(id, &out).err().map(|e| (id.clone(), e)))
                    .collect::<Vec<(String, anyhow::Error)>>()
            };
            if bad.is_empty() {
                break;
            }
            for (bid, e) in bad.into_iter() {
                let Some(r) = self.anchors.get(&bid).and_then(|a| a.remote.as_ref()) else {
                    continue;
                };
                let key = (r.origin_system.clone(), r.origin_anchor.clone());
                let children = r.children.clone();
                if self.rejected.insert(key) {
                    log::warn!(
                        "replica Anchor ({bid}) is not made because its tree breaks the rule: {e}"
                    );
                }
                for p in children.iter() {
                    if let Some(s) = self.sources.get_mut(p) {
                        s.anchors.remove(&bid);
                        // 木の形が変われば作り直せるよう、次の取り込みは全部取る
                        s.last_version = None;
                    }
                    if &bid != p {
                        self.transforms.remove(&transform_identifier(&bid, p));
                    }
                }
                self.remove_replica_anchor(&bid);
            }
        }
        if let Err(e) = self.rebuild_trees() {
            log::error!("rebuilding Transform trees after importing failed: {e}");
        }
        let index = &self.replica_index;
        self.rejected.retain(|k| !index.contains_key(k));
    }

    // ---- 取り込みの結果 ----

    /// 取り込めた値を当てはめる (§19.3・§19.4)。値が変わった Container を返す
    fn import_succeeded(
        &mut self,
        p: &str,
        resp: PoseStateResponse,
    ) -> JuizResult<BTreeSet<String>> {
        let my = self.system_uuid.clone();
        let Some(src) = self.sources.get_mut(p) else {
            return Ok(BTreeSet::new());
        };
        if src.failing {
            log::info!("Importing poses from Container({p}) of another System is restored.");
            src.failing = false;
        }
        src.state = RemoteState::Ok;
        src.last_success = Some((Instant::now(), Time::now()));
        src.remote_system = Some(resp.system.clone());
        let source_id = src.identifier.clone();
        if !resp.changed {
            return Ok(BTreeSet::new());
        }
        src.last_version = Some(resp.version);
        if let Some(r) = self.nodes.get_mut(p).and_then(|n| n.remote.as_mut()) {
            r.origin_system = Some(resp.system.clone());
        }

        let mut desired = BTreeMap::new();
        for e in resp.entries.into_iter() {
            // 回ってきた自分の Anchor の値は取り込まない (循環、§19.5)
            if e.origin_system == my {
                continue;
            }
            let key = (e.origin_system.clone(), e.origin_anchor.clone());
            let bid = match self.replica_index.get(&key) {
                Some(b) => b.clone(),
                None => replica_anchor_id(&source_id, &e.anchor)?,
            };
            desired.insert(bid, (key, e));
        }
        let old = self.sources[p]
            .anchors
            .keys()
            .filter(|b| !desired.contains_key(*b))
            .cloned()
            .collect::<Vec<String>>();
        let mut structural = !old.is_empty();
        for b in old.iter() {
            self.detach(p, b);
        }
        let mut touched = BTreeSet::new();
        for (bid, (key, e)) in desired.into_iter() {
            let prev = self.sources[p].anchors.get(&bid).copied();
            if prev == Some(e.version) && self.anchors.contains_key(&bid) {
                continue;
            }
            if !self.anchors.contains_key(&bid) {
                if let Err(err) = self.ensure_replica_anchor(&bid, &key) {
                    log::warn!("replica Anchor ({bid}) is not made: {err}");
                    continue;
                }
                structural = true;
            }
            if bid == p {
                // P 自身が Anchor: P' がそのまま写しの Anchor (§19.3)
                write_entry(&self.nodes[p].handle, &bid, Pose::identity(), e.stamp);
            } else {
                let tid = transform_identifier(&bid, p);
                let value = e.pose.to_transform().normalized()?;
                let version = self.next_version();
                match self.transforms.get_mut(&tid) {
                    Some(t) => {
                        t.value = value;
                        t.stamp = e.stamp;
                        t.version = version;
                    }
                    None => {
                        self.transforms.insert(
                            tid,
                            TransformEntry {
                                source: bid.clone(),
                                destination: p.to_owned(),
                                value,
                                stamp: e.stamp,
                                version,
                                imported: true,
                                remote: None,
                            },
                        );
                        structural = true;
                    }
                }
            }
            if let Some(r) = self.anchors.get_mut(&bid).and_then(|a| a.remote.as_mut()) {
                r.children.insert(p.to_owned());
            }
            if let Some(s) = self.sources.get_mut(p) {
                s.anchors.insert(bid.clone(), e.version);
            }
            touched.insert(bid);
        }
        if structural {
            self.rebuild_trees_dropping_bad_replicas();
        }
        let mut changed = BTreeSet::new();
        for bid in touched.iter() {
            if self.anchors.contains_key(bid) {
                changed.extend(self.update_anchor(bid)?);
            }
        }
        Ok(changed)
    }

    /// 取り込めなかった (§19.6)。最後の値を持ち続け、timeout を過ぎたら写しを消す
    fn import_failed(&mut self, p: &str, e: &anyhow::Error) {
        let Some(src) = self.sources.get_mut(p) else {
            return;
        };
        src.last_version = None;
        if !src.failing {
            log::warn!(
                "Importing poses from Container({p}) of another System failed. The last values are kept for {}s: {e}",
                src.timeout
            );
            src.failing = true;
        } else {
            log::debug!("Importing poses from Container({p}) failed: {e}");
        }
        let since = src.last_success.map(|(i, _)| i).unwrap_or(src.started_at);
        if src.state != RemoteState::Lost && since.elapsed().as_secs_f64() >= src.timeout {
            src.state = RemoteState::Lost;
            let reason = format!(
                "the System {} that has Container({p}) is not reachable",
                src.describe_system()
            );
            log::debug!("Replicas imported from Container({p}) are dropped: {reason}");
            self.drop_replicas(p, Some(&reason));
        } else if src.state == RemoteState::Ok {
            src.state = RemoteState::Stale;
        }
    }

    // ---- 読み出し ----

    /// 基準 `name` が、消した写し・つながらない写しの始点に当たれば、その理由 (§19.6)
    fn unreachable_reason(&self, name: &str, namespace: &str) -> Option<String> {
        if let Some((_, l)) = self
            .lost
            .iter()
            .find(|(id, l)| *id == name || matches_origin(&l.origin, name, namespace))
        {
            return Some(l.reason.clone());
        }
        let id = self.resolve(name, namespace).ok()?;
        let src = self.sources.get(&id)?;
        if src.state == RemoteState::Ok {
            return None;
        }
        if src.last_success.is_none() && src.state != RemoteState::Lost {
            return Some(format!(
                "poses of Container({id}) have not been imported from {} yet",
                src.describe_system()
            ));
        }
        Some(format!(
            "the System {} that has Container({id}) is not reachable",
            src.describe_system()
        ))
    }

    /// 名前を解決して pose を読む。写しの相手につながらないときは `NoCommonAnchorError` に理由を足す (§19.6)
    pub(super) fn pose_explained(
        &self,
        container: &str,
        reference: &str,
        namespace: &str,
    ) -> JuizResult<juiz_sdk::geometry::PoseStamped> {
        let result = (|| {
            let c = self.resolve(container, namespace)?;
            let r = self.resolve(reference, namespace)?;
            self.pose(&c, &r)
        })();
        result.map_err(|e| {
            let relevant = is_not_found(&e)
                || matches!(
                    e.downcast_ref::<JuizError>(),
                    Some(JuizError::NoCommonAnchorError { .. })
                );
            if !relevant {
                return e;
            }
            match [reference, container]
                .iter()
                .find_map(|n| self.unreachable_reason(n, namespace))
            {
                Some(reason) => anyhow!(JuizError::NoCommonAnchorError {
                    container: container.to_owned(),
                    reference: reference.to_owned(),
                })
                .context(reason),
                None => e,
            }
        })
    }

    /// 取りに来た System を覚える (§19.10 の `remote_children`)
    fn record_subscriber(&mut self, id: &str, query: &PoseStateQuery) {
        let Some(uuid) = query.subscriber.as_ref() else {
            return;
        };
        let timeout = query
            .timeout
            .filter(|t| t.is_finite() && *t > 0.0)
            .unwrap_or(juiz_sdk::manifests::DEFAULT_REMOTE_TIMEOUT);
        self.subscribers.entry(id.to_owned()).or_default().insert(
            uuid.clone(),
            Subscriber {
                destinations: query.destination.clone(),
                last_access: Instant::now(),
                last_access_time: Time::now(),
                timeout,
            },
        );
    }

    /// `container/profile_full` の `remote_children` (§19.10)。最後に来てから timeout を過ぎたものは消す
    fn remote_children(&mut self, id: &str) -> Option<Value> {
        let subs = self.subscribers.get_mut(id)?;
        subs.retain(|_, s| s.last_access.elapsed().as_secs_f64() < s.timeout);
        if subs.is_empty() {
            self.subscribers.remove(id);
            return None;
        }
        Some(Value::Array(
            subs.iter()
                .map(|(uuid, s)| {
                    jvalue!({
                        "system": uuid,
                        "destinations": s.destinations,
                        "last_access": Value::from(s.last_access_time),
                    })
                })
                .collect(),
        ))
    }

    /// `container/pose_state` (§19.4)
    fn pose_state(&mut self, query: &PoseStateQuery, namespace: &str) -> JuizResult<Value> {
        let id = self.resolve(&query.identifier, namespace)?;
        let node = self.node(&id)?;
        if node.is_remote() {
            return Err(cross_system_error(format!(
                "Container ({id}) is a replica of a Container of another System. Ask that System."
            )));
        }
        let handle = node.handle.clone();
        self.record_subscriber(&id, query);
        let state = handle.state();
        let state = state.read().map_err(|_| super::lock_error())?;
        let version = state.last_version();
        let same_system = query
            .system
            .as_deref()
            .map_or(true, |s| s == self.system_uuid);
        let changed = !(same_system && query.since == Some(version));
        let mut entries = Vec::new();
        if changed {
            for (aid, e) in state.iter() {
                let (origin_system, origin_anchor) =
                    match self.anchors.get(aid).and_then(|a| a.remote.as_ref()) {
                        Some(r) => (r.origin_system.clone(), r.origin_anchor.clone()),
                        None => (self.system_uuid.clone(), aid.clone()),
                    };
                entries.push(jvalue!({
                    "anchor": aid,
                    "origin_system": origin_system,
                    "origin_anchor": origin_anchor,
                    "pose": serde_json::to_value(e.pose)?,
                    "stamp": serde_json::to_value(e.stamp)?,
                    "version": e.version,
                }));
            }
        }
        Ok(jvalue!({
            "identifier": id,
            "system": self.system_uuid,
            "version": version,
            "changed": changed,
            "entries": entries,
        }))
    }
}

impl Transformations {
    /// System の UUID を置く (CoreWorker が作るとき、§19.4)
    pub fn set_system_uuid(&self, uuid: &str) -> JuizResult<()> {
        self.lock()?.system_uuid = uuid.to_owned();
        Ok(())
    }

    pub fn system_uuid(&self) -> JuizResult<String> {
        Ok(self.lock()?.system_uuid.clone())
    }

    /// 相手の System の `container/pose_state` を呼ぶ口を置く (§19.4)
    pub fn set_pose_state_fetcher(&self, fetcher: Arc<dyn PoseStateFetcher>) -> JuizResult<()> {
        *self.fetcher.lock().map_err(|_| super::lock_error())? = Some(fetcher);
        Ok(())
    }

    /// `container/pose_state` (§19.4)。Container `query.identifier` の PoseState を版番号つきで返す。
    /// `query.subscriber` があれば、取りに来た System として覚える (§19.10)
    pub fn pose_state(&self, query: &PoseStateQuery, namespace: &str) -> JuizResult<Value> {
        self.lock()?.pose_state(query, namespace)
    }

    /// Container `identifier` (core の ID) の写しを取りに来ている System の一覧 (§19.10)。いなければ `None`
    pub fn remote_children(&self, identifier: &str) -> JuizResult<Option<Value>> {
        Ok(self.lock()?.remote_children(identifier))
    }

    /// 自分の System の Container の名前か ID を core の ID にする (§19.2 の 5 で回すときの始点)
    pub fn resolve_local_id(&self, reference: &str, namespace: &str) -> JuizResult<String> {
        self.lock()?.resolve_local(reference, namespace)
    }

    /// 名前か ID が、自分の System の Container か写しに当たるか (§19.5 の 1・2)
    pub fn knows(&self, reference: &str, namespace: &str) -> JuizResult<bool> {
        Ok(self.lock()?.resolve(reference, namespace).is_ok())
    }

    /// [`Transformations::pose_by_name`] の、基準 `reference` を System `system` の写しだけから探すもの (§19.8)
    pub fn pose_by_name_from_system(
        &self,
        container: &str,
        reference: &str,
        namespace: &str,
        system: &str,
    ) -> JuizResult<juiz_sdk::geometry::PoseStamped> {
        let inner = self.lock()?;
        let c = inner.resolve(container, namespace)?;
        let r = inner.resolve_in_system(reference, namespace, system)?;
        inner.pose_explained(&c, &r, namespace)
    }

    /// [`Transformations::set_pose_by_name`] の、基準 `reference` を System `system` の写しだけから探すもの (§19.8)
    pub fn set_pose_by_name_from_system(
        &self,
        container: &str,
        reference: &str,
        namespace: &str,
        system: &str,
        pose: &Pose,
    ) -> JuizResult<juiz_sdk::geometry::PoseStamped> {
        let (c, r) = {
            let inner = self.lock()?;
            (
                inner.resolve(container, namespace)?,
                inner.resolve_in_system(reference, namespace, system)?,
            )
        };
        self.set_pose_ids(&c, &r, pose)
    }

    /// `system/remove_subsystem` で System `uuid` が外れたとき、その System からの写しを消す (§19.6)。
    /// Transform は残し、取り込みは続ける (つながり直せば戻る)
    pub fn remove_remote_system(&self, uuid: &str) -> JuizResult<()> {
        let mut inner = self.lock()?;
        let targets = inner
            .sources
            .iter()
            .filter(|(_, s)| s.remote_system.as_deref() == Some(uuid))
            .map(|(p, _)| p.clone())
            .collect::<Vec<String>>();
        for p in targets.iter() {
            let reason = format!("the System (uuid={uuid}) that has Container({p}) was removed");
            if let Some(s) = inner.sources.get_mut(p) {
                s.state = RemoteState::Lost;
                s.last_success = None;
                s.started_at = Instant::now();
            }
            inner.drop_replicas(p, Some(&reason));
        }
        Ok(())
    }

    /// まだスレッドのない写しの始点の取り込みを始める
    pub(super) fn start_imports(&self, inner: &mut Inner) -> JuizResult<()> {
        let pending = inner
            .sources
            .iter()
            .filter(|(_, s)| s.thread.is_none())
            .map(|(p, _)| p.clone())
            .collect::<Vec<String>>();
        for p in pending.into_iter() {
            let t = self.spawn_import(&p)?;
            let s = inner.sources.get_mut(&p).unwrap();
            s.started_at = Instant::now();
            s.thread = Some(t);
        }
        Ok(())
    }

    fn spawn_import(&self, p: &str) -> JuizResult<ImportThread> {
        let (tx, rx) = mpsc::channel::<ImportMsg>();
        let me = self.me.clone();
        let source = p.to_owned();
        let fetch_tx = tx.clone();
        let handle = std::thread::Builder::new()
            .name("pose_state_import".to_owned())
            .spawn(move || {
                let mut next = Instant::now();
                let mut in_flight: Option<(u64, Instant)> = None;
                let mut seq = 0u64;
                loop {
                    let deadline = in_flight.map(|(_, d)| d).unwrap_or(next);
                    let result = match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(ImportMsg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(ImportMsg::Wake) => {
                            if in_flight.is_none() {
                                next = Instant::now();
                            }
                            continue;
                        }
                        Ok(ImportMsg::Fetched(s, r)) => {
                            if in_flight.map(|(f, _)| f) != Some(s) {
                                continue; // timeout の後に届いたもの
                            }
                            in_flight = None;
                            Some(r)
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => match in_flight {
                            Some((_, d)) if Instant::now() >= d => {
                                in_flight = None;
                                Some(Err(anyhow!(JuizError::TimeoutError {})))
                            }
                            Some(_) => continue,
                            None => None,
                        },
                    };
                    let Some(registry) = me.upgrade() else {
                        break;
                    };
                    if let Some(r) = result {
                        match registry.finish_import(&source, r) {
                            Some(period) => next = now_after(next, period),
                            None => break,
                        }
                        continue;
                    }
                    let Some(req) = registry.begin_import(&source) else {
                        break;
                    };
                    drop(registry);
                    seq += 1;
                    let s = seq;
                    let tx = fetch_tx.clone();
                    in_flight = Some((s, Instant::now() + req.timeout));
                    let spawned = std::thread::Builder::new()
                        .name("pose_state_fetch".to_owned())
                        .spawn(move || {
                            let r = match &req.fetcher {
                                Some(f) => f.fetch(&req.source, &req.query),
                                None => Err(cross_system_error(
                                    "this System can not call container/pose_state of other Systems."
                                        .to_owned(),
                                )),
                            };
                            let _ = tx.send(ImportMsg::Fetched(s, r));
                        });
                    if let Err(e) = spawned {
                        let _ = fetch_tx.send(ImportMsg::Fetched(s, Err(e.into())));
                    }
                }
                log::trace!("pose_state_import({source}) thread exit");
            })?;
        Ok(ImportThread { tx, handle })
    }

    fn begin_import(&self, p: &str) -> Option<ImportRequest> {
        let fetcher = self.fetcher.lock().ok()?.clone();
        let inner = self.lock().ok()?;
        let src = inner.sources.get(p)?;
        let mut query = PoseStateQuery::new(&src.origin.to_string());
        query.since = src.last_version;
        query.system = src.remote_system.clone();
        // 相手の `remote_children` に出る (§19.10)
        if !inner.system_uuid.is_empty() {
            query.subscriber = Some(inner.system_uuid.clone());
        }
        query.destination = inner
            .transforms
            .values()
            .filter(|t| !t.imported && t.source == p)
            .map(|t| t.destination.clone())
            .collect();
        query.timeout = Some(src.timeout);
        Some(ImportRequest {
            source: src.identifier.clone(),
            query,
            timeout: Duration::from_secs_f64(src.timeout),
            fetcher,
        })
    }

    /// 1 回の取り込みの結果を当てはめ、次までの間隔を返す。写しの始点がなくなっていれば `None`
    fn finish_import(&self, p: &str, result: JuizResult<Value>) -> Option<Duration> {
        let (period, notifications) = {
            let mut inner = self.lock().ok()?;
            inner.sources.get(p)?;
            let parsed = result.and_then(|v| {
                serde_json::from_value::<PoseStateResponse>(v.clone()).map_err(|e| {
                    anyhow!(JuizError::InvalidValueError {
                        message: format!(
                            "container/pose_state returned an invalid value: {e} ({v})"
                        ),
                    })
                })
            });
            let outcome = parsed.and_then(|resp| inner.import_succeeded(p, resp));
            let notifications = match outcome {
                // 取り込みは明示的な書き込みではないので on_update の通知だけ (§19.7)
                Ok(changed) => inner.notifications(&[], &changed),
                Err(e) => {
                    inner.import_failed(p, &e);
                    Vec::new()
                }
            };
            let src = inner.sources.get(p)?;
            let period = if src.failing {
                src.period().max(Duration::from_secs(1))
            } else {
                src.period()
            };
            (period, notifications)
        };
        fire(notifications);
        Some(period)
    }
}
