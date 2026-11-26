//! LAN の自動発見 (DNS-SD / mDNS、docs/spec/naming.md §11、issue #619)
//!
//! - 広告: `option.naming.advertise: true` か `--advertise` の System が、`_juiz._tcp` のサービスを 1 つ出す。
//!   TXT に登録の中身 (§3.2) を載せる。止めるときに goodbye (TTL 0) を送る。
//! - 発見: `_juiz._tcp` を [`BROWSE_WAIT`] の間だけ探し、TXT から登録を作り直す。ブローカーの host が
//!   `0.0.0.0` などなら、mDNS の応答の住所に置き換える (§3.3)。
//!
//! mDNS を話す部分は feature `naming-mdns` のときだけ。TXT の読み書きは feature によらずに使える (テストのため)。

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

use super::record::{validate_name, RegisteredBroker, Registration, FORMAT};
use crate::prelude::*;

/// DNS-SD のサービス種別 (§11)
pub const SERVICE_TYPE: &str = "_juiz._tcp.local.";

/// 発見で応答を待つ時間 (§11.5)。mDNS の応答者は同じレコードを 1 秒に 1 回しか流さない (RFC 6762 §6) ので、
/// 直前に別の juiz が引いた直後でも、1 秒後の問い合わせの再送への応答を受け取れるように 1.5 秒にする
pub const BROWSE_WAIT: Duration = Duration::from_millis(1500);

/// TXT の 1 項目 (`<キー>=<値>`) の長さの上限 (RFC 6763 §6.1)
pub const TXT_ITEM_MAX: usize = 255;

/// LAN から引く登録に付ける、他の機械のブローカーを選ぶ順 (§7.3)
pub const REMOTE_ORDER: [&str; 3] = ["zenoh", "qmp", "http"];

/// 広告に載せない (他の機械からつなげない) ブローカーの種類
fn is_local_only(type_name: &str) -> bool {
    matches!(type_name, "ipc" | "local" | "core")
}

/// ブローカーの ID の host が、この機械の中だけを指すか (`127.0.0.1`・`localhost`・`::1`)
fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h == "localhost"
        || h.parse::<IpAddr>()
            .map(|a| a.is_loopback())
            .unwrap_or(false)
}

/// ID を `(scheme, host, port, path)` に分ける。ipc など authority の無いものは None
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

/// 広告に載せるブローカー (§11.3)。
///
/// ブローカーの ID の host は、待ち受けが `0.0.0.0` でも `127.0.0.1` になることがある (http・zenoh)。
/// そこで ID の host が `127.0.0.1` などのときは conf の `brokers` を見て、同じ種類の要素がすべて `host` に
/// この機械の中だけの住所 (`127.0.0.1`・`localhost`・`::1`) を書いていれば載せず、そうでなければ (`host` を
/// 書いていない、`0.0.0.0` など) 待ち受けはすべての住所とみなして host を `0.0.0.0` にして載せる
/// (受け手が応答の住所に置き換える)。ipc は載せない。
pub fn advertised_brokers(
    brokers: &[RegisteredBroker],
    conf_brokers: &[Value],
) -> Vec<RegisteredBroker> {
    let loopback_only = |type_name: &str| {
        let hosts: Vec<Option<&str>> = conf_brokers
            .iter()
            .filter(|b| b.get("type_name").and_then(Value::as_str) == Some(type_name))
            .map(|b| b.get("host").and_then(Value::as_str))
            .collect();
        !hosts.is_empty() && hosts.iter().all(|h| h.is_some_and(is_loopback_host))
    };
    brokers
        .iter()
        .filter(|b| !is_local_only(&b.type_name))
        .filter_map(|b| {
            let (scheme, host, port, path) = split_id(&b.id)?;
            if !is_loopback_host(host) {
                return Some(b.clone());
            }
            if loopback_only(&b.type_name) {
                return None;
            }
            let id = match port {
                Some(p) => format!("{scheme}://0.0.0.0:{p}{path}"),
                None => format!("{scheme}://0.0.0.0{path}"),
            };
            Some(RegisteredBroker {
                type_name: b.type_name.clone(),
                id,
                port: b.port,
            })
        })
        .collect()
}

