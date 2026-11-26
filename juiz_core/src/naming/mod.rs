//! ネーミングサービス: 名前から System のブローカーの接続先を引く (docs/spec/naming.md)
//!
//! Step 1 (#616) は機械ごとの登録ファイル (§3〜§7)。動き続ける System は、ブローカーを立てた後に
//! `<dir>/names/<ns>/<名前>.json` を書き、止まるときに消す。引く側は pid と `system/profile` で死活を確かめる。
//!
//! Step 3 (#618) は名前で指す書き方 `name://…` (§9.1)、CLI の `-s` の名前 (§9.2)、API `naming/list`・`naming/resolve` (§9.4)。
//!
//! Step 4 (#619) は LAN の自動発見 (mDNS、§11)。広告は `option.naming.advertise: true` のときだけ、引く側は
//! feature `naming-mdns` があれば、この機械の登録で見つからなかったときに LAN も見る。
//!
//! Step 5 (#620) は知っている juiz に聞く (peers、§12)。`option.naming.peers`・`--peer`・`JUIZ_NAMING_PEERS` に
//! 書いた juiz の API `naming/list` を、LAN と同じ段で引く。

mod dir;
pub mod lan;
pub mod peers;
mod probe;
mod record;
mod registry;
mod spec;

pub use dir::{names_dir, runtime_dir, IPC_DIR_ENV, NAMES_DIR};
pub use probe::{
    check, check_pid, check_remote, core_kinds, pid_alive, CoreProbe, Liveness, Probe,
    ProfileProbe, LOCAL_ORDER, PROBE_TIMEOUT,
};
pub use record::{
    host_name, namespace_segments, read_registration, registered_name, registration_path,
    validate_name, ReadResult, RegisteredBroker, Registration, FORMAT,
};
pub use registry::{
    ambiguous_message, not_registered_message, Candidate, Conflict, ListedRegistration, Looked,
    NameQuery, NameRegistry, Place, RegisterRequest, Registered,
};
pub use spec::{split_broker_url, NameSpec, Resolved, NAME_SCHEME};

use crate::prelude::*;

/// conf の `option.naming` (§10)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingOptions {
    pub register: bool,
    pub unique: bool,
    /// LAN に広告する (§11、既定 false)
    pub advertise: bool,
    /// 知っている juiz の URL (§12)。書いたまま (読み方は [`peers::parse_peer`])
    pub peers: Vec<String>,
}

impl Default for NamingOptions {
    fn default() -> Self {
        NamingOptions {
            register: true,
            unique: false,
            advertise: false,
            peers: Vec::new(),
        }
    }
}

const KNOWN_OPTION_KEYS: [&str; 4] = ["register", "unique", "advertise", "peers"];

/// conf (System の manifest) の `option.naming` を読む。知らないキーは警告をログに出して無視する
pub fn naming_options(manifest: &Value) -> NamingOptions {
    let mut o = NamingOptions::default();
    let Some(n) = manifest.get("option").and_then(|v| v.get("naming")) else {
        return o;
    };
    let Some(map) = n.as_object() else {
        log::warn!("option.naming must be a map, but {n}. Ignored.");
        return o;
    };
    for (k, v) in map {
        match k.as_str() {
            "register" => match v.as_bool() {
                Some(b) => o.register = b,
                None => {
                    log::warn!("option.naming.register must be true or false, but {v}. Ignored.")
                }
            },
            "unique" => match v.as_bool() {
                Some(b) => o.unique = b,
                None => log::warn!("option.naming.unique must be true or false, but {v}. Ignored."),
            },
            "advertise" => match v.as_bool() {
                Some(b) => o.advertise = b,
                None => {
                    log::warn!("option.naming.advertise must be true or false, but {v}. Ignored.")
                }
            },
            "peers" => match v {
                Value::Array(a) => {
                    for p in a {
                        match p.as_str() {
                            Some(s) => o.peers.push(s.to_owned()),
                            None => log::warn!(
                                "option.naming.peers must be a list of URLs, but has {p}. Ignored."
                            ),
                        }
                    }
                }
                Value::String(s) => o.peers.push(s.clone()),
                Value::Null => {}
                v => log::warn!("option.naming.peers must be a list of URLs, but {v}. Ignored."),
            },
            k if KNOWN_OPTION_KEYS.contains(&k) => {}
            k => log::warn!("unknown key option.naming.{k} is ignored."),
        }
    }
    o
}

