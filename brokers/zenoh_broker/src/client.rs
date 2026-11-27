//! Zenoh ブローカーのクライアント (docs/spec/zenoh-broker.md §5.2、broker-plugin-abi.md §5.4)
//!
//! ホストの書く要求のフレームを query で送り、最初の reply の payload を読み口にしてホストに渡す。
//! id の照合・エラー応答の扱い・max_frame_bytes を超える要求を送らないことは、ホストがする。

use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use juiz_sdk::broker_cabi::{BrokerError, ClientHandle, FrameReceiver, FrameSource};
use juiz_sdk::log;
use serde_json::Value;
use zenoh::query::{ConsolidationMode, QueryTarget};
use zenoh::{Session, Wait};

use crate::server::host_port_string;
use crate::shm::FrameEncoder;
use crate::{frame_limits, zenoh_config, CRUD_KEY, DEFAULT_PORT};

/// manifest に timeout_ms が無いときの応答の待ち時間 (§4.2)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Zenoh が待ち時間切れを知らせるまでの余裕。これを過ぎても何も来なければこちらで打ち切る
const TIMEOUT_MARGIN: Duration = Duration::from_secs(2);

/// Zenoh ブローカーのクライアント。同じ値で同時にいくつ request を呼んでもよい
pub struct Client {
    session: Session,
    timeout: Duration,
    encoder: FrameEncoder,
}

/// `<host>:<port>` (IPv6 は `[<addr>]:<port>`、ポートを省くと 7448) を繋ぎ先のアドレスにする。
/// ホスト名は名前解決し、IPv4 を優先する (localhost が ::1 にも解決される環境があるため、qmp と同じ)
fn resolve(name: &str) -> Result<SocketAddr, BrokerError> {
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
        .map_err(|e| BrokerError::unavailable(format!("can not resolve zenoh broker address '{name}'. {e}")))?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .cloned()
        .ok_or_else(|| BrokerError::unavailable(format!("can not resolve zenoh broker address '{name}'")))
}

impl Client {
    pub(crate) fn connect(manifest: &Value) -> Result<Self, BrokerError> {
        log::trace!("zenoh_broker: connect('{manifest}')");
        let name = manifest
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BrokerError::error("the manifest has no 'name'"))?;
        let addr = resolve(name)?;
        let locator = format!("tcp/{addr}");
        let limits = frame_limits(manifest);
        let timeout = match manifest.get("timeout_ms") {
            None | Some(Value::Null) => DEFAULT_TIMEOUT,
            Some(ms) => match ms.as_i64() {
                Some(ms) if ms > 0 => Duration::from_millis(ms as u64),
                _ => return Err(BrokerError::error(format!("timeout_ms must be positive ({ms})"))),
            },
        };
        let config = zenoh_config(manifest, None, Some(&locator), &limits)?;
        // 同期の wait() は tokio のランタイムの中 (current_thread) では使えないので、別スレッドで開く。
        // 共有メモリの pool もここで用意しておく (§11)
        let m = manifest.clone();
        let (session, encoder) = std::thread::spawn(move || {
            let session = zenoh::open(config).wait().map_err(|e| e.to_string())?;
            let encoder = FrameEncoder::for_session(&session, &m);
            Ok::<_, String>((session, encoder))
        })
        .join()
        .map_err(|_| BrokerError::error("opening a zenoh session panicked"))?
        .map_err(|e| BrokerError::unavailable(format!("can not connect ({locator}). {e}")))?;
        log::debug!(
            "zenoh_broker: connected to {locator} (shared memory {})",
            if encoder.uses_shm() { "on" } else { "off" }
        );
        Ok(Client { session, timeout, encoder })
    }

    fn no_response(&self) -> BrokerError {
        BrokerError::timeout(format!("no response within {} ms", self.timeout.as_millis()))
    }
}

impl ClientHandle for Client {
    fn request(&self, request: FrameSource<'_>, response: FrameReceiver<'_>) -> Result<(), BrokerError> {
        // 共有メモリが使えれば、ホストにそこへ直接書かせる (§11)
        let bytes = self.encoder.encode(request)?;
        let replies = self
            .session
            .get(CRUD_KEY)
            .payload(bytes)
            .target(QueryTarget::BestMatching)
            .consolidation(ConsolidationMode::None)
            .timeout(self.timeout)
            .wait()
            .map_err(|e| BrokerError::unavailable(format!("sending a request failed. {e}")))?;
        let reply = match replies.recv_timeout(self.timeout + TIMEOUT_MARGIN) {
            Ok(Some(reply)) => reply,
            Ok(None) => return Err(self.no_response()),
            Err(_) => return Err(BrokerError::unavailable("no juiz zenoh broker answered")),
        };
        let sample = match reply.result() {
            Ok(sample) => sample,
            Err(e) => {
                let message = e.payload().try_to_string().map(|s| s.into_owned()).unwrap_or_default();
                // Zenoh は待ち時間切れを ReplyError ("Timeout") で知らせる
                if message.contains("Timeout") {
                    return Err(self.no_response());
                }
                return Err(BrokerError::error(format!("zenoh reply error. {message}")));
            }
        };
        let payload = sample.payload();
        // ZBytes の断片から、本体をホストの最終的な領域へ直接読ませる (broker-plugin-abi.md §5.1)
        response.receive(&mut payload.reader(), Some(payload.len() as u64))
    }
}

impl Drop for Client {
    /// セッションを閉じる。同期の wait() は tokio のランタイムの中では使えないので、別スレッドで閉じる
    fn drop(&mut self) {
        let session = self.session.clone();
        let _ = std::thread::spawn(move || {
            let _ = session.close().wait();
        })
        .join();
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
