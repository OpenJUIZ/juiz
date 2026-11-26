//! 機械ごとの登録の登録・解除・一覧・解決 (docs/spec/naming.md §4〜§7)

use std::path::{Path, PathBuf};

use juiz_sdk::anyhow::anyhow;

use crate::prelude::*;

use super::lan::REMOTE_ORDER;
use super::probe::{check, check_pid, Liveness, ProfileProbe, LOCAL_ORDER};
use super::record::{
    is_stale, read_registration, registered_name, registration_files, registration_path,
    remove_if_uuid, remove_registration, validate_name, write_new, ReadResult, RegisteredBroker,
    Registration,
};

/// 同じ名前に付ける番号の上限 (`-2` 〜)。これを越えたら諦める
const MAX_SUFFIX: u32 = 1000;

/// 登録ファイルを置くディレクトリ (`<dir>/names`) を扱う
#[derive(Debug, Clone)]
pub struct NameRegistry {
    dir: PathBuf,
}

/// 登録しようとした名前が、生きている別の juiz に使われていた (§4.3)
#[derive(Debug, Clone)]
pub struct Conflict {
    /// 先に登録していた juiz の登録名
    pub registered_name: String,
    pub pid: u32,
}

/// 登録の結果
#[derive(Debug)]
pub struct Registered {
    names_dir: PathBuf,
    path: PathBuf,
    /// 書いた中身 (名前は `-2` などを付けた後のもの)
    pub registration: Registration,
    /// 同じ名前が使われていて、番号を付けたとき
    pub conflict: Option<Conflict>,
    removed: bool,
}

impl Registered {
    /// 登録ファイルのパス
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 解除する (§5.2)。`uuid` が自分のものでなければ消さない
    pub fn unregister(mut self) -> bool {
        self.remove()
    }

    fn remove(&mut self) -> bool {
        if self.removed {
            return false;
        }
        self.removed = true;
        let r = remove_if_uuid(&self.names_dir, &self.path, &self.registration.uuid);
        if r {
            log::info!("unregistered {}", self.registration.registered_name());
        }
        r
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.remove();
    }
}

/// 名前の引き方 (§4.2)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameQuery {
    /// `None` ならどのネームスペースでも。`Some("")` はルート
    pub namespace: Option<String>,
    pub name: String,
    /// `@<機械>`
    pub host: Option<String>,
}

impl NameQuery {
    /// `mobile_sim`・`robot1/mobile_sim`・`/mobile_sim`・`mobile_sim@robot1-pc` を読む
    pub fn parse(s: &str) -> JuizResult<NameQuery> {
        let (body, host) = match s.split_once('@') {
            Some((b, h)) => {
                if h.is_empty() || h.contains('@') || h.contains('/') {
                    return Err(anyhow!("invalid registered name '{s}'. Write the machine once after the name (ex., mobile_sim@robot1-pc)"));
                }
                (b, Some(h.to_owned()))
            }
            None => (s, None),
        };
        let (namespace, name) = match body.rsplit_once('/') {
            Some((ns, n)) => {
                let ns = ns.trim_matches('/');
                super::record::namespace_segments(ns)
                    .map_err(|e| anyhow!("invalid registered name '{s}': {e}"))?;
                (Some(ns.to_owned()), n)
            }
            None => (None, body),
        };
        validate_name(name).map_err(|e| anyhow!("invalid registered name '{s}': {e}"))?;
        Ok(NameQuery {
            namespace,
            name: name.to_owned(),
            host,
        })
    }

    /// 登録が当てはまるか
    pub fn matches(&self, reg: &Registration) -> bool {
        reg.name == self.name
            && self
                .namespace
                .as_ref()
                .is_none_or(|ns| reg.namespace.trim_matches('/') == ns)
            && self.host.as_ref().is_none_or(|h| &reg.host == h)
    }
}

impl std::fmt::Display for NameQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(ns) if ns.is_empty() => write!(f, "/{}", self.name)?,
            Some(ns) => write!(f, "{ns}/{}", self.name)?,
            None => write!(f, "{}", self.name)?,
        }
        if let Some(h) = &self.host {
            write!(f, "@{h}")?;
        }
        Ok(())
    }
}

/// 登録を見つけた場所 (§7.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Place {
    /// この機械の登録ファイル (§3)
    #[default]
    ThisMachine,
    /// LAN の mDNS (§11)。機械は登録の `host`
    Lan,
    /// 知っている juiz (§12) に聞いたもの。機械は登録の `host`
    Peer,
}

