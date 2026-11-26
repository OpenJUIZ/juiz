//! 知っている juiz に聞く (peers、docs/spec/naming.md §12、issue #620)
//!
//! マルチキャストが通らない網 (Docker の bridge、サブネット越え) のため、知っている juiz の URL を
//! `option.naming.peers`・`--peer <URL>`・環境変数 [`PEERS_ENV`] に書く。引くときに各 peer の API
//! `naming/list?connected=true` (§9.4) を並行に呼び、その機械の登録と、peer がつないでいる別の機械の System を
//! 候補に足す。peer は普通の juiz で、新しいサーバーは作らない。
//!
//! peer が返すブローカーの host が `127.0.0.1`・`0.0.0.0` などなら、peer の URL の host に置き換える (§12.3)。
//! peer に聞けるのは http だけ (§12.1)。

use std::collections::BTreeSet;
use std::time::Duration;

use juiz_sdk::anyhow::anyhow;

use super::record::{validate_name, RegisteredBroker, Registration, FORMAT};
use crate::prelude::*;

/// peers を書く環境変数 (§12.1)。`,` か空白で区切る
pub const PEERS_ENV: &str = "JUIZ_NAMING_PEERS";

/// 1 つの peer の `naming/list` を待つ時間 (§12.2)。peer は並行に聞くので、全体でもこの長さ
pub const PEER_TIMEOUT: Duration = Duration::from_millis(1500);

/// peer の URL のポートを省いたとき (juiz の HTTP ブローカーの既定)
pub const DEFAULT_PEER_PORT: u16 = 8000;

/// 知っている juiz (§12.1)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// `192.168.0.10`・`robot2-pc`・`[fd00::1]`
    pub host: String,
    pub port: u16,
}

impl Peer {
    /// `http://<host>:<port>`
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.url())
    }
}

/// peer の URL を読む (§12.1)。`http://<host>[:<port>][/…]`・`<host>[:<port>]`。ポートを省けば 8000
pub fn parse_peer(s: &str) -> JuizResult<Peer> {
    let t = s.trim();
    let rest = match t.split_once("://") {
        Some(("http", r)) => r,
        Some((scheme, _)) => {
            return Err(anyhow!(
                "peer '{s}' is not an http URL. Write the HTTP broker of the juiz (ex., http://192.168.0.10:8000); {scheme} is not supported for peers"
            ))
        }
        None => t,
    };
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = if let Some(r) = authority.strip_prefix('[') {
        // [IPv6]:port
        let (h, after) = r
            .split_once(']')
            .ok_or_else(|| anyhow!("invalid peer '{s}'"))?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(anyhow!("invalid peer '{s}'")),
        };
        (format!("[{h}]"), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_owned(), Some(p)),
            None => (authority.to_owned(), None),
        }
    };
    if host.is_empty() || host == "[]" || host.contains(char::is_whitespace) {
        return Err(anyhow!(
            "invalid peer '{s}'. Write the URL of a juiz (ex., http://192.168.0.10:8000)"
        ));
    }
    let port = match port {
        None => DEFAULT_PEER_PORT,
        Some(p) => p
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| anyhow!("invalid port in peer '{s}'"))?,
    };
    Ok(Peer { host, port })
}

