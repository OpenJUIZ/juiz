//! System の Transformations 登録所 (docs/spec/transformations.md §5〜§10)
//!
//! ロックの順番 (§9.2): Container の利用者の状態 → 登録所 ([`Transformations::inner`]) → PoseState。
//! 登録所は Container の利用者の状態のロックを取らない (Container は [`ContainerPoseHandle`] だけで持つ)。
//! Pose 通知の Process は登録所のロックを外してから、書き込んだスレッドで呼ぶ (§8.3)。

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Time, Transform, Vector3};
use juiz_sdk::identifier::split_namespace;
use juiz_sdk::manifests::{
    pose_listener_identifier, transform_identifier, AnchorKind, AnchorManifest,
};
use juiz_sdk::transformations::{ContainerPoseHandle, PoseEntry, TransformationsRegistry};

use super::tree::{compute_tree, out_edges, RemoteParams, TransformEntry, Tree};
use crate::prelude::*;

mod remote;

pub use remote::PoseStateFetcher;
use remote::{ImportThread, LostAnchor, RemoteNode, RemoteSource, ReplicaAnchor};

/// Pose の値が変わったかを比べるときの許容誤差
const POSE_CHANGE_EPSILON: f64 = 1e-12;

thread_local! {
    /// Pose 通知で呼んだ Process の中にいる深さ (§8.4 の再入の判定)
    static NOTIFY_DEPTH: Cell<u32> = const { Cell::new(0) };
}

pub type TransformationsPtr = Arc<Transformations>;

struct Node {
    identifier: ContainerIdentifier,
    handle: ContainerPoseHandle,
    /// 写しの始点 P'・写しの Anchor B' (別 System のもの、§19.3) なら `Some`
    remote: Option<RemoteNode>,
}

impl Node {
    fn local(identifier: ContainerIdentifier, handle: ContainerPoseHandle) -> Self {
        Self {
            identifier,
            handle,
            remote: None,
        }
    }

    /// `header.frame_id` に入れる名前 (§5.4)。写しはブローカー付きの ID (§19.5)
    fn frame_id(&self) -> String {
        match self.remote {
            Some(_) => self.identifier.to_string(),
            None => self.identifier.full_name(),
        }
    }

    fn is_remote(&self) -> bool {
        self.remote.is_some()
    }
}

struct PeriodicThread {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

struct AnchorEntry {
    manifest: AnchorManifest,
    tree: Tree,
    /// Container の ID → 最後に計算したときの (親の値の版番号, Transform の版番号) (§6.2 の 2)
    cache: HashMap<String, (u64, u64)>,
    thread: Option<PeriodicThread>,
    /// 写しの Anchor B' なら `Some` (§19.3)
    remote: Option<ReplicaAnchor>,
}

struct Listener {
    container: String,
    process: ProcessPtr,
    process_id: String,
    arg: String,
    reference: Option<String>,
    on_update: bool,
}

/// 登録所のロックを外してから呼ぶ Pose 通知
struct Notification {
    listener: String,
    process: ProcessPtr,
    arg: String,
    value: Value,
}

#[derive(Default)]
struct Inner {
    nodes: BTreeMap<String, Node>,
    transforms: BTreeMap<String, TransformEntry>,
    anchors: BTreeMap<String, AnchorEntry>,
    listeners: BTreeMap<String, Listener>,
    last_version: u64,
    /// System の起動が終わったか。終わった後に作った Periodic Anchor はすぐスレッドを始める
    started: bool,
    /// この System の UUID (§19.4 の `system`・§19.5 の循環の除外)
    system_uuid: String,
    /// 写しの始点 P' の ID → 取り込みの状態 (§19.4)
    sources: BTreeMap<String, RemoteSource>,
    /// (origin_system, origin_anchor) → 写しの Anchor B' の ID (§19.5)
    replica_index: BTreeMap<(String, String), String>,
    /// 木の規則を破るので作らなかった写しの Anchor (WARN を 1 回だけ出すため、§19.3)
    rejected: BTreeSet<(String, String)>,
    /// つながらなくなって消した写しの Anchor (エラー文に理由を入れるため、§19.6)
    lost: BTreeMap<String, LostAnchor>,
}

/// System の Transformations 登録所 (§9.1)
pub struct Transformations {
    inner: Mutex<Inner>,
    me: Weak<Transformations>,
    /// 相手の System の `container/pose_state` を呼ぶ口 (§19.4)。System が作るときに置く
    fetcher: Mutex<Option<Arc<dyn PoseStateFetcher>>>,
}

impl std::fmt::Debug for Transformations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transformations").finish()
    }
}

fn lock_error() -> anyhow::Error {
    anyhow!(JuizError::ObjectLockError {
        target: "Transformations".to_owned()
    })
}

fn not_found(reference: &str) -> anyhow::Error {
    anyhow!(JuizError::ObjectCanNotFoundByIdError {
        id: reference.to_owned()
    })
}

fn is_not_found(e: &anyhow::Error) -> bool {
    matches!(
        e.downcast_ref::<JuizError>(),
        Some(JuizError::ObjectCanNotFoundByIdError { .. })
    )
}

fn cross_system_error(message: String) -> anyhow::Error {
    anyhow!(JuizError::CrossSystemTransformError { message })
}

