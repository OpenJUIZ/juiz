//! ブローカーの待ち受けポートが使用中のときの扱い (docs/spec/app-package.md §16、issue #392)
//!
//! - `port_fallback`: `next`(既定)なら +1 ずつ元の番号 + 100 まで取り直す。`error` なら起動を止める
//! - `adopt_master`: `true`(既定)なら使用中の相手が juiz のとき、その System を `mastersystem` とする
//!
//! HTTP ブローカーと qmp ブローカーが [`bind_with_policy`] を使う。

use std::future::Future;
use std::io::ErrorKind;
use std::sync::{Arc, Mutex};

use crate::brokers::CRUDBroker;
use crate::prelude::*;
use juiz_sdk::anyhow::anyhow;

/// `port_fallback: next` で取り直す範囲 (元の番号から + この数まで)
pub const PORT_FALLBACK_RANGE: i64 = 100;

/// 相手が juiz かを確かめるときの待ち時間 (§16.3)
pub const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// ポートが使用中のときの扱いを指定できるブローカーの種類
pub const PORT_POLICY_BROKER_TYPES: [&str; 2] = ["http", "qmp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortFallback {
    Next,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortPolicy {
    pub fallback: PortFallback,
    pub adopt_master: bool,
}

impl Default for PortPolicy {
    fn default() -> Self {
        PortPolicy {
            fallback: PortFallback::Next,
            adopt_master: true,
        }
    }
}

impl PortPolicy {
    /// ブローカーのマニフェストの `port_fallback`・`adopt_master` を読む。書いていなければ既定。知らない値はエラー
    pub fn from_manifest(manifest: &Value) -> JuizResult<PortPolicy> {
        let fallback = match manifest.get("port_fallback") {
            None | Some(Value::Null) => PortFallback::Next,
            Some(Value::String(s)) if s == "next" => PortFallback::Next,
            Some(Value::String(s)) if s == "error" => PortFallback::Error,
            Some(Value::String(s)) => {
                return Err(anyhow!(
                    "port_fallback must be 'next' or 'error', but '{s}'"
                ))
            }
            Some(v) => {
                return Err(anyhow!(
                    "port_fallback must be 'next' or 'error', but '{v}'"
                ))
            }
        };
        let adopt_master = match manifest.get("adopt_master") {
            None | Some(Value::Null) => true,
            Some(Value::Bool(b)) => *b,
            Some(v) => return Err(anyhow!("adopt_master must be true or false, but '{v}'")),
        };
        Ok(PortPolicy {
            fallback,
            adopt_master,
        })
    }
}

/// conf の中で、ポートの扱いを持つブローカーの設定 (`option.http_broker` と `brokers` の http・qmp) を順に渡す。
/// `create_http_broker_option` が真なら `option.http_broker` が無くても作る (`juiz -d` は conf に書かなくても
/// HTTP ブローカーを立てるので)。
fn for_each_broker_conf(
    conf: &mut Value,
    create_http_broker_option: bool,
    mut f: impl FnMut(&mut serde_json::Map<String, Value>),
) {
    let Some(obj) = conf.as_object_mut() else {
        return;
    };
    if create_http_broker_option {
        let option = obj.entry("option").or_insert_with(|| jvalue!({}));
        if option.is_null() {
            *option = jvalue!({});
        }
        if let Some(option) = option.as_object_mut() {
            let http = option.entry("http_broker").or_insert_with(|| jvalue!({}));
            if http.is_null() {
                *http = jvalue!({});
            }
        }
    }
    if let Some(http) = obj
        .get_mut("option")
        .and_then(|o| o.get_mut("http_broker"))
        .and_then(|h| h.as_object_mut())
    {
        f(http);
    }
    if let Some(brokers) = obj.get_mut("brokers").and_then(|b| b.as_array_mut()) {
        for b in brokers.iter_mut() {
            let Some(b) = b.as_object_mut() else { continue };
            let is_target = b
                .get("type_name")
                .and_then(|t| t.as_str())
                .map(|t| PORT_POLICY_BROKER_TYPES.contains(&t))
                .unwrap_or(false);
            if is_target {
                f(b);
            }
        }
    }
}

/// コマンドの引数 `--no-port-fallback`・`--no-adopt-master` を conf に反映する (§16.2)。conf の値より優先する
pub fn override_port_policy(conf: &mut Value, no_port_fallback: bool, no_adopt_master: bool) {
    if !no_port_fallback && !no_adopt_master {
        return;
    }
    for_each_broker_conf(conf, true, |b| {
        if no_port_fallback {
            b.insert("port_fallback".to_owned(), "error".into());
        }
        if no_adopt_master {
            b.insert("adopt_master".to_owned(), false.into());
        }
    });
}

/// conf に `adopt_master` を書いていないブローカーを `adopt_master: <value>` にする (§16.2 の juiz run の既定)
pub fn default_adopt_master(conf: &mut Value, value: bool) {
    // option.http_broker を書いていなくても HTTP ブローカーは立つので、そこにも入れる
    for_each_broker_conf(conf, true, |b| {
        if b.get("adopt_master").map(|v| v.is_null()).unwrap_or(true) {
            b.insert("adopt_master".to_owned(), value.into());
        }
    });
}

/// 使用中のポートを持つ相手が juiz かどうか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occupant {
    Juiz,
    Other,
}