/// TXT を作った結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Txt {
    pub items: Vec<(String, String)>,
    /// 長すぎて載せなかった項目の警告
    pub warnings: Vec<String>,
}

fn item_len(k: &str, v: &str) -> usize {
    k.len() + 1 + v.len()
}

/// 登録の中身から TXT を作る (§11.2)。`brokers` は [`advertised_brokers`] を通したもの。
///
/// 必須の項目が 255 バイトを越えれば Err (広告しない)。`package` の項目とブローカーは、越えたものだけ載せない。
pub fn txt_from_registration(reg: &Registration) -> Result<Txt, String> {
    let mut items: Vec<(String, String)> = vec![];
    let mut warnings = vec![];
    let required = [
        ("f", FORMAT.to_string()),
        ("name", reg.name.clone()),
        ("ns", reg.namespace.clone()),
        ("uuid", reg.uuid.clone()),
        ("pid", reg.pid.to_string()),
        ("host", reg.host.clone()),
        ("v", reg.juiz_version.clone()),
        ("t", reg.started_at.clone()),
    ];
    for (k, v) in required {
        if item_len(k, &v) > TXT_ITEM_MAX {
            return Err(format!(
                "'{k}' is longer than {TXT_ITEM_MAX} bytes in the TXT record"
            ));
        }
        items.push((k.to_owned(), v));
    }
    let mut optional = vec![];
    if let Some(n) = reg.package_name() {
        optional.push(("pkg".to_owned(), n.to_owned()));
    }
    if let Some(v) = reg.package_version() {
        optional.push(("pkgv".to_owned(), v.to_owned()));
    }
    if let Some(k) = reg.package_kind() {
        optional.push(("kind".to_owned(), k.to_owned()));
    }
    for (i, b) in reg.brokers.iter().enumerate() {
        optional.push((format!("b{i}"), b.id.clone()));
    }
    for (k, v) in optional {
        if item_len(&k, &v) > TXT_ITEM_MAX {
            warnings.push(format!(
                "'{k}={v}' is longer than {TXT_ITEM_MAX} bytes and is not advertised"
            ));
            continue;
        }
        items.push((k, v));
    }
    Ok(Txt { items, warnings })
}

/// 応答の住所から、ブローカーの host に入れるものを 1 つ選ぶ (§11.3)。
///
/// IPv4 (リンクローカルでないもの → リンクローカル) を先に、無ければ IPv6 (リンクローカルでないもの)。
pub fn choose_address(addresses: &[IpAddr]) -> Option<String> {
    let mut v4: Vec<_> = addresses
        .iter()
        .filter_map(|a| match a {
            IpAddr::V4(a) if !a.is_loopback() && !a.is_unspecified() => Some(*a),
            _ => None,
        })
        .collect();
    v4.sort_by_key(|a| (a.is_link_local(), *a));
    if let Some(a) = v4.first() {
        return Some(a.to_string());
    }
    let mut v6: Vec<_> = addresses
        .iter()
        .filter_map(|a| match a {
            IpAddr::V6(a)
                if !a.is_loopback()
                    && !a.is_unspecified()
                    && (a.segments()[0] & 0xffc0) != 0xfe80 =>
            {
                Some(*a)
            }
            _ => None,
        })
        .collect();
    v6.sort();
    v6.first().map(|a| format!("[{a}]"))
}

/// ブローカーの ID の host を、別の機械からつなげるものにする (§3.3)。
/// `0.0.0.0` などは `address` に置き換え、この機械の中だけを指すもの (`127.0.0.1`) は None
pub fn fill_host(id: &str, address: Option<&str>) -> Option<String> {
    let (scheme, host, port, path) = split_id(id)?;
    if is_local_only(scheme) || is_loopback_host(host) {
        return None;
    }
    let host = match host {
        "" | "0.0.0.0" | "::" | "[::]" => address?,
        h => h,
    };
    Some(match port {
        Some(p) => format!("{scheme}://{host}:{p}{path}"),
        None => format!("{scheme}://{host}{path}"),
    })
}

