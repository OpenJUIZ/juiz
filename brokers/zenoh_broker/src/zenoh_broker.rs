//! Zenoh ブローカー (サーバー、docs/spec/zenoh-broker.md §5.3)

use std::net::{IpAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use juiz_core::frame::{read_frame, FrameLimits, FrameReadError, OutgoingFrame};
use juiz_core::localize_identifier;
use juiz_core::{
    anyhow::{self, anyhow},
    prelude::*,
    CRUDBroker,
};
use zenoh::bytes::ZBytes;
use zenoh::query::Query;
use zenoh::Wait;

use crate::shm::{frame_len, FrameEncoder};
use crate::{frame_limits, zenoh_config, zerr, CRUD_KEY, DEFAULT_PORT, TYPE_NAME};

/// 受け取り用のスレッドが、止める知らせを確かめる間隔
const RECV_POLL: Duration = Duration::from_millis(200);

/// 受け取ったリクエストの param に、どの住所でアクセスされたかを書き込み、
/// 自分自身を指す zenoh:// の識別子を core://core/... に書き換える (qmp と同じ、§3)
fn localize_request(cm: &mut CapsuleMap, own_name: &str) {
    let accessed_name = cm
        .get_param("accessed_broker_name")
        .cloned()
        .unwrap_or_else(|| own_name.to_owned());
    if let Some(id) = cm.get_param("identifier").cloned() {
        let mut id = localize_identifier(id.as_str(), TYPE_NAME, Some(accessed_name.as_str()));
        id = localize_identifier(id.as_str(), TYPE_NAME, Some(own_name));
        cm.set_param("identifier", id.as_str());
    }
    cm.set_param("accessed_broker_id", format!("{TYPE_NAME}://{accessed_name}").as_str());
    cm.set_param("accessed_broker_type", TYPE_NAME);
    cm.set_param("accessed_broker_name", accessed_name.as_str());
}

fn handle_request(crud_broker: &Arc<Mutex<CRUDBroker>>, mut cm: CapsuleMap) -> JuizResult<CapsulePtr> {
    // ロックは名前と dispatcher を取り出す間だけ握り、Process の実行中は外しておく (issue #356)
    let (name, dispatcher) = {
        let crud = juiz_lock(crud_broker)?;
        (crud.name(), crud.dispatcher())
    };
    localize_request(&mut cm, name.as_str());
    // Zenoh の query からは相手の IP アドレスが取れない (§3)
    dispatcher.on_capsule_map_request(cm, None)
}

/// 要求のフレームを読んで処理し、応答のフレームを返す。失敗したらエラー応答のフレームにする
fn respond(payload: Option<&ZBytes>, crud_broker: &Arc<Mutex<CRUDBroker>>, limits: &FrameLimits) -> OutgoingFrame {
    let Some(payload) = payload else {
        log::error!("zenoh_broker: a query without payload");
        return OutgoingFrame::error(0, "", "", "the query has no payload".to_owned());
    };
    // ZBytes の断片から、本体を最終的な領域へ直接読む (§5.2)
    let mut reader = payload.reader();
    match read_frame(&mut reader, limits) {
        Ok(frame) => {
            let id = frame.id();
            let param = |name: &str| {
                frame
                    .header
                    .param
                    .as_ref()
                    .and_then(|p| p.get(name).cloned())
                    .unwrap_or_default()
            };
            let (class_name, function_name) = (param("class_name"), param("function_name"));
            let result = frame
                .into_capsule_map()
                .and_then(|cm| handle_request(crud_broker, cm))
                .and_then(|capsule| OutgoingFrame::response(id, &capsule));
            match result {
                Ok(response) => response,
                Err(e) => {
                    log::error!("zenoh_broker: {class_name}/{function_name} failed. {e:#}");
                    OutgoingFrame::error(id, &class_name, &function_name, format!("{e:#}"))
                }
            }
        }
        Err(FrameReadError::Fatal { id, error }) | Err(FrameReadError::Invalid { id, error }) => {
            log::error!("zenoh_broker: {error:#}");
            OutgoingFrame::error(id, "", "", format!("{error:#}"))
        }
        Err(e) => {
            let e: anyhow::Error = e.into();
            log::error!("zenoh_broker: can not read the request frame. {e:#}");
            OutgoingFrame::error(0, "", "", format!("can not read the request frame. {e:#}"))
        }
    }
}

/// query を 1 つ処理して reply を返す。処理用のスレッドで呼ぶ
fn serve_query(query: Query, crud_broker: Arc<Mutex<CRUDBroker>>, limits: FrameLimits, encoder: Arc<FrameEncoder>) {
    let response = respond(query.payload(), &crud_broker, &limits);
    // 応答のフレームは共有メモリが使えればそこへ直接書く (§11)
    let encoded = frame_len(&response).and_then(|len| encoder.encode(&response, len));
    let bytes = match encoded {
        Ok(b) => b,
        Err(e) => {
            log::error!("zenoh_broker: can not write the response frame. {e:#}");
            match OutgoingFrame::error(response.header().id, "", "", format!("{e:#}")).to_bytes() {
                Ok(b) => ZBytes::from(b),
                Err(_) => return,
            }
        }
    };
    if let Err(e) = query.reply(CRUD_KEY, bytes).wait() {
        log::error!("zenoh_broker: replying to a query failed. {e}");
    }
}

/// ブローカーが止まる (on_start のタスクが abort される) と、受け取り用のスレッドに知らせる
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// listen するホストとポート。ポート 0 は空いているポートを選ぶ
fn host_and_port(manifest: &Value) -> anyhow::Result<(String, u16)> {
    let host = obj_get_str(manifest, "host").unwrap_or("0.0.0.0").to_owned();
    let port = match obj_get_i64(manifest, "port") {
        Ok(p) => u16::try_from(p).map_err(|_| anyhow!("invalid zenoh broker port {p}"))?,
        Err(_) => DEFAULT_PORT,
    };
    if port != 0 {
        return Ok((host, port));
    }
    // Zenoh から割り当てられたポートを聞く安定した API が無いので、先に空いているポートを探す
    let listener = TcpListener::bind((host.as_str(), 0))
        .map_err(|e| anyhow!("can not find a free port on {host}. {e}"))?;
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

pub async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    log::info!("zenoh_broker::on_start(broker_manifest='{broker_manifest:}') called");
    let limits = frame_limits(&broker_manifest);
    let (host, port) = match host_and_port(&broker_manifest) {
        Ok(v) => v,
        Err(e) => {
            log::error!("zenoh_broker: {e:#}");
            return;
        }
    };
    let locator = format!("tcp/{}", host_port_string(&host, port));
    let config = match zenoh_config(&broker_manifest, Some(&locator), None, &limits) {
        Ok(c) => c,
        Err(e) => {
            log::error!("zenoh_broker: {e:#}");
            return;
        }
    };
    let session = match zenoh::open(config).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("zenoh_broker: can not listen on {locator}. {e}");
            return;
        }
    };
    let queryable = match session.declare_queryable(CRUD_KEY).await {
        Ok(q) => q,
        Err(e) => {
            log::error!("zenoh_broker: declaring the queryable '{CRUD_KEY}' failed. {}", zerr(e));
            return;
        }
    };
    // 共有メモリの pool を用意する (§11)
    let encoder = Arc::new(FrameEncoder::for_session_async(&session, &broker_manifest).await);
    log::debug!("zenoh_broker: shared memory {}", if encoder.uses_shm() { "on" } else { "off" });
    let stop = Arc::new(AtomicBool::new(false));
    let _stop_on_drop = StopOnDrop(stop.clone());

    // 受け取りは普通のスレッドで。Zenoh のコールバックの中では処理しない (§5.3)
    let crud = crud_broker.clone();
    let spawned = std::thread::Builder::new()
        .name("zenoh_broker_recv".to_owned())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match queryable.recv_timeout(RECV_POLL) {
                    Ok(Some(query)) => {
                        let crud = crud.clone();
                        let encoder = encoder.clone();
                        if let Err(e) = std::thread::Builder::new()
                            .name("zenoh_broker_query".to_owned())
                            .spawn(move || serve_query(query, crud, limits, encoder))
                        {
                            log::error!("zenoh_broker: can not start a thread for a query. {e}");
                        }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            // queryable はここで取り消される
            log::debug!("zenoh_broker: stopped receiving");
        });
    if let Err(e) = spawned {
        log::error!("zenoh_broker: can not start the receiving thread. {e}");
        return;
    }

    // 名前を実際の住所にそろえてから起動済みにする (qmp と同じ)
    {
        let mut b = match juiz_lock(&crud_broker) {
            Ok(b) => b,
            Err(e) => {
                log::error!("zenoh_broker: {e:#}");
                return;
            }
        };
        let name_host = match host.parse::<IpAddr>() {
            Ok(ip) if ip.is_unspecified() => "127.0.0.1".to_owned(),
            _ => host.clone(),
        };
        let _ = b.update_broker_name(host_port_string(&name_host, port).as_str());
        b.set_started();
    }
    log::info!("zenoh_broker: listening on {locator}");
    // ブローカーが止まるまでセッションを持ち続ける。stop() でこのタスクが abort されると、
    // セッションが閉じ、受け取り用のスレッドも終わる
    let _session = session;
    std::future::pending::<()>().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_is_bracketed() {
        assert_eq!(host_port_string("::1", 7448), "[::1]:7448");
        assert_eq!(host_port_string("127.0.0.1", 7448), "127.0.0.1:7448");
        assert_eq!(host_port_string("localhost", 7448), "localhost:7448");
    }

    #[test]
    fn default_host_and_port() {
        assert_eq!(host_and_port(&jvalue!({})).unwrap(), ("0.0.0.0".to_owned(), 7448));
        let (_, port) = host_and_port(&jvalue!({"host": "127.0.0.1", "port": 0})).unwrap();
        assert_ne!(port, 0);
        assert!(host_and_port(&jvalue!({"port": 70000})).is_err());
    }
}