impl Occupant {
    fn describe(self) -> &'static str {
        match self {
            Occupant::Juiz => "by a juiz",
            Occupant::Other => "by another program",
        }
    }
}

/// 相手の `system/profile` の値が juiz の形か (§16.3)。BrokerProxy が `{"__value__": …}` で包んだものも受ける
pub fn looks_like_juiz_profile(v: &Value) -> bool {
    let v = v.get("__value__").unwrap_or(v);
    v.get("system_store")
        .and_then(|s| s.get("uuid"))
        .and_then(|u| u.as_str())
        .map(|u| !u.is_empty())
        .unwrap_or(false)
}

/// HTTP の相手の `system/profile` を読む (§16.3)。juiz の形でなければ None。
///
/// `GET http://<host>:<port>/api/system/profile` を [`PROBE_TIMEOUT`] で呼び、`system_store.uuid` を持つ JSON が
/// 返れば、その値 (`__value__` の包みは外す) を返す。0.2.10 までの juiz には `system/profile` が無いので、
/// 404 なら `system/profile_full` を呼び直す (#508)。`juiz run` の事前確認 (§15.2) は `package` も読む。
pub async fn fetch_http_profile(host: &str, port: i64) -> Option<Value> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .no_proxy()
        .build()
        .ok()?;
    for function in ["profile", "profile_full"] {
        let url = format!("http://{host}:{port}/api/system/{function}");
        let response = client.get(&url).send().await.ok()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            continue;
        }
        if !response.status().is_success() {
            return None;
        }
        let v = response.json::<Value>().await.ok()?;
        return juiz_profile(v);
    }
    None
}

/// `system/profile` の値が juiz の形なら、`__value__` の包みを外して返す
pub fn juiz_profile(v: Value) -> Option<Value> {
    if !looks_like_juiz_profile(&v) {
        return None;
    }
    Some(match v {
        Value::Object(mut m) if m.contains_key("__value__") => m.remove("__value__").unwrap(),
        v => v,
    })
}

/// 使用中のポートの juiz を親とするときの conf の `mastersystem` の値 (§16.1、§15.2)
pub fn master_info(broker_type: &str, port: i64) -> Value {
    jvalue!({
        "host": "127.0.0.1",
        "port": port,
        "broker_type": broker_type,
        "broker_name": format!("127.0.0.1:{port}"),
    })
}

/// 待ち受けるホストから、同じ機械で相手に問い合わせるときのホストを決める
pub fn probe_host(host: &str) -> &str {
    match host {
        "0.0.0.0" | "::" | "[::]" | "" => "127.0.0.1",
        h => h,
    }
}

/// [`bind_with_policy`] で取れたもの
pub struct Bound<T> {
    pub listener: T,
    pub port: i64,
}

/// ポートの扱い (§16) に従って bind する。
///
/// - `label`: 文に出すブローカーの名前 (`HTTP broker`、`qmp broker`)
/// - `bind`: その番号で待ち受ける。使用中なら `ErrorKind::AddrInUse` の io::Error を返す
/// - `probe`: 使用中の番号の相手が juiz かを確かめる (§16.3)
///
/// 番号を取り直したとき・相手を親とみなしたときは標準エラーに出す (§16.4)。エラーの文は利用者にそのまま見せる。
pub async fn bind_with_policy<T, B, BFut, P, PFut>(
    label: &str,
    broker_type: &str,
    host: &str,
    start_port: i64,
    policy: PortPolicy,
    crud_broker: &Arc<Mutex<CRUDBroker>>,
    mut bind: B,
    probe: P,
) -> Result<Bound<T>, String>
where
    B: FnMut(String) -> BFut,
    BFut: Future<Output = std::io::Result<T>>,
    P: Fn(i64) -> PFut,
    PFut: Future<Output = Occupant>,
{
    let last_port = start_port + PORT_FALLBACK_RANGE;
    let mut port = start_port;
    // 親とみなすのは最初に見つけた juiz だけ
    let mut adopted = false;
    loop {
        let address = format!("{host}:{port}");
        log::debug!("{label}: binding {address}");
        match bind(address).await {
            Ok(listener) => {
                if port != start_port {
                    eprintln!("warning: port {start_port} is in use; the {label} uses {port}");
                }
                return Ok(Bound { listener, port });
            }
            Err(e) if e.kind() == ErrorKind::AddrInUse => match policy.fallback {
                PortFallback::Error => {
                    let occupant = probe(port).await;
                    return Err(format!(
                        "port {port} for the {label} is in use ({}). Use another port, or remove port_fallback: error / --no-port-fallback.",
                        occupant.describe()
                    ));
                }
                PortFallback::Next => {
                    if policy.adopt_master && !adopted {
                        if probe(port).await == Occupant::Juiz {
                            adopted = true;
                            adopt(label, broker_type, port, crud_broker);
                        } else {
                            log::info!(
                                "{label}: port {port} is used by a program that is not a juiz"
                            );
                        }
                    }
                    port += 1;
                    if port > last_port {
                        return Err(format!(
                            "no free port for the {label} in {start_port}..{last_port}"
                        ));
                    }
                }
            },
            Err(e) => {
                return Err(format!("cannot start the {label} on {host}:{port}: {e}"));
            }
        }
    }
}