fn listener_error(message: String) -> anyhow::Error {
    anyhow!(JuizError::PoseListenerError { message })
}

fn read_entry(handle: &ContainerPoseHandle, anchor: &str) -> Option<PoseEntry> {
    handle.entry(anchor).ok().flatten()
}

fn write_entry(handle: &ContainerPoseHandle, anchor: &str, pose: Pose, stamp: Time) -> u64 {
    match handle.state().write() {
        Ok(mut s) => s.set(anchor, pose, stamp),
        Err(_) => {
            log::error!("PoseState of {} is poisoned.", handle.identifier());
            0
        }
    }
}

fn remove_entry(handle: &ContainerPoseHandle, anchor: &str) {
    if let Ok(mut s) = handle.state().write() {
        s.remove(anchor);
    }
}

impl Inner {
    fn next_version(&mut self) -> u64 {
        self.last_version += 1;
        self.last_version
    }

    fn node(&self, id: &str) -> JuizResult<&Node> {
        self.nodes.get(id).ok_or_else(|| not_found(id))
    }

    /// Container の ID か名前を ID にする (§5.4)。名前は `namespace` から解決し (先頭 `/` は絶対)、
    /// `<name>::<type_name>` の形で type_name を足してもよい。同じ名前が 2 つあればエラー
    /// 自分の System の Container に当たらなければ、写しの Anchor・写しの始点から探す (§19.5)
    fn resolve(&self, reference: &str, namespace: &str) -> JuizResult<String> {
        match self.resolve_local(reference, namespace) {
            Ok(id) => Ok(id),
            Err(e) if is_not_found(&e) => self.resolve_remote(reference, namespace).map_err(|re| {
                if is_not_found(&re) {
                    e
                } else {
                    re
                }
            }),
            Err(e) => Err(e),
        }
    }