impl Place {
    /// 別の機械か (ipc は使えず、pid は確かめられない)
    pub fn is_remote(&self) -> bool {
        !matches!(self, Place::ThisMachine)
    }
}

/// 一覧の 1 行 (§9.3)
#[derive(Debug, Clone)]
pub struct ListedRegistration {
    pub registration: Registration,
    /// `--check` のとき: 応答したか
    pub alive: Option<bool>,
    pub place: Place,
}

impl ListedRegistration {
    /// `juiz name list --json` の要素 (§3.2 の形に `alive`・`machine` を足す)
    pub fn to_json(&self) -> Value {
        let mut v = self.registration.to_json();
        if let Some(m) = v.as_object_mut() {
            if let Some(a) = self.alive {
                m.insert("alive".to_owned(), jvalue!(a));
            }
            let machine = match self.place {
                Place::ThisMachine => "this".to_owned(),
                Place::Lan | Place::Peer => self.registration.host.clone(),
            };
            m.insert("machine".to_owned(), jvalue!(machine));
            match self.place {
                Place::Lan => {
                    m.insert("found".to_owned(), jvalue!("lan"));
                }
                Place::Peer => {
                    m.insert("found".to_owned(), jvalue!("peer"));
                }
                Place::ThisMachine => {}
            }
        }
        v
    }
}

/// 解決の候補 (§2)
#[derive(Debug, Clone)]
pub struct Candidate {
    pub registration: Registration,
    pub place: Place,
}

impl Candidate {
    /// この機械の登録の候補
    pub fn local(registration: Registration) -> Candidate {
        Candidate {
            registration,
            place: Place::ThisMachine,
        }
    }

    /// LAN で見つけた候補
    pub fn lan(registration: Registration) -> Candidate {
        Candidate {
            registration,
            place: Place::Lan,
        }
    }

    /// 知っている juiz (§12) に聞いた候補
    pub fn peer(registration: Registration) -> Candidate {
        Candidate {
            registration,
            place: Place::Peer,
        }
    }

    /// 候補の一覧の 1 行 `robot1/mobile_sim  0.4.0  pid 12345  (this machine)`、
    /// LAN なら `robot2/mobile_sim  0.4.0  at robot2-pc` (§7.2)
    pub fn describe(&self) -> String {
        let r = &self.registration;
        let mut s = r.registered_name();
        if let Some(v) = r.package_version() {
            s.push_str(&format!("  {v}"));
        }
        match self.place {
            Place::ThisMachine => s.push_str(&format!("  pid {}  (this machine)", r.pid)),
            Place::Lan | Place::Peer => s.push_str(&format!("  at {}", r.host)),
        }
        s
    }

    /// 複数候補のエラーの文の 1 つ (`robot1/mobile_sim (this machine)`・`robot2/mobile_sim@robot2-pc`)
    pub fn short(&self) -> String {
        match self.place {
            Place::ThisMachine => format!("{} (this machine)", self.registration.registered_name()),
            Place::Lan | Place::Peer => format!(
                "{}@{}",
                self.registration.registered_name(),
                self.registration.host
            ),
        }
    }

    /// ブローカーを選ぶ順 (§7.3)
    fn order(&self) -> &'static [&'static str] {
        match self.place {
            Place::ThisMachine => &LOCAL_ORDER,
            Place::Lan | Place::Peer => &REMOTE_ORDER,
        }
    }

    /// つなぐブローカーを選ぶ (§7.3)。`kind` は種類の指定、`kinds` は引く側がつなげる種類
    pub fn select_broker(
        &self,
        kind: Option<&str>,
        kinds: &[String],
    ) -> JuizResult<(RegisteredBroker, String)> {
        let reg = &self.registration;
        let has = || {
            let mut v: Vec<&str> = vec![];
            for b in &reg.brokers {
                if !v.contains(&b.type_name.as_str()) {
                    v.push(&b.type_name);
                }
            }
            v.join(", ")
        };
        if let Some(kind) = kind {
            let b = reg
                .brokers
                .iter()
                .filter(|b| self.place == Place::ThisMachine || b.type_name != "ipc")
                .find(|b| b.type_name == kind)
                .ok_or_else(|| {
                    anyhow!(
                        "{} has no {kind} broker (it has: {})",
                        reg.registered_name(),
                        has()
                    )
                })?;
            if !kinds.iter().any(|k| k == kind) {
                return Err(anyhow!("this juiz can not connect to a {kind} broker"));
            }
            return Ok((b.clone(), b.local_url()));
        }
        for k in self.order() {
            if !kinds.iter().any(|x| x == k) {
                continue;
            }
            if let Some(b) = reg.brokers.iter().find(|b| &b.type_name == k) {
                return Ok((b.clone(), b.local_url()));
            }
        }
        Err(anyhow!(
            "{} has no broker this juiz can connect to (it has: {})",
            reg.registered_name(),
            has()
        ))
    }
}

