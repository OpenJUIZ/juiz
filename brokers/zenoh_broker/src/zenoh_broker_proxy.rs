//! Zenoh ブローカープロキシ (クライアント、docs/spec/zenoh-broker.md §5.2)

use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use juiz_core::frame::{read_frame, FrameLimits, FrameReadError, OutgoingFrame};
use juiz_core::{
    anyhow::{self, anyhow},
    prelude::*,
    CRUDBrokerProxy, CRUDBrokerProxyHolder,
};
use zenoh::bytes::ZBytes;
use zenoh::query::{ConsolidationMode, QueryTarget};
use zenoh::{Session, Wait};

use crate::zenoh_broker::host_port_string;
use crate::{frame_limits, zenoh_config, CRUD_KEY, DEFAULT_PORT, TYPE_NAME};

/// manifest に timeout_ms が無いときの応答の待ち時間 (§4.2)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Zenoh が待ち時間切れを知らせるまでの余裕。これを過ぎても何も来なければこちらで打ち切る
const TIMEOUT_MARGIN: Duration = Duration::from_secs(2);

pub(crate) fn create_broker_proxy_function(
    _core_broker: &CoreWorker,
    manifest: Value,
) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
    log::trace!("create_broker_proxy_function({manifest:}) called");
    let name = obj_get_str(&manifest, "name")?;
    Ok(CRUDBrokerProxyHolder::new(
        "ZenohBrokerProxy",
        TYPE_NAME,
        name,
        Box::new(ZenohBrokerProxy::new(&manifest)?),
    )?)
}

pub struct ZenohBrokerProxy {
    name: String,
    session: Session,
    timeout: Duration,
    limits: FrameLimits,
    next_id: AtomicU64,
}

/// `<host>:<port>` (IPv6 は `[<addr>]:<port>`、ポートを省くと 7448) を繋ぎ先のアドレスにする。
/// ホスト名は名前解決し、IPv4 を優先する (localhost が ::1 にも解決される環境があるため、qmp と同じ)
fn resolve(name: &str) -> anyhow::Result<SocketAddr> {
    // "host:port" (host は IPv6 の生のアドレスでない) ならポートが付いている
    let host_has_port = name
        .rsplit_once(':')
        .is_some_and(|(h, p)| !h.is_empty() && !h.contains(':') && p.parse::<u16>().is_ok());
    let with_port = if host_has_port || name.parse::<SocketAddr>().is_ok() {
        name.to_owned()
    } else {
        let host = name.trim_start_matches('[').trim_end_matches(']');
        host_port_string(host, DEFAULT_PORT)
    };
    let addrs: Vec<SocketAddr> = with_port
        .to_socket_addrs()
        .map_err(|e| anyhow!("can not resolve zenoh broker address '{name}'. {e}"))?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .cloned()
        .ok_or_else(|| anyhow!("can not resolve zenoh broker address '{name}'"))
}

impl ZenohBrokerProxy {
    pub fn new(manifest: &Value) -> JuizResult<Self> {
        log::trace!("ZenohBrokerProxy::new('{manifest:}') called");
        let name = obj_get_str(manifest, "name")?.to_owned();
        let addr = resolve(&name)?;
        let locator = format!("tcp/{addr}");
        let limits = frame_limits(manifest);
        let timeout = match obj_get_i64(manifest, "timeout_ms") {
            Ok(ms) if ms > 0 => Duration::from_millis(ms as u64),
            Ok(ms) => return Err(anyhow!("zenoh://{name}: timeout_ms must be positive ({ms})")),
            Err(_) => DEFAULT_TIMEOUT,
        };
        let config = zenoh_config(manifest, None, Some(&locator), &limits)?;
        // 同期の wait() は tokio のランタイムの中 (current_thread) では使えないので、別スレッドで開く
        let session = std::thread::spawn(move || zenoh::open(config).wait().map_err(|e| e.to_string()))
            .join()
            .map_err(|_| anyhow!("zenoh://{name}: opening a zenoh session panicked"))?
            .map_err(|e| anyhow!("{TYPE_NAME}://{name}: can not connect ({locator}). {e}"))?;
        log::debug!("ZenohBrokerProxy: connected to {locator}");
        Ok(ZenohBrokerProxy {
            name,
            session,
            timeout,
            limits,
            next_id: AtomicU64::new(1),
        })
    }

    fn label(&self) -> String {
        format!("{TYPE_NAME}://{}", self.name)
    }