/// TXT と応答の住所から登録を作り直す (§11.2・§11.3)。足りない・知らない版なら None
pub fn registration_from_txt(
    txt: &BTreeMap<String, String>,
    addresses: &[IpAddr],
) -> Option<Registration> {
    let get = |k: &str| txt.get(k).cloned();
    let format: u64 = get("f")?.parse().ok()?;
    if format != FORMAT {
        log::debug!("skipped a juiz on the LAN with an unknown format {format}");
        return None;
    }
    let name = get("name")?;
    validate_name(&name).ok()?;
    let namespace = get("ns").unwrap_or_default();
    super::record::namespace_segments(&namespace).ok()?;
    let address = choose_address(addresses);
    let mut indexed: Vec<(u32, String)> = txt
        .iter()
        .filter_map(|(k, v)| {
            let i: u32 = k.strip_prefix('b')?.parse().ok()?;
            Some((i, v.clone()))
        })
        .collect();
    indexed.sort();
    let brokers = indexed
        .into_iter()
        .filter_map(|(_, id)| {
            let url = fill_host(&id, address.as_deref())?;
            let (type_name, rest) = url.split_once("://")?;
            Some(RegisteredBroker::new(type_name, rest))
        })
        .collect();
    let mut package = serde_json::Map::new();
    for (k, key) in [("pkg", "name"), ("pkgv", "version"), ("kind", "kind")] {
        if let Some(v) = get(k) {
            package.insert(key.to_owned(), serde_json::Value::String(v));
        }
    }
    Some(Registration {
        format,
        name,
        namespace: namespace.trim_matches('/').to_owned(),
        uuid: get("uuid")?,
        pid: get("pid").and_then(|p| p.parse().ok()).unwrap_or(0),
        host: get("host").unwrap_or_default(),
        juiz_version: get("v").unwrap_or_default(),
        started_at: get("t").unwrap_or_default(),
        package: (!package.is_empty()).then_some(serde_json::Value::Object(package)),
        brokers,
    })
}

/// DNS-SD のインスタンス名 (§11.4)。登録名の `/` を `.` にし、54 バイトまでに切って uuid の先頭 8 文字を付ける
pub fn instance_name(reg: &Registration) -> String {
    let mut base = reg.registered_name().replace('/', ".");
    while base.len() > 54 {
        base.pop();
    }
    let short: String = reg.uuid.chars().filter(|c| *c != '-').take(8).collect();
    format!("{base}-{short}")
}

/// mDNS の A・AAAA レコードに使う host 名。OS の応答者 (Avahi・Bonjour) とぶつからないよう、機械の名前は使わない
pub fn mdns_host_name(reg: &Registration) -> String {
    let short: String = reg.uuid.chars().filter(|c| *c != '-').take(8).collect();
    format!("juiz-{short}.local.")
}

/// SRV のポート。載せたブローカーのうち、http → qmp → zenoh の順で最初に見つかったもの
pub fn srv_port(brokers: &[RegisteredBroker]) -> Option<u16> {
    for k in ["http", "qmp", "zenoh"] {
        if let Some(p) = brokers
            .iter()
            .find(|b| b.type_name == k)
            .and_then(|b| b.port)
        {
            return Some(p);
        }
    }
    brokers.iter().find_map(|b| b.port)
}

/// この juiz が LAN を引けるか (feature `naming-mdns`)
pub const fn available() -> bool {
    cfg!(feature = "naming-mdns")
}

#[cfg(feature = "naming-mdns")]
mod imp {
    use std::collections::BTreeMap;
    use std::net::IpAddr;
    use std::time::{Duration, Instant};

    use juiz_sdk::anyhow::anyhow;
    use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