/// 環境変数 [`PEERS_ENV`] の値を分ける
pub fn split_peers_env(v: &str) -> Vec<String> {
    v.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 書いた peers を読む。読めないものは警告をログに出して飛ばし、同じものは 1 つにする
pub fn parse_peers<'a>(written: impl IntoIterator<Item = &'a str>) -> Vec<Peer> {
    let mut out: Vec<Peer> = vec![];
    for s in written {
        match parse_peer(s) {
            Ok(p) => {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
            Err(e) => log::warn!("{e}. Ignored."),
        }
    }
    out
}

/// conf (System の manifest) の `option.naming.peers` と環境変数 [`PEERS_ENV`] の peers (§12.1)。
/// `--peer` は conf に書き足してから呼ぶ ([`add_peers_option`])
pub fn peers_of_manifest(manifest: &Value) -> Vec<Peer> {
    let mut written = super::naming_options(manifest).peers;
    written.extend(env_peers());
    parse_peers(written.iter().map(String::as_str))
}

/// 環境変数 [`PEERS_ENV`] だけの peers (`juiz name`・CLI の `-s` は conf を持たない)
pub fn env_peers() -> Vec<String> {
    std::env::var(PEERS_ENV)
        .map(|v| split_peers_env(&v))
        .unwrap_or_default()
}

/// `--peer <URL>` を conf の `option.naming.peers` に足す (§10)。同じものは足さない
pub fn add_peers_option(manifest: &mut Value, peers: &[String]) {
    if peers.is_empty() {
        return;
    }
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
    let list = naming
        .as_object_mut()
        .unwrap()
        .entry("peers")
        .or_insert_with(|| jvalue!([]));
    if !list.is_array() {
        *list = match list.as_str() {
            Some(s) => jvalue!([s]),
            None => jvalue!([]),
        };
    }
    let a = list.as_array_mut().unwrap();
    for p in peers {
        if !a.iter().any(|v| v.as_str() == Some(p.as_str())) {
            a.push(jvalue!(p));
        }
    }
}

fn split_id(id: &str) -> Option<(&str, &str, Option<&str>, &str)> {
    let (scheme, rest) = id.split_once("://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (h, Some(p)),
        _ => (authority, None),
    };
    Some((scheme, host, port, path))
}

fn is_local_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h.is_empty()
        || h == "localhost"
        || h.parse::<std::net::IpAddr>()
            .map(|a| a.is_loopback() || a.is_unspecified())
            .unwrap_or(false)
}

/// peer が返したブローカーの ID を、こちらからつなげるものにする (§12.3)。
/// host が `127.0.0.1`・`localhost`・`::1`・`0.0.0.0`・`::`・空なら `host` (peer の URL の host) に置き換える。
/// ipc など別の機械からつなげない種類は None
pub fn fill_peer_host(id: &str, host: &str) -> Option<String> {
    let (scheme, h, port, path) = split_id(id)?;
    if matches!(scheme, "ipc" | "local" | "core") {
        return None;
    }
    let h = if is_local_host(h) { host } else { h };
    Some(match port {
        Some(p) => format!("{scheme}://{h}:{p}{path}"),
        None => format!("{scheme}://{h}{path}"),
    })
}

/// `naming/list` の応答 (§9.3 の `--json` と同じ形の配列) から登録を作る。ブローカーの host は [`fill_peer_host`]。
/// 知らない版・名前が規則に合わないものは飛ばす
pub fn registrations_from_list(v: &Value, host: &str) -> Vec<Registration> {
    let v = match v {
        Value::Object(m) if m.len() == 1 && m.contains_key("__value__") => &m["__value__"],
        v => v,
    };
    let Some(a) = v.as_array() else {
        return vec![];
    };
    a.iter()
        .filter_map(|e| serde_json::from_value::<Registration>(e.clone()).ok())
        .filter(|r| r.format == FORMAT && validate_name(&r.name).is_ok())
        .map(|mut r| {
            r.brokers = r
                .brokers
                .iter()
                .filter_map(|b| {
                    let id = fill_peer_host(&b.id, host)?;
                    let (t, rest) = id.split_once("://")?;
                    Some(RegisteredBroker::new(t, rest))
                })
                .collect();
            r
        })
        .collect()
}

/// `http://<host>:<port>/api/naming/list[?connected=true]` を取る
pub fn fetch_list(host: &str, port: u16, connected: bool) -> JuizResult<Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(PEER_TIMEOUT)
        .no_proxy()
        .build()?;
    let q = if connected { "?connected=true" } else { "" };
    let url = format!("http://{host}:{port}/api/naming/list{q}");
    let r = client.get(&url).send()?;
    if !r.status().is_success() {
        return Err(anyhow!(
            "{} (a juiz older than the name list does not have naming/list)",
            r.status()
        ));
    }
    Ok(r.json::<Value>()?)
}