    /// 自分の System の Container だけから探す (§5.4)
    fn resolve_local(&self, reference: &str, namespace: &str) -> JuizResult<String> {
        if reference.contains("://") {
            let id = ContainerIdentifier::try_from(reference.to_owned())?.to_string();
            return match self.nodes.get(&id) {
                Some(n) if !n.is_remote() => Ok(id),
                _ => Err(not_found(reference)),
            };
        }
        let (path, type_name) = match reference.split_once("::") {
            Some((p, t)) => (p, Some(t)),
            None => (reference, None),
        };
        let full = juiz_sdk::topic_identifier::resolve_topic_name(namespace, path)?;
        let (ns, name) = split_namespace(&full);
        let found = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                !n.is_remote()
                    && n.identifier.namespace == ns
                    && n.identifier.name == name
                    && type_name.map_or(true, |t| n.identifier.type_name == t)
            })
            .map(|(k, _)| k.clone())
            .collect::<Vec<String>>();
        match found.len() {
            0 => Err(not_found(reference)),
            1 => Ok(found.into_iter().next().unwrap()),
            _ => Err(anyhow!(JuizError::InvalidValueError {
                message: format!(
                    "Container name '{reference}' is ambiguous ({found:?}). Use the identifier instead."
                ),
            })),
        }
    }

    // ---- 木 ----

    /// すべての Anchor の木を作り直す。木の規則 (§5.2) を破っていればエラーで、何も変えない。
    /// 木から外れた Container の PoseState からその Anchor の項目を消す (§6.2 の 3)
    fn rebuild_trees(&mut self) -> JuizResult<()> {
        let trees = {
            let out = out_edges(&self.transforms);
            self.anchors
                .keys()
                .map(|a| Ok((a.clone(), compute_tree(a, &out)?)))
                .collect::<JuizResult<Vec<(String, Tree)>>>()?
        };
        for (a, tree) in trees.into_iter() {
            let entry = self.anchors.get_mut(&a).unwrap();
            for old in entry.tree.order.iter() {
                if !tree.contains(old) {
                    if let Some(n) = self.nodes.get(old) {
                        remove_entry(&n.handle, &a);
                    }
                    entry.cache.remove(old);
                }
            }
            entry.tree = tree;
        }
        Ok(())
    }

    fn tree(&self, anchor: &str) -> JuizResult<&Tree> {
        self.anchors
            .get(anchor)
            .map(|a| &a.tree)
            .ok_or_else(|| not_found(anchor))
    }

    /// PoseState を使わずに、木をたどってその場で pose_A(C) を計算する
    fn compute_now(&self, anchor: &str, node: &str) -> JuizResult<Pose> {
        let tree = self.tree(anchor)?;
        if !tree.contains(node) {
            return Err(anyhow!(JuizError::NoCommonAnchorError {
                container: node.to_owned(),
                reference: anchor.to_owned(),
            }));
        }
        let mut chain = Vec::new();
        let mut cur = node;
        while let Some((p, tid)) = tree.parent.get(cur) {
            chain.push(tid);
            cur = p;
        }
        let mut pose = Pose::identity();
        for tid in chain.iter().rev() {
            pose = pose * self.transforms[*tid].value;
        }
        Ok(pose)
    }

    /// PoseState[A] の値。まだなければその場で計算する (§7.1 の 2)
    fn entry_or_compute(&self, anchor: &str, node: &str) -> JuizResult<(Pose, Time)> {
        match read_entry(&self.node(node)?.handle, anchor) {
            Some(e) => Ok((e.pose, e.stamp)),
            None => Ok((self.compute_now(anchor, node)?, Time::now())),
        }
    }

    /// R と C の両方を木に含む Anchor を選ぶ (§7.1 の 2・3)
    fn choose_anchor(&self, container: &str, reference: &str) -> Option<String> {
        if let Some(a) = self.anchors.get(reference) {
            if a.tree.contains(container) {
                return Some(reference.to_owned());
            }
        }
        self.anchors
            .iter()
            .filter_map(|(id, a)| {
                let dr = a.tree.depth.get(reference)?;
                let dc = a.tree.depth.get(container)?;
                Some((dr + dc, id))
            })
            .min()
            .map(|(_, id)| id.clone())
    }

    /// Anchor `anchor` の木を更新する (§6.2)。値が変わった Container の ID を返す
    fn update_anchor(&mut self, anchor: &str) -> JuizResult<BTreeSet<String>> {
        let stamp = Time::now();
        let mut changed = BTreeSet::new();
        let Inner {
            nodes,
            transforms,
            anchors,
            ..
        } = self;
        let entry = anchors.get_mut(anchor).ok_or_else(|| not_found(anchor))?;
        let root = nodes.get(anchor).ok_or_else(|| not_found(anchor))?;
        if read_entry(&root.handle, anchor).is_none() {
            write_entry(&root.handle, anchor, Pose::identity(), stamp);
        }
        // 写しの Anchor の木の時刻は、取り込んだ値の時刻 (相手の System の時計) のまま運ぶ (§19.3)
        let replica = entry.remote.is_some();
        for node in entry.tree.order.iter().skip(1) {
            let (parent, tid) = &entry.tree.parent[node];
            let (Some(p), Some(c)) = (nodes.get(parent), nodes.get(node)) else {
                continue;
            };
            let Some(pe) = read_entry(&p.handle, anchor) else {
                continue;
            };
            let t = &transforms[tid];
            let key = (pe.version, t.version);
            let old = read_entry(&c.handle, anchor);
            if old.is_some() && entry.cache.get(node) == Some(&key) {
                continue;
            }
            let pose = pe.pose * t.value;
            let node_stamp = match (replica, t.imported) {
                (false, _) => stamp,
                (true, true) => t.stamp,
                (true, false) => pe.stamp,
            };
            write_entry(&c.handle, anchor, pose, node_stamp);
            entry.cache.insert(node.clone(), key);
            if old.map_or(true, |o| !o.pose.approx_eq(&pose, POSE_CHANGE_EPSILON)) {
                changed.insert(node.clone());
            }
        }
        Ok(changed)
    }

    /// StaticAnchor を全部更新する (§4.4 の 2)。`only_with` を渡すと、その Transform を木に持つものだけ
    fn update_static_anchors(&mut self, only_with: Option<&str>) -> BTreeSet<String> {
        let targets = self
            .anchors
            .iter()
            .filter(|(_, a)| a.manifest.kind == AnchorKind::Static)
            .filter(|(_, a)| only_with.map_or(true, |t| a.tree.transforms.contains(t)))
            .map(|(id, _)| id.clone())
            .collect::<Vec<String>>();
        let mut changed = BTreeSet::new();
        for a in targets.iter() {
            match self.update_anchor(a) {
                Ok(c) => changed.extend(c),
                Err(e) => log::error!("updating StaticAnchor({a}) failed: {e}"),
            }
        }
        changed
    }

    // ---- 読み書き ----

    fn pose(&self, container: &str, reference: &str) -> JuizResult<PoseStamped> {
        let frame_id = self.node(reference)?.frame_id();
        self.node(container)?;
        if container == reference {
            return Ok(PoseStamped::new(&frame_id, Time::now(), Pose::identity()));
        }
        let anchor = self.choose_anchor(container, reference).ok_or_else(|| {
            anyhow!(JuizError::NoCommonAnchorError {
                container: container.to_owned(),
                reference: reference.to_owned(),
            })
        })?;
        let (pc, sc) = self.entry_or_compute(&anchor, container)?;
        if anchor == reference {
            return Ok(PoseStamped::new(&frame_id, sc, pc));
        }
        let (pr, sr) = self.entry_or_compute(&anchor, reference)?;
        Ok(PoseStamped::new(&frame_id, sc.min(sr), pr.relative(&pc)))
    }

    /// §7.2 の 1〜6。書いた後の値 (その場で計算したもの) と、値が変わった Container を返す
    fn set_pose(
        &mut self,
        container: &str,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<(PoseStamped, BTreeSet<String>)> {
        let frame_id = self.node(reference)?.frame_id();
        self.node(container)?;
        let not_writable = |message: &str| {
            anyhow!(JuizError::PoseNotWritableError {
                container: container.to_owned(),
                reference: reference.to_owned(),
                message: message.to_owned(),
            })
        };
        if container == reference {
            return Err(not_writable("the Container is the reference itself."));
        }
        if self.node(container)?.is_remote() {
            return Err(not_writable(
                "the Container is a replica of a Container of another System (§19.3). Write it in that System.",
            ));
        }
        let anchor = self.choose_anchor(container, reference).ok_or_else(|| {
            anyhow!(JuizError::NoCommonAnchorError {
                container: container.to_owned(),
                reference: reference.to_owned(),
            })
        })?;
        if container == anchor {
            return Err(not_writable(
                "the Container is the root Anchor of the tree.",
            ));
        }
        let tree = self.tree(&anchor)?;
        if tree.is_descendant(reference, container) {
            return Err(not_writable(
                "the reference is a descendant of the Container.",
            ));
        }
        let (parent, tid) = tree.parent[container].clone();
        let pose = pose.normalized()?;
        let pose_parent = self.compute_now(&anchor, &parent)?;
        let pose_reference = self.compute_now(&anchor, reference)?;
        let pose_container = (pose_reference * pose).normalized()?;
        let value = pose_parent
            .inverse()
            .compose(&pose_container.to_transform())
            .to_transform()
            .normalized()?;
        let stamp = Time::now();
        let version = self.next_version();
        {
            let t = self.transforms.get_mut(&tid).unwrap();
            t.value = value;
            t.stamp = stamp;
            t.version = version;
        }
        write_entry(
            &self.node(container)?.handle,
            &anchor,
            pose_container,
            stamp,
        );
        let mut changed = BTreeSet::from([container.to_owned()]);
        changed.extend(self.update_static_anchors(Some(&tid)));
        let written = PoseStamped::new(&frame_id, stamp, pose_reference.relative(&pose_container));
        Ok((written, changed))
    }

    /// Transform の値を変えたときに、終点の PoseState をすぐ新しい値にし、StaticAnchor を更新する (§7.3)
    fn after_transform_value_changed(&mut self, tid: &str) -> BTreeSet<String> {
        let destination = self.transforms[tid].destination.clone();
        let stamp = self.transforms[tid].stamp;
        let anchors = self
            .anchors
            .iter()
            .filter(|(_, a)| a.tree.transforms.contains(tid))
            .map(|(id, _)| id.clone())
            .collect::<Vec<String>>();
        let mut changed = BTreeSet::new();
        for a in anchors.iter() {
            if let (Ok(p), Some(n)) = (
                self.compute_now(a, &destination),
                self.nodes.get(&destination),
            ) {
                write_entry(&n.handle, a, p, stamp);
                changed.insert(destination.clone());
            }
        }
        changed.extend(self.update_static_anchors(Some(tid)));
        changed
    }

    // ---- Pose 通知 ----

    /// 書き込み (`written` = (Container, 書き込みの基準)) と、Anchor の更新で値が変わった Container から、
    /// 呼ぶ Pose 通知を集める (§8)。1 つの登録は 1 回だけ呼ぶ
    fn notifications(
        &self,
        written: &[(String, String)],
        updated: &BTreeSet<String>,
    ) -> Vec<Notification> {
        let mut out = Vec::new();
        for (lid, l) in self.listeners.iter() {
            let reference = if let Some((_, r)) = written.iter().find(|(c, _)| *c == l.container) {
                l.reference.clone().unwrap_or_else(|| r.clone())
            } else if l.on_update && updated.contains(&l.container) {
                match &l.reference {
                    Some(r) => r.clone(),
                    None => continue,
                }
            } else {
                continue;
            };
            match self.pose(&l.container, &reference) {
                Ok(p) => out.push(Notification {
                    listener: lid.clone(),
                    process: l.process.clone(),
                    arg: l.arg.clone(),
                    value: p.into(),
                }),
                Err(e) => log::debug!("Pose listener ({lid}) is skipped: {e}"),
            }
        }
        out
    }

    // ---- 後始末 ----

    /// Container を外す (§10)。止めるべき Periodic のスレッドと、消えた Transform の終点への通知を返す
    fn remove_node(
        &mut self,
        id: &str,
    ) -> (
        Option<PeriodicThread>,
        Vec<(String, String)>,
        BTreeSet<String>,
    ) {
        self.listeners
            .retain(|_, l| l.container != id && l.reference.as_deref() != Some(id));
        let removed = self
            .transforms
            .iter()
            .filter(|(_, t)| t.source == id || t.destination == id)
            .map(|(tid, t)| (tid.clone(), t.source.clone(), t.destination.clone()))
            .collect::<Vec<(String, String, String)>>();
        for (tid, _, _) in removed.iter() {
            self.transforms.remove(tid);
        }
        let mut thread = None;
        if let Some(a) = self.anchors.remove(id) {
            thread = a.thread;
            for n in self.nodes.values() {
                remove_entry(&n.handle, id);
            }
        }
        if let Some(n) = self.nodes.remove(id) {
            let _ = n.handle.detach();
        }
        if let Err(e) = self.rebuild_trees() {
            log::error!("rebuilding Transform trees after removing Container({id}) failed: {e}");
        }
        let changed = self.update_static_anchors(None);
        let written = removed
            .into_iter()
            .filter(|(_, s, d)| s != id && d != id)
            .map(|(_, s, d)| (d, s))
            .collect();
        (thread, written, changed)
    }
}