    use super::*;

    /// LAN への広告。drop で goodbye を送って止める (§11.6)
    pub struct Advertisement {
        daemon: ServiceDaemon,
        fullname: String,
        registered_name: String,
    }

    impl std::fmt::Debug for Advertisement {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Advertisement")
                .field("fullname", &self.fullname)
                .finish()
        }
    }

    impl Advertisement {
        /// DNS-SD の完全な名前 (`mobile_sim-1b2c3d4e._juiz._tcp.local.`)
        pub fn fullname(&self) -> &str {
            &self.fullname
        }
    }

    impl Drop for Advertisement {
        fn drop(&mut self) {
            match self.daemon.unregister(&self.fullname) {
                Ok(rx) => {
                    // goodbye を送り終わるのを少しだけ待つ
                    let _ = rx.recv_timeout(Duration::from_secs(1));
                }
                Err(e) => log::warn!("mDNS unregister of {} failed: {e}", self.fullname),
            }
            if let Ok(rx) = self.daemon.shutdown() {
                let _ = rx.recv_timeout(Duration::from_secs(1));
            }
            log::info!("stopped advertising {} on the LAN", self.registered_name);
        }
    }

    /// 登録の中身を LAN に広告する (§11)。`reg.brokers` から他の機械がつなげないものは外す ([`advertised_brokers`])
    pub fn advertise(reg: &Registration, conf_brokers: &[Value]) -> JuizResult<Advertisement> {
        let brokers = advertised_brokers(&reg.brokers, conf_brokers);
        let Some(port) = srv_port(&brokers) else {
            return Err(anyhow!(
                "it has no broker that other machines can connect to (http, qmp or zenoh not listening only on 127.0.0.1)"
            ));
        };
        let mut r = reg.clone();
        r.brokers = brokers;
        let txt = txt_from_registration(&r).map_err(|e| anyhow!(e))?;
        for w in &txt.warnings {
            eprintln!("warning: {w}");
        }
        let props: Vec<(&str, &str)> = txt
            .items
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name(&r),
            &mdns_host_name(&r),
            "",
            port,
            &props[..],
        )
        .map_err(|e| anyhow!("{e}"))?
        .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        let daemon = ServiceDaemon::new().map_err(|e| anyhow!("can not start mDNS: {e}"))?;
        daemon
            .register(info)
            .map_err(|e| anyhow!("mDNS register failed: {e}"))?;
        let registered_name = r.registered_name();
        log::info!("advertised {registered_name} on the LAN as {fullname}");
        // HTTP ブローカーには認証が無いので、LAN に出したことを利用者に見えるようにする (§11.1)
        eprintln!(
            "note: {registered_name} is advertised on the LAN (mDNS). Other machines on the LAN can find and call it."
        );
        Ok(Advertisement {
            daemon,
            fullname,
            registered_name,
        })
    }

    /// `_juiz._tcp` を `wait` の間だけ探し、見つかった登録を返す (§11.5)。同じ uuid は 1 つにまとめる
    pub fn browse(wait: Duration) -> Vec<Registration> {
        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("can not start mDNS to find juiz on the LAN: {e}");
                return vec![];
            }
        };
        let rx = match daemon.browse(SERVICE_TYPE) {
            Ok(rx) => rx,
            Err(e) => {
                log::warn!("mDNS browse failed: {e}");
                let _ = daemon.shutdown();
                return vec![];
            }
        };
        let deadline = Instant::now() + wait;
        // 同じサービスの応答は、住所が揃うにつれて何度か届く。ブローカーが多く埋まったものを残す
        let mut out: Vec<Registration> = vec![];
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match rx.recv_timeout(deadline - now) {
                Ok(ServiceEvent::ServiceResolved(s)) => {
                    let txt: BTreeMap<String, String> = s
                        .txt_properties
                        .iter()
                        .map(|p| (p.key().to_owned(), p.val_str().to_owned()))
                        .collect();
                    let addrs: Vec<IpAddr> = s.addresses.iter().map(|a| a.to_ip_addr()).collect();
                    match registration_from_txt(&txt, &addrs) {
                        Some(r) => match out.iter_mut().find(|o| o.uuid == r.uuid) {
                            Some(o) => {
                                if r.brokers.len() > o.brokers.len() {
                                    *o = r;
                                }
                            }
                            None => {
                                log::debug!(
                                    "found {} on the LAN ({})",
                                    r.registered_name(),
                                    s.fullname
                                );
                                out.push(r);
                            }
                        },
                        None => log::debug!("skipped {} on the LAN (not a juiz TXT)", s.fullname),
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = daemon.stop_browse(SERVICE_TYPE);
        if let Ok(rx) = daemon.shutdown() {
            let _ = rx.recv_timeout(Duration::from_millis(500));
        }
        out
    }
}

#[cfg(not(feature = "naming-mdns"))]
mod imp {
    use std::time::Duration;

    use juiz_sdk::anyhow::anyhow;

    use super::Registration;
    use crate::prelude::*;

    /// feature `naming-mdns` の無い juiz では作れない
    #[derive(Debug)]
    pub struct Advertisement;

    impl Advertisement {
        pub fn fullname(&self) -> &str {
            ""
        }
    }

    pub fn advertise(_reg: &Registration, _conf_brokers: &[Value]) -> JuizResult<Advertisement> {
        Err(anyhow!(
            "this juiz is built without the feature naming-mdns"
        ))
    }

    pub fn browse(_wait: Duration) -> Vec<Registration> {
        vec![]
    }
}

pub use imp::{advertise, browse, Advertisement};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{Candidate, NameQuery};

    fn reg() -> Registration {
        Registration {
            format: FORMAT,
            name: "mobile_sim".into(),
            namespace: "robot2".into(),
            uuid: "1b2c3d4e-0000-4000-8000-000000000000".into(),
            pid: 4242,
            host: "robot2-pc".into(),
            juiz_version: "0.3.6".into(),
            started_at: "2026-11-25T06:00:00Z".into(),
            package: Some(jvalue!({"name": "mobile_sim", "version": "0.4.0", "kind": "base"})),
            brokers: vec![
                RegisteredBroker::new("ipc", "mobile_sim"),
                RegisteredBroker::new("http", "127.0.0.1:8001"),
                RegisteredBroker::new("qmp", "0.0.0.0:9001"),
                RegisteredBroker::new("zenoh", "192.168.0.12:7448"),
            ],
        }
    }

    fn ids(b: &[RegisteredBroker]) -> Vec<&str> {
        b.iter().map(|b| b.id.as_str()).collect()
    }

    #[test]
    fn advertised_brokers_follow_the_conf_host() {
        let r = reg();
        // conf に host が無い (既定 0.0.0.0): 127.0.0.1 の ID も全部の住所で待ち受けているとみなす。ipc は載せない
        let b = advertised_brokers(&r.brokers, &[]);
        assert_eq!(
            ids(&b),
            vec![
                "http://0.0.0.0:8001",
                "qmp://0.0.0.0:9001",
                "zenoh://192.168.0.12:7448"
            ]
        );
        assert_eq!(b[0].port, Some(8001));
        // conf で 127.0.0.1 だけで待ち受けると書いた種類は載せない
        let conf =
            vec![jvalue!({"type_name": "http", "name": "h", "host": "127.0.0.1", "port": 8001})];
        assert_eq!(
            ids(&advertised_brokers(&r.brokers, &conf)),
            vec!["qmp://0.0.0.0:9001", "zenoh://192.168.0.12:7448"]
        );
        let conf =
            vec![jvalue!({"type_name": "http", "name": "h", "host": "0.0.0.0", "port": 8001})];
        assert_eq!(advertised_brokers(&r.brokers, &conf).len(), 3);
        assert_eq!(srv_port(&advertised_brokers(&r.brokers, &[])), Some(8001));
        assert_eq!(srv_port(&[RegisteredBroker::new("ipc", "x")]), None);
    }

    #[test]
    fn txt_round_trip_fills_the_host() {
        let mut r = reg();
        r.brokers = advertised_brokers(&r.brokers, &[]);
        let txt = txt_from_registration(&r).unwrap();
        assert!(txt.warnings.is_empty());
        let keys: Vec<&str> = txt.items.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "f", "name", "ns", "uuid", "pid", "host", "v", "t", "pkg", "pkgv", "kind", "b0",
                "b1", "b2"
            ]
        );
        let map: BTreeMap<String, String> = txt.items.into_iter().collect();
        let addrs: Vec<IpAddr> = vec![
            "127.0.0.1".parse().unwrap(),
            "fe80::1".parse().unwrap(),
            "192.168.0.12".parse().unwrap(),
        ];
        let back = registration_from_txt(&map, &addrs).unwrap();
        assert_eq!(back.registered_name(), "robot2/mobile_sim");
        assert_eq!(back.uuid, r.uuid);
        assert_eq!(back.pid, 4242);
        assert_eq!(back.host, "robot2-pc");
        assert_eq!(back.package_version(), Some("0.4.0"));
        assert_eq!(back.package_kind(), Some("base"));
        assert_eq!(
            ids(&back.brokers),
            vec![
                "http://192.168.0.12:8001",
                "qmp://192.168.0.12:9001",
                "zenoh://192.168.0.12:7448"
            ]
        );
        // 住所が無い (まだ届いていない) なら、0.0.0.0 のブローカーは埋められない
        let partial = registration_from_txt(&map, &[]).unwrap();
        assert_eq!(ids(&partial.brokers), vec!["zenoh://192.168.0.12:7448"]);
        // 知らない版・足りないものは読まない
        let mut m2 = map.clone();
        m2.insert("f".into(), "2".into());
        assert!(registration_from_txt(&m2, &addrs).is_none());
        let mut m3 = map.clone();
        m3.remove("uuid");
        assert!(registration_from_txt(&m3, &addrs).is_none());
        let mut m4 = map;
        m4.insert("name".into(), "bad name".into());
        assert!(registration_from_txt(&m4, &addrs).is_none());
    }

    #[test]
    fn too_long_items() {
        let mut r = reg();
        r.brokers = vec![RegisteredBroker::new(
            "http",
            &format!("0.0.0.0:8001/{}", "x".repeat(260)),
        )];
        let txt = txt_from_registration(&r).unwrap();
        assert_eq!(txt.warnings.len(), 1, "{:?}", txt.warnings);
        assert!(txt.items.iter().all(|(k, _)| k != "b0"));
        r.namespace = vec!["a".repeat(60); 5].join("/");
        assert!(txt_from_registration(&r).is_err());
    }

    #[test]
    fn addresses_and_hosts() {
        let a = |s: &[&str]| {
            s.iter()
                .map(|x| x.parse::<IpAddr>().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            choose_address(&a(&["169.254.1.2", "10.0.0.5"])).as_deref(),
            Some("10.0.0.5")
        );
        assert_eq!(
            choose_address(&a(&["169.254.1.2"])).as_deref(),
            Some("169.254.1.2")
        );
        assert_eq!(
            choose_address(&a(&["fe80::1", "2001:db8::5"])).as_deref(),
            Some("[2001:db8::5]")
        );
        assert_eq!(choose_address(&a(&["127.0.0.1", "fe80::1"])), None);
        assert_eq!(
            fill_host("http://0.0.0.0:8001", Some("10.0.0.5")).as_deref(),
            Some("http://10.0.0.5:8001")
        );
        assert_eq!(
            fill_host("http://[::]:8001", Some("[2001:db8::5]")).as_deref(),
            Some("http://[2001:db8::5]:8001")
        );
        assert_eq!(fill_host("http://127.0.0.1:8001", Some("10.0.0.5")), None);
        assert_eq!(fill_host("ipc://mobile_sim", Some("10.0.0.5")), None);
        assert_eq!(
            fill_host("qmp://10.0.0.9:9001", None).as_deref(),
            Some("qmp://10.0.0.9:9001")
        );
    }

    #[test]
    fn instance_names() {
        let mut r = reg();
        assert_eq!(instance_name(&r), "robot2.mobile_sim-1b2c3d4e");
        assert_eq!(mdns_host_name(&r), "juiz-1b2c3d4e.local.");
        r.name = "n".repeat(64);
        assert!(instance_name(&r).len() <= 63, "{}", instance_name(&r));
    }

    #[test]
    fn lan_candidates_use_the_remote_order() {
        let mut r = reg();
        r.brokers = registration_from_txt(
            &txt_from_registration(&Registration {
                brokers: advertised_brokers(&r.brokers, &[]),
                ..r.clone()
            })
            .unwrap()
            .items
            .into_iter()
            .collect(),
            &["192.168.0.12".parse().unwrap()],
        )
        .unwrap()
        .brokers;
        r.brokers
            .insert(0, RegisteredBroker::new("ipc", "mobile_sim"));
        let all = vec![
            "ipc".to_owned(),
            "http".to_owned(),
            "qmp".to_owned(),
            "zenoh".to_owned(),
        ];
        let c = Candidate::lan(r.clone());
        // 別の機械なので ipc は使わず、zenoh → qmp → http
        assert_eq!(
            c.select_broker(None, &all).unwrap().1,
            "zenoh://192.168.0.12:7448"
        );
        let http = vec!["ipc".to_owned(), "http".to_owned()];
        assert_eq!(
            c.select_broker(None, &http).unwrap().1,
            "http://192.168.0.12:8001"
        );
        assert!(c.select_broker(Some("ipc"), &all).is_err());
        assert_eq!(c.describe(), "robot2/mobile_sim  0.4.0  at robot2-pc");
        let local = Candidate::local(r);
        assert_eq!(
            local.select_broker(None, &all).unwrap().1,
            "ipc://mobile_sim"
        );
        let q = NameQuery::parse("mobile_sim").unwrap();
        assert_eq!(
            crate::naming::ambiguous_message(&q, &[local, c]),
            "mobile_sim is ambiguous: robot2/mobile_sim (this machine), robot2/mobile_sim@robot2-pc. Write the namespace or the machine (ex., robot2/mobile_sim)."
        );
        assert_eq!(
            crate::naming::not_registered_message(
                &q,
                crate::naming::Looked {
                    lan: true,
                    peers: false
                }
            ),
            "mobile_sim is not registered on this machine or the LAN."
        );
    }

    /// 同じプロセスの中で広告して、mDNS で見つけられる (マルチキャストが自分に戻ってくる網が要る)
    #[cfg(feature = "naming-mdns")]
    #[test]
    fn advertise_and_browse() {
        let mut r = reg();
        r.uuid = format!("{}", uuid::Uuid::new_v4());
        r.brokers = vec![RegisteredBroker::new("http", "127.0.0.1:18999")];
        let ad = advertise(&r, &[]).unwrap();
        assert!(
            ad.fullname().ends_with("._juiz._tcp.local."),
            "{}",
            ad.fullname()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let found = loop {
            if let Some(f) = browse(BROWSE_WAIT).into_iter().find(|f| f.uuid == r.uuid) {
                if !f.brokers.is_empty() {
                    break f;
                }
            }
            assert!(std::time::Instant::now() < deadline, "not found on the LAN");
        };
        assert_eq!(found.registered_name(), "robot2/mobile_sim");
        assert_eq!(found.brokers[0].port, Some(18999));
        assert!(
            !found.brokers[0].id.contains("127.0.0.1"),
            "{:?}",
            found.brokers
        );
        drop(ad);
        // goodbye の後は見つからない
        assert!(browse(BROWSE_WAIT).into_iter().all(|f| f.uuid != r.uuid));
    }
}