/// 1 つの peer に聞く
fn ask_peer(peer: &Peer) -> JuizResult<Vec<Registration>> {
    let v = fetch_list(&peer.host, peer.port, true)?;
    Ok(registrations_from_list(&v, &peer.host))
}

/// peers に並行に聞き、登録を集める (§12.2)。`skip` の uuid (この機械の登録など) と、同じ uuid の 2 つ目は捨てる。
/// 答えない peer は 2 つ目の戻り値に `<peer> did not answer: <理由>` を入れる (呼ぶ側が警告にする)
pub fn peer_registrations(
    peers: &[Peer],
    skip: &BTreeSet<String>,
) -> (Vec<Registration>, Vec<String>) {
    if peers.is_empty() {
        return (vec![], vec![]);
    }
    // 呼ぶ側が tokio の中かどうかによらないよう、peer ごとに別スレッドで聞く
    let handles: Vec<_> = peers
        .iter()
        .cloned()
        .map(|p| std::thread::spawn(move || (p.clone(), ask_peer(&p))))
        .collect();
    let mut seen = skip.clone();
    let mut out = vec![];
    let mut warnings = vec![];
    for h in handles {
        match h.join() {
            Ok((_, Ok(regs))) => {
                for r in regs {
                    if seen.insert(r.uuid.clone()) {
                        out.push(r);
                    }
                }
            }
            Ok((p, Err(e))) => warnings.push(format!("peer {p} did not answer: {e}")),
            Err(_) => warnings.push("asking a peer panicked".to_owned()),
        }
    }
    (out, warnings)
}