/// 使用中のポートの juiz を mastersystem として予約する。conf に mastersystem があれば予約しない
fn adopt(label: &str, broker_type: &str, port: i64, crud_broker: &Arc<Mutex<CRUDBroker>>) {
    let reserved = juiz_lock(crud_broker)
        .and_then(|mut b| b.reserve_master_broker(master_info(broker_type, port)));
    match reserved {
        Ok(true) => eprintln!("joined the juiz at 127.0.0.1:{port} as its subsystem"),
        Ok(false) => log::info!(
            "{label}: the juiz at 127.0.0.1:{port} is not adopted because the conf has 'mastersystem'"
        ),
        Err(e) => log::error!("{label}: reserving the juiz at 127.0.0.1:{port} as the mastersystem failed. {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_defaults_and_values() {
        assert_eq!(
            PortPolicy::from_manifest(&jvalue!({})).unwrap(),
            PortPolicy::default()
        );
        let p =
            PortPolicy::from_manifest(&jvalue!({"port_fallback": "error", "adopt_master": false}))
                .unwrap();
        assert_eq!(p.fallback, PortFallback::Error);
        assert!(!p.adopt_master);
        let e = PortPolicy::from_manifest(&jvalue!({"port_fallback": "skip"}))
            .unwrap_err()
            .to_string();
        assert_eq!(e, "port_fallback must be 'next' or 'error', but 'skip'");
        assert!(PortPolicy::from_manifest(&jvalue!({"adopt_master": "yes"})).is_err());
    }

    #[test]
    fn override_sets_every_broker() {
        let mut conf = jvalue!({
            "brokers": [
                {"type_name": "qmp", "port": 3000, "port_fallback": "next"},
                {"type_name": "ipc", "name": "x"},
                {"type_name": "http", "port": 8100},
            ]
        });
        override_port_policy(&mut conf, true, true);
        assert_eq!(conf["option"]["http_broker"]["port_fallback"], "error");
        assert_eq!(conf["option"]["http_broker"]["adopt_master"], false);
        assert_eq!(conf["brokers"][0]["port_fallback"], "error");
        assert_eq!(conf["brokers"][0]["adopt_master"], false);
        assert!(conf["brokers"][1].get("port_fallback").is_none());
        assert_eq!(conf["brokers"][2]["port_fallback"], "error");

        // 引数が無ければ何も変えない
        let mut conf = jvalue!({"name": "a"});
        override_port_policy(&mut conf, false, false);
        assert_eq!(conf, jvalue!({"name": "a"}));
    }

    #[test]
    fn default_adopt_master_keeps_conf_value() {
        let mut conf = jvalue!({
            "option": {"http_broker": {"port": 8100, "adopt_master": true}},
            "brokers": [{"type_name": "qmp", "port": 3000}],
        });
        default_adopt_master(&mut conf, false);
        assert_eq!(conf["option"]["http_broker"]["adopt_master"], true);
        assert_eq!(conf["brokers"][0]["adopt_master"], false);
    }

    #[test]
    fn juiz_profile_shape() {
        assert!(looks_like_juiz_profile(
            &jvalue!({"system_store": {"uuid": "abc"}})
        ));
        assert!(looks_like_juiz_profile(
            &jvalue!({"__value__": {"system_store": {"uuid": "abc"}}})
        ));
        assert!(!looks_like_juiz_profile(&jvalue!({"system_store": {}})));
        assert!(!looks_like_juiz_profile(&jvalue!("<html>")));
        assert_eq!(
            juiz_profile(jvalue!({"__value__": {"system_store": {"uuid": "abc"}, "package": {"name": "a"}}})),
            Some(jvalue!({"system_store": {"uuid": "abc"}, "package": {"name": "a"}}))
        );
        assert_eq!(juiz_profile(jvalue!({"system_store": {}})), None);
    }
}