/// conf の `option.naming.register` を書き換える (`--no-register`、§10)
pub fn set_register_option(manifest: &mut Value, register: bool) {
    set_naming_option(manifest, "register", register)
}

/// conf の `option.naming.advertise` を書き換える (`--advertise`、§10・§11)
pub fn set_advertise_option(manifest: &mut Value, advertise: bool) {
    set_naming_option(manifest, "advertise", advertise)
}

fn set_naming_option(manifest: &mut Value, key: &str, value: bool) {
    let Some(obj) = manifest.as_object_mut() else {
        return;
    };
    let option = obj.entry("option").or_insert_with(|| jvalue!({}));
    if !option.is_object() {
        *option = jvalue!({});
    }
    let naming = option
        .as_object_mut()
        .unwrap()
        .entry("naming")
        .or_insert_with(|| jvalue!({}));
    if !naming.is_object() {
        *naming = jvalue!({});
    }
    naming
        .as_object_mut()
        .unwrap()
        .insert(key.to_owned(), jvalue!(value));
}

/// 登録名の名前の部分 (§4.1)。`juiz run` の `package.name`、無ければ conf の `name`
pub fn name_of_manifest(manifest: &Value) -> Option<String> {
    manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .or_else(|| manifest.get("name").and_then(Value::as_str))
        .map(str::to_owned)
}

/// System の既定ネームスペース (先頭・末尾の `/` は外す)
pub fn namespace_of_manifest(manifest: &Value) -> String {
    manifest
        .get("namespace")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_matches('/')
        .to_owned()
}

/// system/profile の `core_store.brokers` の形 (種類と名前) から、登録する `brokers` を作る (§3.3)
pub fn brokers_from_profiles<'a>(
    profiles: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<RegisteredBroker> {
    profiles
        .into_iter()
        .filter(|(t, _)| !matches!(*t, "local" | "core"))
        .map(|(t, n)| RegisteredBroker::new(t, n))
        .collect()
}