/// peer の側: つないでいる System (subsystems・mastersystem) のうち、別の機械に http でつないだものの登録 (§12.4)。
///
/// `connected` は (uuid, ブローカーの種類, ブローカーの名前)。その System の `naming/list` を聞き、uuid が同じ要素を
/// 返す。ブローカーの host は、つないでいる URL の host に置き換える。`skip` の uuid は返さない
pub fn connected_registrations(
    connected: &[(String, String, String)],
    skip: &BTreeSet<String>,
) -> Vec<Registration> {
    let targets: Vec<(String, String, u16)> = connected
        .iter()
        .filter(|(uuid, t, _)| t == "http" && !skip.contains(uuid))
        .filter_map(|(uuid, _, name)| {
            let p = parse_peer(name).ok()?;
            // 同じ機械の System は、この機械の登録で返している
            (!is_local_host(&p.host)).then(|| (uuid.clone(), p.host, p.port))
        })
        .collect();
    let handles: Vec<_> = targets
        .into_iter()
        .map(|(uuid, host, port)| {
            std::thread::spawn(move || {
                let v = match fetch_list(&host, port, false) {
                    Ok(v) => v,
                    Err(e) => {
                        log::debug!(
                            "naming/list of the connected System at {host}:{port} failed: {e}"
                        );
                        return None;
                    }
                };
                registrations_from_list(&v, &host)
                    .into_iter()
                    .find(|r| r.uuid == uuid)
            })
        })
        .collect();
    let mut seen = skip.clone();
    handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .filter(|r| seen.insert(r.uuid.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_peers_in_the_forms() {
        let p = |s| parse_peer(s).map(|p| p.url());
        assert_eq!(
            p("http://192.168.0.10:8000").unwrap(),
            "http://192.168.0.10:8000"
        );
        assert_eq!(
            p("http://robot2-pc:8100/").unwrap(),
            "http://robot2-pc:8100"
        );
        assert_eq!(p("robot2-pc").unwrap(), "http://robot2-pc:8000");
        assert_eq!(p("10.0.0.5:8001").unwrap(), "http://10.0.0.5:8001");
        assert_eq!(p("http://[fd00::1]:8001").unwrap(), "http://[fd00::1]:8001");
        assert_eq!(p("[fd00::1]").unwrap(), "http://[fd00::1]:8000");
        assert!(p("qmp://10.0.0.5:9001")
            .unwrap_err()
            .to_string()
            .contains("not an http URL"));
        assert!(p("http://:8000").is_err());
        assert!(p("robot2-pc:x").is_err());
        assert!(p("robot2-pc:0").is_err());
        assert_eq!(
            split_peers_env("http://a:8000, b:8001  c"),
            vec!["http://a:8000", "b:8001", "c"]
        );
        let ps = parse_peers(["a", "http://a:8000", "bad:x", "b"]);
        assert_eq!(
            ps.iter().map(Peer::url).collect::<Vec<_>>(),
            vec!["http://a:8000", "http://b:8000"]
        );
    }

    #[test]
    fn peers_option() {
        let mut m = jvalue!({"name": "app"});
        add_peers_option(&mut m, &["http://a:8000".to_owned()]);
        add_peers_option(&mut m, &["http://a:8000".to_owned(), "b:8001".to_owned()]);
        assert_eq!(
            m["option"]["naming"]["peers"],
            jvalue!(["http://a:8000", "b:8001"])
        );
        assert_eq!(
            super::super::naming_options(&m).peers,
            vec!["http://a:8000", "b:8001"]
        );
        // 1 つだけなら文字列でも書ける
        let m = jvalue!({"option": {"naming": {"peers": "c"}}});
        assert_eq!(super::super::naming_options(&m).peers, vec!["c"]);
    }

    #[test]
    fn hosts_are_filled_with_the_peer() {
        assert_eq!(
            fill_peer_host("http://127.0.0.1:8001", "10.0.0.5").as_deref(),
            Some("http://10.0.0.5:8001")
        );
        assert_eq!(
            fill_peer_host("zenoh://0.0.0.0:7448", "10.0.0.5").as_deref(),
            Some("zenoh://10.0.0.5:7448")
        );
        assert_eq!(
            fill_peer_host("qmp://192.168.0.12:9001", "10.0.0.5").as_deref(),
            Some("qmp://192.168.0.12:9001")
        );
        assert_eq!(fill_peer_host("ipc://mobile_sim", "10.0.0.5"), None);
        let list = jvalue!([
            {
                "format": 1, "name": "mobile_sim", "namespace": "robot2", "uuid": "u1",
                "pid": 42, "host": "robot2-pc", "juiz_version": "0.3.6",
                "started_at": "2026-11-25T06:00:00Z",
                "package": {"name": "mobile_sim", "version": "0.4.0", "kind": "base"},
                "brokers": [
                    {"type_name": "ipc", "id": "ipc://mobile_sim"},
                    {"type_name": "http", "id": "http://127.0.0.1:8001", "port": 8001}
                ],
                "machine": "this"
            },
            {"format": 2, "name": "future", "namespace": "", "uuid": "u2", "pid": 1, "host": "h",
             "juiz_version": "9", "started_at": "", "brokers": []},
            {"bad": true}
        ]);
        let regs = registrations_from_list(&jvalue!({"__value__": list}), "10.0.0.5");
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].registered_name(), "robot2/mobile_sim");
        assert_eq!(regs[0].brokers.len(), 1);
        assert_eq!(regs[0].brokers[0].id, "http://10.0.0.5:8001");
        assert_eq!(regs[0].brokers[0].port, Some(8001));
    }

    #[test]
    fn a_peer_that_does_not_answer_is_a_warning() {
        // 使われていないはずのポート
        let peers = vec![Peer {
            host: "127.0.0.1".into(),
            port: 1,
        }];
        let (regs, warnings) = peer_registrations(&peers, &BTreeSet::new());
        assert!(regs.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].starts_with("peer http://127.0.0.1:1 did not answer"),
            "{warnings:?}"
        );
    }
}