fn now_after(next: Instant, period: Duration) -> Instant {
    let n = next + period;
    let now = Instant::now();
    // 周期より長くかかったときは、次の更新をすぐに始める (遅れを貯めない、§4.3)
    if n < now {
        now
    } else {
        n
    }
}

/// Pose 通知を呼ぶ (§8.3)。Y のエラーは WARN を出して捨てる
fn fire(notifications: Vec<Notification>) {
    for n in notifications.into_iter() {
        NOTIFY_DEPTH.with(|d| d.set(d.get() + 1));
        let result = n
            .process
            .lock()
            .and_then(|p| p.push_by(&n.arg, CapsulePtr::from(n.value)));
        NOTIFY_DEPTH.with(|d| d.set(d.get() - 1));
        if let Err(e) = result {
            log::warn!("Pose listener ({}) failed: {e}", n.listener);
        }
    }
}

/// 通知の中での書き込みなら通知を出さない (§8.4)
fn fire_unless_reentrant(notifications: Vec<Notification>) {
    if notifications.is_empty() {
        return;
    }
    if NOTIFY_DEPTH.with(|d| d.get()) > 0 {
        log::warn!(
            "Pose was written inside a Pose listener. Pose listeners for this write are not called ({} listener(s)).",
            notifications.len()
        );
        return;
    }
    fire(notifications);
}