/// 登録のための情報 (§3.2)
#[derive(Debug, Clone)]
pub struct RegisterRequest {
    pub name: String,
    pub namespace: String,
    pub uuid: String,
    pub package: Option<Value>,
    pub brokers: Vec<RegisteredBroker>,
    /// `option.naming.unique` (§4.3)
    pub unique: bool,
}

impl NameRegistry {
    /// この機械の登録の置き場所 (`<dir>/names`、§3.1)。無ければ作る
    pub fn open() -> JuizResult<NameRegistry> {
        Ok(NameRegistry {
            dir: super::dir::names_dir()?,
        })
    }

    /// 決まったディレクトリを使う (テスト用)
    pub fn at(dir: impl Into<PathBuf>) -> NameRegistry {
        NameRegistry { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 登録する (§4.3・§5.1)。生きている同じ名前があれば `-2`、`-3`、… を付ける。`unique` ならエラー
    pub fn register(
        &self,
        req: &RegisterRequest,
        probe: &dyn ProfileProbe,
        kinds: &[String],
    ) -> JuizResult<Registered> {
        validate_name(&req.name)?;
        super::record::namespace_segments(&req.namespace)?;
        let namespace = req.namespace.trim_matches('/').to_owned();
        let mut conflict: Option<Conflict> = None;
        for n in 1..=MAX_SUFFIX {
            let name = if n == 1 {
                req.name.clone()
            } else {
                format!("{}-{n}", req.name)
            };
            if validate_name(&name).is_err() {
                break;
            }
            let path = registration_path(&self.dir, &namespace, &name)?;
            let reg = Registration {
                format: super::record::FORMAT,
                name: name.clone(),
                namespace: namespace.clone(),
                uuid: req.uuid.clone(),
                pid: std::process::id(),
                host: super::record::host_name(),
                juiz_version: env!("CARGO_PKG_VERSION").to_owned(),
                started_at: super::record::now_rfc3339(),
                package: req.package.clone(),
                brokers: req.brokers.clone(),
            };
            // 死んだ登録を消したときは、同じ名前で 1 回だけやり直す
            for _attempt in 0..2 {
                match write_new(&path, &reg) {
                    Ok(()) => {
                        return Ok(Registered {
                            names_dir: self.dir.clone(),
                            path,
                            registration: reg,
                            conflict,
                            removed: false,
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => {
                        return Err(anyhow!("{e}"));
                    }
                }
                match read_registration(&path) {
                    ReadResult::Ok(other) => match check(&other, probe, kinds) {
                        Liveness::Dead => {
                            remove_if_uuid(&self.dir, &path, &other.uuid);
                            continue;
                        }
                        _ => {
                            if req.unique {
                                return Err(anyhow!(
                                    "{} is already registered by another juiz (pid {})",
                                    other.registered_name(),
                                    other.pid
                                ));
                            }
                            if conflict.is_none() {
                                conflict = Some(Conflict {
                                    registered_name: other.registered_name(),
                                    pid: other.pid,
                                });
                            }
                            break;
                        }
                    },
                    ReadResult::Missing => continue,
                    ReadResult::Unreadable if is_stale(&path) => {
                        remove_registration(&self.dir, &path);
                        continue;
                    }
                    // 書いている途中、または知らない版。使われているとみなす
                    _ => {
                        if req.unique {
                            return Err(anyhow!(
                                "{} is already registered by another juiz",
                                registered_name(&namespace, &name)
                            ));
                        }
                        break;
                    }
                }
            }
        }
        Err(anyhow!(
            "can not find a free name for {}",
            registered_name(&namespace, &req.name)
        ))
    }

    /// 読めた登録をすべて (パスと一緒に)。古い壊れたファイルは消し、知らない版は飛ばす
    fn read_all(&self) -> Vec<(PathBuf, Registration)> {
        let mut out = vec![];
        for p in registration_files(&self.dir) {
            match read_registration(&p) {
                ReadResult::Ok(r) => out.push((p, r)),
                ReadResult::UnknownFormat(f) => {
                    log::warn!("skipped {} (unknown format {f})", p.display());
                }
                ReadResult::Unreadable => {
                    if is_stale(&p) {
                        log::info!("removed a broken registration {}", p.display());
                        remove_registration(&self.dir, &p);
                    }
                }
                ReadResult::Missing => {}
            }
        }
        out
    }

    /// この機械の登録の一覧 (§9.3)。pid の死んだものは消す。`check` なら §6 の 2 も確かめる
    pub fn list(
        &self,
        check_profile: bool,
        probe: &dyn ProfileProbe,
        kinds: &[String],
    ) -> Vec<ListedRegistration> {
        let mut out = vec![];
        for (p, r) in self.read_all() {
            let liveness = if check_profile {
                check(&r, probe, kinds)
            } else {
                check_pid(&r)
            };
            match liveness {
                Liveness::Dead => {
                    remove_if_uuid(&self.dir, &p, &r.uuid);
                }
                Liveness::Alive => out.push(ListedRegistration {
                    registration: r,
                    alive: check_profile.then_some(true),
                    place: Place::ThisMachine,
                }),
                Liveness::NoResponse | Liveness::NotConnectable => out.push(ListedRegistration {
                    registration: r,
                    alive: Some(false),
                    place: Place::ThisMachine,
                }),
            }
        }
        out.sort_by_key(|l| l.registration.registered_name());
        out
    }

    /// 名前に当てはまる、生きている候補 (§7.1)。死んだものは消す
    pub fn candidates(
        &self,
        query: &NameQuery,
        probe: &dyn ProfileProbe,
        kinds: &[String],
    ) -> Vec<Candidate> {
        self.candidates_by(|r| query.matches(r), probe, kinds)
    }

    /// 条件に当てはまる、生きている候補。`juiz run` の依存は `package.name` で照合する (§7.1・§8)
    pub fn candidates_by(
        &self,
        filter: impl Fn(&Registration) -> bool,
        probe: &dyn ProfileProbe,
        kinds: &[String],
    ) -> Vec<Candidate> {
        let mut out = vec![];
        for (p, r) in self.read_all() {
            if !filter(&r) {
                continue;
            }
            match check(&r, probe, kinds) {
                Liveness::Alive => out.push(Candidate::local(r)),
                Liveness::Dead => {
                    remove_if_uuid(&self.dir, &p, &r.uuid);
                }
                Liveness::NoResponse | Liveness::NotConnectable => {}
            }
        }
        out
    }

    /// この機械の登録の uuid (生きているかは確かめない)。LAN の登録から同じ機械のものを外すのに使う (§11.4)
    pub fn uuids(&self) -> std::collections::BTreeSet<String> {
        self.read_all().into_iter().map(|(_, r)| r.uuid).collect()
    }

    /// 名前を 1 つの候補に決める (§7.2、対話しないとき)。候補が 0 か複数ならエラー
    pub fn resolve(
        &self,
        query: &NameQuery,
        probe: &dyn ProfileProbe,
        kinds: &[String],
    ) -> JuizResult<Candidate> {
        let mut c = self.candidates(query, probe, kinds);
        match c.len() {
            0 => Err(anyhow!(not_registered_message(query, Looked::default()))),
            1 => Ok(c.remove(0)),
            _ => Err(anyhow!(ambiguous_message(query, &c))),
        }
    }
}

/// この機械の登録のほかに見た所 (§7.1)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Looked {
    /// LAN (mDNS、§11)
    pub lan: bool,
    /// 知っている juiz (§12)
    pub peers: bool,
}

impl Looked {
    pub fn any(&self) -> bool {
        self.lan || self.peers
    }
}

/// 候補が 0 のときの文 (§7.2・§12.2)。`looked` はこの機械のほかに見た所
pub fn not_registered_message(query: &NameQuery, looked: Looked) -> String {
    match (looked.lan, looked.peers) {
        (false, false) => format!("{query} is not registered on this machine."),
        (true, false) => format!("{query} is not registered on this machine or the LAN."),
        (false, true) => format!("{query} is not registered on this machine or the peers."),
        (true, true) => {
            format!("{query} is not registered on this machine, the LAN or the peers.")
        }
    }
}

/// 候補が複数で、対話できないときの文 (§7.2)
pub fn ambiguous_message(query: &NameQuery, candidates: &[Candidate]) -> String {
    let list = candidates
        .iter()
        .map(Candidate::short)
        .collect::<Vec<_>>()
        .join(", ");
    let example = candidates
        .first()
        .map(|c| c.registration.registered_name())
        .unwrap_or_else(|| query.name.clone());
    format!("{query} is ambiguous: {list}. Write the namespace or the machine (ex., {example}).")
}