    /// 要求のフレームを query で送り、最初の reply を応答として読む
    fn request(&self, frame: &OutgoingFrame) -> JuizResult<CapsulePtr> {
        let id = frame.header().id;
        let bytes = frame.to_bytes()?;
        // 上限を超えるメッセージは Zenoh が黙って捨てるので、送る前に確かめる (§5.2)
        if bytes.len() as u64 > self.limits.max_frame_bytes {
            return Err(anyhow!(
                "{}: the request frame is too large ({} bytes > max_frame_bytes {} bytes)",
                self.label(),
                bytes.len(),
                self.limits.max_frame_bytes
            ));
        }
        let replies = self
            .session
            .get(CRUD_KEY)
            .payload(ZBytes::from(bytes))
            .target(QueryTarget::BestMatching)
            .consolidation(ConsolidationMode::None)
            .timeout(self.timeout)
            .wait()
            .map_err(|e| anyhow!("{}: sending a request failed. {e}", self.label()))?;
        let reply = match replies.recv_timeout(self.timeout + TIMEOUT_MARGIN) {
            Ok(Some(reply)) => reply,
            Ok(None) => return Err(self.no_response()),
            Err(_) => {
                return Err(anyhow!(
                    "{}: no juiz zenoh broker answered",
                    self.label()
                ))
            }
        };
        let sample = match reply.result() {
            Ok(sample) => sample,
            Err(e) => {
                let message = e.payload().try_to_string().map(|s| s.into_owned()).unwrap_or_default();
                // Zenoh は待ち時間切れを ReplyError ("Timeout") で知らせる
                if message.contains("Timeout") {
                    return Err(self.no_response());
                }
                return Err(anyhow!("{}: zenoh reply error. {message}", self.label()));
            }
        };
        let mut reader = sample.payload().reader();
        let response = read_frame(&mut reader, &self.limits).map_err(|e| {
            let e: anyhow::Error = match e {
                FrameReadError::Closed => anyhow!("the reply is empty"),
                e => e.into(),
            };
            e.context(format!("receiving a response from {} failed", self.label()))
        })?;
        if response.id() != id {
            return Err(anyhow!(
                "{}: the response id ({}) does not match the request id ({id})",
                self.label(),
                response.id()
            ));
        }
        response.into_capsule_ptr()
    }

    fn no_response(&self) -> anyhow::Error {
        anyhow!(
            "{}: no response within {} ms",
            self.label(),
            self.timeout.as_millis()
        )
    }

    fn crud_request(
        &self,
        method_name: &str,
        class_name: &str,
        function_name: &str,
        mut map: CapsuleMap,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("ZenohBrokerProxy::{method_name}({class_name}, {function_name}) called");
        for (k, v) in param.iter() {
            map.set_param(k, v);
        }
        map.set_param("method_name", method_name);
        map.set_param("class_name", class_name);
        map.set_param("function_name", function_name);
        // どの住所でアクセスしたかをサーバーに伝える (HTTP の Host ヘッダにあたる)
        map.set_param("accessed_broker_name", self.name.as_str());
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.request(&OutgoingFrame::request(id, &map)?)
    }
}

impl Drop for ZenohBrokerProxy {
    /// セッションを閉じる。同期の wait() は tokio のランタイムの中では使えないので、別スレッドで閉じる
    fn drop(&mut self) {
        let session = self.session.clone();
        let _ = std::thread::spawn(move || {
            let _ = session.close().wait();
        })
        .join();
    }
}

/// create の payload (オブジェクトの Value) を CapsuleMap にする (qmp と同じ)
fn value_to_map(payload: Value) -> JuizResult<CapsuleMap> {
    match payload {
        Value::Null => Ok(CapsuleMap::new()),
        v @ Value::Object(_) => CapsuleMap::try_from(v),
        v => Err(anyhow!(JuizError::InvalidValueError {
            message: format!("create payload must be an object ({v})")
        })),
    }
}

impl CRUDBrokerProxy for ZenohBrokerProxy {
    fn create(
        &self,
        class_name: &str,
        function_name: &str,
        payload: Value,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        self.crud_request("create", class_name, function_name, value_to_map(payload)?, param)
    }

    fn delete(
        &self,
        class_name: &str,
        function_name: &str,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        self.crud_request("delete", class_name, function_name, CapsuleMap::new(), param)
    }

    fn read(
        &self,
        class_name: &str,
        function_name: &str,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        self.crud_request("read", class_name, function_name, CapsuleMap::new(), param)
    }

    fn update(
        &self,
        class_name: &str,
        function_name: &str,
        payload: CapsuleMap,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        self.crud_request("update", class_name, function_name, payload, param)
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn resolve_names() {
        assert_eq!(resolve("127.0.0.1:5000").unwrap().to_string(), "127.0.0.1:5000");
        assert_eq!(resolve("127.0.0.1").unwrap().to_string(), "127.0.0.1:7448");
        assert_eq!(resolve("localhost:5000").unwrap().to_string(), "127.0.0.1:5000");
        assert_eq!(resolve("[::1]:5000").unwrap().to_string(), "[::1]:5000");
        assert_eq!(resolve("[::1]").unwrap().to_string(), "[::1]:7448");
    }
}