fn join_threads(threads: Vec<PeriodicThread>) {
    let me = std::thread::current().id();
    for t in threads.into_iter() {
        let _ = t.stop.send(());
        if t.handle.thread().id() != me {
            let _ = t.handle.join();
        }
    }
}

impl Transformations {
    pub fn new() -> TransformationsPtr {
        Arc::new_cyclic(|me| Transformations {
            inner: Mutex::new(Inner::default()),
            me: me.clone(),
            fetcher: Mutex::new(None),
        })
    }

    fn lock(&self) -> JuizResult<MutexGuard<'_, Inner>> {
        self.inner.lock().map_err(|_| lock_error())
    }

    fn weak_registry(&self) -> Weak<dyn TransformationsRegistry> {
        self.me.clone() as Weak<dyn TransformationsRegistry>
    }

    // ---- Container と Anchor ----

    /// System に登録した Container をつなぐ (§9.1)。Anchor なら `anchor` を渡す
    pub fn add_container(
        &self,
        container: &ContainerPtr,
        anchor: Option<AnchorManifest>,
    ) -> JuizResult<()> {
        let id = container.identifier().to_string();
        let handle = container.pose_handle();
        handle.attach(self.weak_registry())?;
        let changed = {
            let mut inner = self.lock()?;
            inner
                .nodes
                .insert(id.clone(), Node::local(container.identifier(), handle));
            let Some(manifest) = anchor else {
                return Ok(());
            };
            let tree = compute_tree(&id, &out_edges(&inner.transforms))?;
            let thread = if inner.started && manifest.kind == AnchorKind::Periodic {
                Some(self.spawn_periodic(&id, &manifest)?)
            } else {
                None
            };
            inner.anchors.insert(
                id.clone(),
                AnchorEntry {
                    manifest,
                    tree,
                    cache: HashMap::new(),
                    thread,
                    remote: None,
                },
            );
            // 作られたときに自分の木を更新する (Static は §4.4 の 1、Periodic も根の値を置いておく)
            let changed = inner.update_anchor(&id)?;
            inner.notifications(&[], &changed)
        };
        fire(changed);
        Ok(())
    }

    /// Container を外す (§10)。それを端点に持つ Transform と、それを見張る Pose 通知を消す
    pub fn remove_container(&self, identifier: &str) -> JuizResult<()> {
        let (thread, imports, notifications) = {
            let mut inner = self.lock()?;
            if !inner.nodes.get(identifier).is_some_and(|n| !n.is_remote()) {
                return Ok(());
            }
            let (thread, written, mut changed) = inner.remove_node(identifier);
            // 終点を消して Transform がなくなった写しの始点の取り込みを止める (§19.6)
            let imports = inner.remove_unused_sources();
            changed.extend(inner.update_static_anchors(None));
            (thread, imports, inner.notifications(&written, &changed))
        };
        join_threads(thread.into_iter().collect());
        remote::join_imports(imports);
        fire_unless_reentrant(notifications);
        Ok(())
    }

    /// Process (ContainerProcess を含む) を消したときに、それを呼ぶ Pose 通知を消す (§10)
    pub fn remove_listeners_of_process(&self, process_id: &str) -> JuizResult<()> {
        self.lock()?
            .listeners
            .retain(|_, l| l.process_id != process_id);
        Ok(())
    }

    /// System の起動 (conf の読み込みの後) で Periodic Anchor のスレッドを始める (§4.3)
    pub fn start(&self) -> JuizResult<()> {
        let mut inner = self.lock()?;
        inner.started = true;
        let pending = inner
            .anchors
            .iter()
            .filter(|(_, a)| a.manifest.kind == AnchorKind::Periodic && a.thread.is_none())
            .map(|(id, a)| (id.clone(), a.manifest))
            .collect::<Vec<(String, AnchorManifest)>>();
        for (id, manifest) in pending.into_iter() {
            let t = self.spawn_periodic(&id, &manifest)?;
            inner.anchors.get_mut(&id).unwrap().thread = Some(t);
        }
        // 写しの取り込みも System の起動の最後に始める (§19.4)
        self.start_imports(&mut inner)?;
        Ok(())
    }

    /// すべて外す (System の終了)。Periodic のスレッドを止め、Container を登録所から外す
    pub fn clear(&self) -> JuizResult<()> {
        let (threads, imports) = {
            let mut inner = self.lock()?;
            let threads = std::mem::take(&mut inner.anchors)
                .into_values()
                .filter_map(|a| a.thread)
                .collect::<Vec<PeriodicThread>>();
            let imports = std::mem::take(&mut inner.sources)
                .into_values()
                .filter_map(|s| s.thread)
                .collect::<Vec<ImportThread>>();
            for n in inner.nodes.values() {
                let _ = n.handle.detach();
            }
            let system_uuid = std::mem::take(&mut inner.system_uuid);
            *inner = Inner::default();
            inner.system_uuid = system_uuid;
            (threads, imports)
        };
        join_threads(threads);
        remote::join_imports(imports);
        Ok(())
    }

    fn spawn_periodic(&self, id: &str, manifest: &AnchorManifest) -> JuizResult<PeriodicThread> {
        let period = manifest.period().ok_or_else(|| {
            anyhow!(JuizError::InvalidValueError {
                message: format!("Anchor ({id}) is not periodic."),
            })
        })?;
        let (stop, rx) = mpsc::channel::<()>();
        let me = self.me.clone();
        let anchor = id.to_owned();
        let handle = std::thread::Builder::new()
            .name("periodic_anchor".to_owned())
            .spawn(move || {
                let mut next = Instant::now();
                loop {
                    let wait = next.saturating_duration_since(Instant::now());
                    match rx.recv_timeout(wait) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        _ => break,
                    }
                    let Some(registry) = me.upgrade() else {
                        break;
                    };
                    if !registry.tick(&anchor) {
                        break;
                    }
                    drop(registry);
                    next = now_after(next, period);
                }
                log::trace!("periodic_anchor({anchor}) thread exit");
            })?;
        Ok(PeriodicThread { stop, handle })
    }

    /// Periodic Anchor の 1 回の更新。Anchor が無くなっていれば false
    fn tick(&self, anchor: &str) -> bool {
        let notifications = {
            let Ok(mut inner) = self.lock() else {
                return false;
            };
            if !inner.anchors.contains_key(anchor) {
                return false;
            }
            match inner.update_anchor(anchor) {
                Ok(changed) => inner.notifications(&[], &changed),
                Err(e) => {
                    log::error!("updating PeriodicExecutionAnchor({anchor}) failed: {e}");
                    Vec::new()
                }
            }
        };
        fire(notifications);
        true
    }

    /// Anchor の木を今すぐ更新する (API `anchor/update`、§4.4 の 3)。更新の時刻を返す
    pub fn update_anchor(&self, anchor: &str, namespace: &str) -> JuizResult<Value> {
        let (id, notifications) = {
            let mut inner = self.lock()?;
            let id = inner.resolve(anchor, namespace)?;
            let Some(a) = inner.anchors.get(&id) else {
                return Err(anyhow!(JuizError::InvalidValueError {
                    message: format!("Container ({anchor}) is not an Anchor."),
                }));
            };
            // 写しの Anchor なら、その写しの始点の取り込みをすぐ 1 回行う (§19.10)
            if let Some(r) = &a.remote {
                for p in r.children.iter() {
                    if let Some(t) = inner.sources.get(p).and_then(|s| s.thread.as_ref()) {
                        t.wake();
                    }
                }
            }
            let changed = inner.update_anchor(&id)?;
            (id, inner.notifications(&[], &changed))
        };
        fire(notifications);
        Ok(jvalue!({"identifier": id, "stamp": Value::from(Time::now())}))
    }

    /// 登録されている Anchor の ID
    pub fn anchor_list(&self) -> JuizResult<Vec<String>> {
        Ok(self.lock()?.anchors.keys().cloned().collect())
    }

    // ---- Transform ----

    /// Transform をまとめて作る。conf の `transforms:` (全部登録してから木を検査する、§5.2・§11) と API の `transform/create` で使う。
    /// 名前は `namespace` から解決する。1 つでも不備があれば何も作らない
    pub fn create_transforms(
        &self,
        manifests: &[juiz_sdk::manifests::TransformManifest],
        namespace: &str,
    ) -> JuizResult<Vec<Value>> {
        let (profiles, notifications) = {
            let mut inner = self.lock()?;
            let mut added: Vec<String> = Vec::new();
            let mut added_sources: Vec<ContainerIdentifier> = Vec::new();
            let mut added_nodes: Vec<String> = Vec::new();
            let mut written = Vec::new();
            let result = (|| -> JuizResult<()> {
                for m in manifests.iter() {
                    let m = m.validated()?;
                    let (source, remote) = match inner.remote_source_of(&m.source)? {
                        Some(identifier) => {
                            let id = identifier.to_string();
                            if !inner.sources.contains_key(&id)
                                && !added_sources.iter().any(|a| a.to_string() == id)
                            {
                                if inner.add_source_node(&identifier) {
                                    added_nodes.push(id.clone());
                                }
                                added_sources.push(identifier);
                            }
                            let params = RemoteParams {
                                rate: m.rate.unwrap_or(juiz_sdk::manifests::DEFAULT_REMOTE_RATE),
                                timeout: m
                                    .timeout
                                    .unwrap_or(juiz_sdk::manifests::DEFAULT_REMOTE_TIMEOUT),
                            };
                            (id, Some(params))
                        }
                        None => (
                            inner.resolve_endpoint(&m.source, namespace, "source")?,
                            None,
                        ),
                    };
                    if remote.is_none() && (m.rate.is_some() || m.timeout.is_some()) {
                        log::warn!(
                            "rate / timeout of Transform ({} -> {}) are ignored: they are used only by a Transform whose source is on another System.",
                            m.source,
                            m.destination
                        );
                    }
                    let destination =
                        inner.resolve_endpoint(&m.destination, namespace, "destination")?;
                    if source == destination {
                        return Err(anyhow!(JuizError::TransformManifestInvalidError {
                            message: format!(
                                "source and destination must be different Containers: {source}"
                            ),
                        }));
                    }
                    let tid = transform_identifier(&source, &destination);
                    if inner.transforms.contains_key(&tid) {
                        return Err(anyhow!(JuizError::TransformAlreadyExistsError {
                            identifier: tid
                        }));
                    }
                    let version = inner.next_version();
                    inner.transforms.insert(
                        tid.clone(),
                        TransformEntry {
                            source: source.clone(),
                            destination: destination.clone(),
                            value: m.transform(),
                            stamp: Time::now(),
                            version,
                            imported: false,
                            remote,
                        },
                    );
                    added.push(tid);
                    written.push((destination, source));
                }
                inner.rebuild_trees()
            })();
            if let Err(e) = result {
                for tid in added.iter() {
                    inner.transforms.remove(tid);
                }
                for p in added_nodes.iter() {
                    inner.nodes.remove(p);
                }
                return Err(e);
            }
            // 新しい写しの始点の取り込みを始める (起動前なら start で、§19.4)
            for p in added_sources.iter() {
                inner.sources.insert(p.to_string(), RemoteSource::new(p));
            }
            inner.refresh_source_params();
            if inner.started {
                self.start_imports(&mut inner)?;
            }
            let changed = inner.update_static_anchors(None);
            let profiles = added
                .iter()
                .map(|tid| transform_profile(&inner, tid))
                .collect::<JuizResult<Vec<Value>>>()?;
            (profiles, inner.notifications(&written, &changed))
        };
        fire_unless_reentrant(notifications);
        Ok(profiles)
    }

    /// Transform の値を変える (§7.3)。省いたものは今のまま
    pub fn update_transform(
        &self,
        identifier: &str,
        translation: Option<Vector3>,
        rotation: Option<Quaternion>,
    ) -> JuizResult<Value> {
        let (profile, notifications) = {
            let mut inner = self.lock()?;
            let t = inner
                .transforms
                .get(identifier)
                .filter(|t| !t.imported)
                .ok_or_else(|| {
                    anyhow!(JuizError::TransformNotFoundError {
                        identifier: identifier.to_owned()
                    })
                })?;
            let value = Transform::new(
                translation.unwrap_or(t.value.translation),
                rotation.unwrap_or(t.value.rotation),
            )
            .normalized()?;
            let written = vec![(t.destination.clone(), t.source.clone())];
            let version = inner.next_version();
            {
                let t = inner.transforms.get_mut(identifier).unwrap();
                t.value = value;
                t.stamp = Time::now();
                t.version = version;
            }
            let changed = inner.after_transform_value_changed(identifier);
            (
                transform_profile(&inner, identifier)?,
                inner.notifications(&written, &changed),
            )
        };
        fire_unless_reentrant(notifications);
        Ok(profile)
    }

    /// Transform を消す (§10)
    pub fn destroy_transform(&self, identifier: &str) -> JuizResult<Value> {
        let (profile, imports, notifications) = {
            let mut inner = self.lock()?;
            let profile = transform_profile(&inner, identifier)?;
            let t = inner.transforms.remove(identifier).unwrap();
            inner.rebuild_trees()?;
            // 始点が写しの始点で、Transform が残らなければ取り込みを止める (§19.6)
            let imports = inner.remove_unused_sources();
            inner.refresh_source_params();
            let changed = inner.update_static_anchors(None);
            let written = vec![(t.destination, t.source)];
            (profile, imports, inner.notifications(&written, &changed))
        };
        remote::join_imports(imports);
        fire_unless_reentrant(notifications);
        Ok(profile)
    }

    pub fn transform_list(&self) -> JuizResult<Vec<String>> {
        Ok(self
            .lock()?
            .transforms
            .iter()
            .filter(|(_, t)| !t.imported)
            .map(|(k, _)| k.clone())
            .collect())
    }

    /// `{identifier, source, destination, transform, stamp}` (§13.1)
    pub fn transform_profile(&self, identifier: &str) -> JuizResult<Value> {
        transform_profile(&*self.lock()?, identifier)
    }

    /// Anchor の木 `{identifier, pose, transform?, children: [...]}` の入れ子 (§13.1 の `transformations/tree`)
    pub fn tree(&self, anchor: &str, namespace: &str) -> JuizResult<Value> {
        let inner = self.lock()?;
        let id = inner.resolve(anchor, namespace)?;
        let tree = inner.tree(&id)?;
        fn build(inner: &Inner, tree: &Tree, anchor: &str, node: &str) -> JuizResult<Value> {
            let (pose, _) = inner.entry_or_compute(anchor, node)?;
            let mut v = jvalue!({"identifier": node, "pose": Value::from(pose)});
            if let Some((_, tid)) = tree.parent.get(node) {
                v["transform"] = inner.transforms[tid].value.into();
            }
            let children = tree
                .children(node)
                .map(|c| build(inner, tree, anchor, c))
                .collect::<JuizResult<Vec<Value>>>()?;
            v["children"] = children.into();
            Ok(v)
        }
        build(&inner, tree, &id, &id)
    }

    // ---- 読み書き (名前で) ----

    /// Container `container` の、基準 `reference` から見た ContainerPose (§7.1)。名前は `namespace` から解決する
    pub fn pose_by_name(
        &self,
        container: &str,
        reference: &str,
        namespace: &str,
    ) -> JuizResult<PoseStamped> {
        let inner = self.lock()?;
        inner.pose_explained(container, reference, namespace)
    }

    /// Container `container` を基準 `reference` から見て `pose` にする (§7.2)。名前は `namespace` から解決する
    pub fn set_pose_by_name(
        &self,
        container: &str,
        reference: &str,
        namespace: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        let (c, r) = {
            let inner = self.lock()?;
            (
                inner.resolve(container, namespace)?,
                inner.resolve(reference, namespace)?,
            )
        };
        self.set_pose_ids(&c, &r, pose)
    }

    fn set_pose_ids(
        &self,
        container: &str,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        let (written, notifications) = {
            let mut inner = self.lock()?;
            let (written, changed) = inner.set_pose(container, reference, pose)?;
            let notifications =
                inner.notifications(&[(container.to_owned(), reference.to_owned())], &changed);
            (written, notifications)
        };
        fire_unless_reentrant(notifications);
        Ok(written)
    }

    // ---- Pose 通知 ----

    /// Pose 通知を登録する (§8.3)。Process の解決と引数・型の検査は `CoreWorker::create_pose_listener` が行う
    pub(crate) fn add_listener(
        &self,
        container: &str,
        process: ProcessPtr,
        arg: &str,
        reference: Option<&str>,
        on_update: bool,
        namespace: &str,
    ) -> JuizResult<Value> {
        let mut inner = self.lock()?;
        let container = inner.resolve(container, namespace)?;
        let reference = reference.map(|r| inner.resolve(r, namespace)).transpose()?;
        let process_id = process.identifier().to_string();
        let id = pose_listener_identifier(&container, &process_id, arg);
        if inner.listeners.contains_key(&id) {
            return Err(listener_error(format!(
                "Pose listener ({id}) already exists."
            )));
        }
        inner.listeners.insert(
            id.clone(),
            Listener {
                container,
                process,
                process_id,
                arg: arg.to_owned(),
                reference,
                on_update,
            },
        );
        listener_profile(&inner, &id)
    }

    pub fn destroy_pose_listener(&self, identifier: &str) -> JuizResult<Value> {
        let mut inner = self.lock()?;
        let profile = listener_profile(&inner, identifier)?;
        inner.listeners.remove(identifier);
        Ok(profile)
    }

    pub fn pose_listener_list(&self) -> JuizResult<Vec<String>> {
        Ok(self.lock()?.listeners.keys().cloned().collect())
    }

    pub fn pose_listener_profile(&self, identifier: &str) -> JuizResult<Value> {
        listener_profile(&*self.lock()?, identifier)
    }
}

