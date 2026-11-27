//! Zenoh ブローカーのサーバー (docs/spec/zenoh-broker.md §5.3、broker-plugin-abi.md §5.3)
//!
//! queryable で受けた query の payload を読み口にしてホストに渡し (ホストが要求を処理する)、
//! ホストの書く応答のフレームを reply で返す。

use std::net::{IpAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use juiz_sdk::broker_cabi::{BrokerError, ServerHandle, ServerHost};
use juiz_sdk::log;
use serde_json::Value;
use zenoh::bytes::ZBytes;
use zenoh::query::Query;
use zenoh::{Session, Wait};

use crate::shm::FrameEncoder;
use crate::{frame_limits, zenoh_config, CRUD_KEY, DEFAULT_PORT};

/// 受け取り用のスレッドが、止める知らせを確かめる間隔
const RECV_POLL: Duration = Duration::from_millis(200);

/// 待ち受け中のサーバー
pub struct Server {
    stop: Arc<AtomicBool>,
    recv: Option<JoinHandle<()>>,
    /// 処理中の query のスレッド
    queries: Arc<Mutex<Vec<JoinHandle<()>>>>,
    session: Session,
}

/// listen するホストとポート。ポート 0 は空いているポートを選ぶ
fn host_and_port(manifest: &Value) -> Result<(String, u16), BrokerError> {
    let host = manifest.get("host").and_then(|v| v.as_str()).unwrap_or("0.0.0.0").to_owned();
    let port = match manifest.get("port") {
        None | Some(Value::Null) => DEFAULT_PORT,
        Some(p) => p
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .ok_or_else(|| BrokerError::error(format!("invalid zenoh broker port {p}")))?,
    };
    if port != 0 {
        return Ok((host, port));
    }
    // Zenoh から割り当てられたポートを聞く安定した API が無いので、先に空いているポートを探す
    let listener = TcpListener::bind((host.as_str(), 0))
        .map_err(|e| BrokerError::error(format!("can not find a free port on {host}. {e}")))?;
    let port = listener.local_addr()?.port();
    Ok((host, port))
}

/// IPv6 のアドレスは [] で囲む (§3)
pub(crate) fn host_port_string(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}

/// 待ち受けを始める。実際の住所 (0.0.0.0 は 127.0.0.1) と一緒に返す
pub(crate) fn start(manifest: Value, host: ServerHost) -> Result<(Server, String), BrokerError> {
    log::info!("zenoh_broker: start(manifest='{manifest}')");
    let limits = frame_limits(&manifest);
    let (bind_host, port) = host_and_port(&manifest)?;
    let locator = format!("tcp/{}", host_port_string(&bind_host, port));
    let config = zenoh_config(&manifest, Some(&locator), None, &limits)?;
    let session = zenoh::open(config)
        .wait()
        .map_err(|e| BrokerError::error(format!("can not listen on {locator}. {e}")))?;
    let queryable = session
        .declare_queryable(CRUD_KEY)
        .wait()
        .map_err(|e| BrokerError::error(format!("declaring the queryable '{CRUD_KEY}' failed. {e}")))?;
    // 共有メモリの pool を用意する (§11)
    let encoder = Arc::new(FrameEncoder::for_session(&session, &manifest));
    log::debug!("zenoh_broker: shared memory {}", if encoder.uses_shm() { "on" } else { "off" });

    let stop = Arc::new(AtomicBool::new(false));
    let queries: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::default();
    // 受け取りは普通のスレッドで。Zenoh のコールバックの中では処理しない (§5.3)。
    // ホストの handle は Process の実行が終わるまで戻らないので、query ごとにスレッドを作る
    let recv = {
        let stop = stop.clone();
        let queries = queries.clone();
        std::thread::Builder::new()
            .name("zenoh_broker_recv".to_owned())
            .spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match queryable.recv_timeout(RECV_POLL) {
                        Ok(Some(query)) => {
                            let encoder = encoder.clone();
                            let spawned = std::thread::Builder::new()
                                .name("zenoh_broker_query".to_owned())
                                .spawn(move || serve_query(query, host, &encoder));
                            match spawned {
                                Ok(t) => {
                                    let mut q = queries.lock().unwrap_or_else(|e| e.into_inner());
                                    q.retain(|t| !t.is_finished());
                                    q.push(t);
                                }
                                Err(e) => log::error!("zenoh_broker: can not start a thread for a query. {e}"),
                            }
                        }
                        Ok(None) => {}
                        Err(_) => break,
                    }
                }
                // queryable はここで取り消される
                log::debug!("zenoh_broker: stopped receiving");
            })?
    };
    let name_host = match bind_host.parse::<IpAddr>() {
        Ok(ip) if ip.is_unspecified() => "127.0.0.1".to_owned(),
        _ => bind_host.clone(),
    };
    log::info!("zenoh_broker: listening on {locator}");
    Ok((
        Server { stop, recv: Some(recv), queries, session },
        host_port_string(&name_host, port),
    ))
}

/// query を 1 つホストに処理させ、応答を reply で返す。処理用のスレッドで呼ぶ
fn serve_query(query: Query, host: ServerHost, encoder: &FrameEncoder) {
    // payload の無い query は長さ 0 の要求にする。ホストは読めないのでエラー応答 (id 0) を返す
    let empty = ZBytes::default();
    let payload = query.payload().unwrap_or(&empty);
    let len = payload.len() as u64;
    // ZBytes の断片から、本体をホストの最終的な領域へ直接読ませる (broker-plugin-abi.md §5.1)
    let mut reader = payload.reader();
    let r = host.handle("", &mut reader, Some(len), |frame| {
        // 応答のフレームは共有メモリが使えればそこへ直接書かせる (§11)
        let bytes = encoder.encode(frame)?;
        query
            .reply(CRUD_KEY, bytes)
            .wait()
            .map_err(|e| BrokerError::error(format!("replying to a query failed. {e}")))
    });
    if let Err(e) = r {
        log::error!("zenoh_broker: {e}");
    }
}

impl ServerHandle for Server {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.recv.take() {
            let _ = t.join();
        }
        // 処理中の要求が終わるまで待つ (broker-plugin-abi.md §5.3)
        let queries = std::mem::take(&mut *self.queries.lock().unwrap_or_else(|e| e.into_inner()));
        for t in queries {
            let _ = t.join();
        }
        // 同期の wait() は tokio のランタイムの中では使えないことがあるので、別スレッドで閉じる
        let session = self.session.clone();
        let _ = std::thread::spawn(move || {
            let _ = session.close().wait();
        })
        .join();
        log::debug!("zenoh_broker: stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ipv6_is_bracketed() {
        assert_eq!(host_port_string("::1", 7448), "[::1]:7448");
        assert_eq!(host_port_string("127.0.0.1", 7448), "127.0.0.1:7448");
        assert_eq!(host_port_string("localhost", 7448), "localhost:7448");
    }

    #[test]
    fn default_host_and_port() {
        assert_eq!(host_and_port(&json!({})).unwrap(), ("0.0.0.0".to_owned(), 7448));
        let (_, port) = host_and_port(&json!({"host": "127.0.0.1", "port": 0})).unwrap();
        assert_ne!(port, 0);
        assert!(host_and_port(&json!({"port": 70000})).is_err());
    }
}