/// 動き続ける System を登録する (§4・§5.1)。
///
/// 登録しないとき (`register: false`、名前が無い・規則に合わない) は `Ok(None)`。
/// 登録に失敗しても警告を出して `Ok(None)`、`unique: true` のときだけエラーを返す。
pub fn register_system(
    manifest: &Value,
    uuid: &str,
    brokers: Vec<RegisteredBroker>,
    probe: &dyn ProfileProbe,
    kinds: &[String],
) -> JuizResult<Option<Registered>> {
    let opts = naming_options(manifest);
    if !opts.register {
        return Ok(None);
    }
    let Some(name) = name_of_manifest(manifest) else {
        return Ok(None);
    };
    let namespace = namespace_of_manifest(manifest);
    if let Err(e) = validate_name(&name).and_then(|_| namespace_segments(&namespace).map(|_| ())) {
        eprintln!(
            "warning: '{}' cannot be used as a registered name ({e}); this juiz is not registered in the name list",
            registered_name(&namespace, &name)
        );
        return Ok(None);
    }
    let req = RegisterRequest {
        name: name.clone(),
        namespace: namespace.clone(),
        uuid: uuid.to_owned(),
        package: manifest.get("package").filter(|p| p.is_object()).cloned(),
        brokers,
        unique: opts.unique,
    };
    let result = NameRegistry::open().and_then(|r| r.register(&req, probe, kinds));
    match result {
        Ok(reg) => {
            let rn = reg.registration.registered_name();
            if let Some(c) = &reg.conflict {
                eprintln!(
                    "warning: {} is already registered by another juiz (pid {}); this juiz is registered as {rn}",
                    c.registered_name, c.pid
                );
            }
            log::info!("registered as {rn}");
            Ok(Some(reg))
        }
        Err(e) if opts.unique => Err(e),
        Err(e) => {
            eprintln!(
                "warning: cannot register {} in the name list: {e}",
                registered_name(&namespace, &name)
            );
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests;

/// 動き続ける System を LAN に広告する (§11)。`option.naming.advertise` が true のときだけ。
///
/// 広告できないとき (feature が無い、名前が無い・規則に合わない、他の機械からつなげるブローカーが無い、mDNS が
/// 使えない) は警告を出して `None`。起動は止めない。
pub fn advertise_system(
    manifest: &Value,
    uuid: &str,
    brokers: Vec<RegisteredBroker>,
    registered: Option<&Registration>,
) -> Option<lan::Advertisement> {
    if !naming_options(manifest).advertise {
        return None;
    }
    // 登録したなら同じ中身 (名前に -2 を付けたもの) を出す。登録しないときは同じ規則で作る
    let reg = match registered {
        Some(r) => r.clone(),
        None => {
            let name = name_of_manifest(manifest)?;
            let namespace = namespace_of_manifest(manifest);
            if let Err(e) =
                validate_name(&name).and_then(|_| namespace_segments(&namespace).map(|_| ()))
            {
                eprintln!(
                    "warning: '{}' cannot be used as a registered name ({e}); this juiz is not advertised on the LAN",
                    registered_name(&namespace, &name)
                );
                return None;
            }
            Registration {
                format: FORMAT,
                name,
                namespace,
                uuid: uuid.to_owned(),
                pid: std::process::id(),
                host: host_name(),
                juiz_version: env!("CARGO_PKG_VERSION").to_owned(),
                started_at: record::now_rfc3339(),
                package: manifest.get("package").filter(|p| p.is_object()).cloned(),
                brokers,
            }
        }
    };
    let rn = reg.registered_name();
    if !lan::available() {
        eprintln!(
            "warning: {rn} is not advertised on the LAN: this juiz is built without the feature naming-mdns"
        );
        return None;
    }
    let conf_brokers: Vec<Value> = manifest
        .get("brokers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    match lan::advertise(&reg, &conf_brokers) {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("warning: cannot advertise {rn} on the LAN: {e}");
            None
        }
    }
}

/// LAN を見るか (§7.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanLookup {
    /// 見ない
    Never,
    /// この機械の登録で見つからなかったときだけ (既定)
    Fallback,
    /// この機械の登録と両方 (`--lan`)
    Always,
}

impl LanLookup {
    /// `--lan` の有無から
    pub fn from_flag(lan: bool) -> LanLookup {
        if lan {
            LanLookup::Always
        } else {
            LanLookup::Fallback
        }
    }
}

/// 別の機械の登録 (§7.1 の 2): `lan` なら LAN (feature `naming-mdns` のとき、§11.5)、`peers` に聞いたもの (§12.2)。
/// この機械の登録と同じ uuid のもの、LAN と peers で同じ uuid の 2 つ目は捨てる。答えない peer は警告を出す
pub fn remote_registrations(
    local: &NameRegistry,
    lan: bool,
    peers: &[peers::Peer],
) -> Vec<(Registration, Place)> {
    let lan = lan && lan::available();
    if !lan && peers.is_empty() {
        return vec![];
    }
    let mine = local.uuids();
    // LAN は 1.5 秒待つので、その間に peers に聞く
    let asking = {
        let peers = peers.to_vec();
        let mine = mine.clone();
        std::thread::spawn(move || peers::peer_registrations(&peers, &mine))
    };
    let mut seen = mine;
    let mut out = vec![];
    if lan {
        for r in lan::browse(lan::BROWSE_WAIT) {
            if seen.insert(r.uuid.clone()) {
                out.push((r, Place::Lan));
            }
        }
    }
    let (from_peers, warnings) = asking.join().unwrap_or_default();
    for w in warnings {
        log::warn!("{w}");
        eprintln!("warning: {w}");
    }
    for r in from_peers {
        if seen.insert(r.uuid.clone()) {
            out.push((r, Place::Peer));
        }
    }
    out
}

/// 別の機械 (LAN と peers) の登録のうち、条件に当てはまり生きているもの (§11.5・§12.2)
pub fn remote_candidates_by(
    local: &NameRegistry,
    filter: impl Fn(&Registration) -> bool,
    probe: &dyn ProfileProbe,
    kinds: &[String],
    peers: &[peers::Peer],
) -> Vec<Candidate> {
    remote_registrations(local, true, peers)
        .into_iter()
        .filter(|(r, _)| filter(r))
        .filter(|(r, _)| check_remote(r, probe, kinds) == Liveness::Alive)
        .map(|(registration, place)| Candidate {
            registration,
            place,
        })
        .collect()
}

/// 名前に当てはまる候補を、この機械の登録と LAN・peers から集める (§7.1)。戻り値の [`Looked`] はほかに見た所
pub fn find_candidates(
    query: &NameQuery,
    probe: &dyn ProfileProbe,
    kinds: &[String],
    lan: LanLookup,
    peers: &[peers::Peer],
) -> JuizResult<(Vec<Candidate>, Looked)> {
    let registry = NameRegistry::open()?;
    let mut out = registry.candidates(query, probe, kinds);
    let look = match lan {
        LanLookup::Never => false,
        LanLookup::Fallback => out.is_empty(),
        LanLookup::Always => true,
    };
    let looked = Looked {
        lan: look && lan::available(),
        peers: look && !peers.is_empty(),
    };
    if looked.any() {
        out.extend(remote_candidates_by(
            &registry,
            |r| query.matches(r),
            probe,
            kinds,
            if looked.peers { peers } else { &[] },
        ));
    }
    Ok((out, looked))
}

/// 名前を 1 つの候補に決める (§7.2、対話しない)。この機械で見つからなければ LAN と peers も見る
pub fn resolve_query(
    query: &NameQuery,
    probe: &dyn ProfileProbe,
    kinds: &[String],
    peers: &[peers::Peer],
) -> JuizResult<Candidate> {
    let (mut c, looked) = find_candidates(query, probe, kinds, LanLookup::Fallback, peers)?;
    match c.len() {
        0 => Err(juiz_sdk::anyhow::anyhow!(not_registered_message(
            query, looked
        ))),
        1 => Ok(c.remove(0)),
        _ => Err(juiz_sdk::anyhow::anyhow!(ambiguous_message(query, &c))),
    }
}

/// 一覧 (§9.3)。`lan` なら LAN で見つけたもの (`--lan`、`naming/list?lan=true`)、`peers` に聞いたものも足す
pub fn list_all(
    check_profile: bool,
    lan: bool,
    peers: &[peers::Peer],
    probe: &dyn ProfileProbe,
    kinds: &[String],
) -> JuizResult<Vec<ListedRegistration>> {
    let registry = NameRegistry::open()?;
    let mut out = registry.list(check_profile, probe, kinds);
    let mut remote: Vec<ListedRegistration> = remote_registrations(&registry, lan, peers)
        .into_iter()
        .filter_map(|(r, place)| {
            let alive = if check_profile {
                match check_remote(&r, probe, kinds) {
                    Liveness::Dead => return None,
                    Liveness::Alive => Some(true),
                    _ => Some(false),
                }
            } else {
                None
            };
            Some(ListedRegistration {
                registration: r,
                alive,
                place,
            })
        })
        .collect();
    remote.sort_by_key(|l| {
        (
            l.registration.registered_name(),
            l.registration.host.clone(),
        )
    });
    out.extend(remote);
    Ok(out)
}