fn transform_profile(inner: &Inner, identifier: &str) -> JuizResult<Value> {
    let t = inner
        .transforms
        .get(identifier)
        .filter(|t| !t.imported)
        .ok_or_else(|| {
            anyhow!(JuizError::TransformNotFoundError {
                identifier: identifier.to_owned()
            })
        })?;
    let mut v = jvalue!({
        "identifier": identifier,
        "source": t.source,
        "destination": t.destination,
        "transform": Value::from(t.value),
        "stamp": Value::from(t.stamp),
    });
    // System をまたぐ Transform の取り込みの状態 (§19.10)
    if let (Some(params), Some(src)) = (t.remote, inner.sources.get(&t.source)) {
        v["remote"] = src.profile(&params);
    }
    Ok(v)
}

fn listener_profile(inner: &Inner, identifier: &str) -> JuizResult<Value> {
    let l = inner
        .listeners
        .get(identifier)
        .ok_or_else(|| listener_error(format!("Pose listener ({identifier}) is not found.")))?;
    Ok(jvalue!({
        "identifier": identifier,
        "container": l.container,
        "process": l.process_id,
        "arg": l.arg,
        "reference": l.reference,
        "on_update": l.on_update,
    }))
}

impl TransformationsRegistry for Transformations {
    fn pose(&self, container: &ContainerIdentifier, reference: &str) -> JuizResult<PoseStamped> {
        let inner = self.lock()?;
        inner.pose_explained(&container.to_string(), reference, &container.namespace)
    }

    fn set_pose(
        &self,
        container: &ContainerIdentifier,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped> {
        let r = self.lock()?.resolve(reference, &container.namespace)?;
        self.set_pose_ids(&container.to_string(), &r, pose)
    }
}
