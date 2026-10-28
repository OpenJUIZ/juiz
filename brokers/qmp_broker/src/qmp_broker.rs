use juiz_core::anyhow::Context;
use juiz_core::{
    anyhow::{self, anyhow},
    prelude::*,
    tokio, CRUDBroker,
};
use std::sync::Mutex;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};

use quinn::ServerConfig;
use quinn::{Connection, Endpoint, Incoming};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivatePkcs8KeyDer;

use crate::{frame_limits, BlockingRecv, BlockingSend, READ_BUFFER_BYTES};
use juiz_core::frame::{read_frame, FrameLimits, FrameReadError, OutgoingFrame};
use std::io::BufReader;
use juiz_core::localize_identifier;

#[allow(unused)]
pub fn make_server_endpoint(
    bind_addr: SocketAddr,
    idle_timeout: Option<u64>,
) -> anyhow::Result<(Endpoint, CertificateDer<'static>)> {
    // Result<(Endpoint, CertificateDer<'static>), Box<dyn Error + Send + Sync + 'static>> {
    let (server_config, server_cert) = match configure_server(idle_timeout) {
        Ok(v) => Ok(v),
        Err(e) => {
            log::error!("configure_server() in make_server_endpoint failed. Error({e:?})");
            Err(anyhow!(e))
        }
    }
    .context("configure_server() failed")?;
    let endpoint = match Endpoint::server(server_config.clone(), bind_addr) {
        Ok(v) => Ok(v),
        Err(e) => {
            log::error!("Endpoint::server({server_config:?}, {bind_addr}) in make_server_endpoint failed. Error({e:?})");
            Err(anyhow!(e))
        }
    }.with_context(|| {format!("Endpoint::server({server_config:?}, {bind_addr}) failed")})?;
    Ok((endpoint, server_cert))
}

/// Returns default server configuration along with its certificate.
fn configure_server(
    timeout: Option<u64>,
) -> anyhow::Result<(ServerConfig, CertificateDer<'static>)> {
    //Result<(ServerConfig, CertificateDer<'static>), Box<dyn Error + Send + Sync + 'static>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = CertificateDer::from(cert.cert);
    let priv_key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    let mut server_config =
        ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())?;
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    if timeout.is_none() {
        transport_config.max_idle_timeout(None);
    } else {
        transport_config.max_idle_timeout(Some(Duration::from_secs(timeout.unwrap()).try_into()?));
    }
    Ok((server_config, cert_der))
}

#[allow(unused)]
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

//type CallbackType = Arc<Mutex<dyn Fn(Vec<u8>)-> anyhow::Result<Vec<u8>>  + Send + Sync >> ;

// fn on_update(crud_broker: Arc<Mutex<CRUDBroker>>, class_name:&str, function_name:&str, cp: CapsuleMap, remote_addr: SocketAddr) -> JuizResult<CapsulePtr>{
//     log::trace!("on_update({cp:?}) called");
//     //let class_name = cp.get_param("class_name").unwrap().clone();
//     //let function_name = cp.get_param("function_name").unwrap();
//     let remote_addr_str = remote_addr.to_string();
//     if class_name == "system" && function_name == "add_mastersystem" {
//         let _r = match cp.get("profile") {
//             Ok(capsule_ptr) => {
//                 capsule_ptr.lock_modify_as_value(|v|{
//                     match v.as_object_mut().unwrap().get_mut("subsystem").unwrap().as_object_mut() {
//                         Some(obj) => {
//                             let broker_name = obj.get("broker_name").unwrap().as_str().unwrap().to_owned();
//                             let broker_tokens = broker_name.split(":").collect::<Vec<&str>>();
//                             let port_str = broker_tokens.get(1).unwrap();
//                             let remote_tokens = remote_addr_str.split(":").collect::<Vec<&str>>();
//                             let addr_str = (*remote_tokens.get(0).unwrap()).to_owned();

//                             let new_broker_name = addr_str + ":" + port_str;
//                             obj.insert("broker_name".to_owned(), jvalue!(new_broker_name));
//                         }
//                         None => todo!(),
//                     }
//                 })
//             }
//             Err(_) => todo!(),
//         };
//     }
//     let retval = juiz_lock(&crud_broker)?.update_class(class_name, function_name, cp);
//     log::info!("retval: {retval:?}");

//     return retval;
// }

/// 受け取ったリクエストの param に、どの住所でアクセスされたかを書き込み、
/// 自分自身を指す qmp:// の識別子を core://core/... に書き換える。
///
/// HTTP ルータが Host ヘッダからしていることと同じ。これにより process/list などは
/// qmp://<アクセスされた住所>/... の識別子を返し、それをそのまま渡されても自分宛てに
/// 再接続せずに処理できる。
fn localize_request(cm: &mut CapsuleMap, own_name: &str) {
    let accessed_name = cm
        .get_param("accessed_broker_name")
        .cloned()
        .unwrap_or_else(|| own_name.to_owned());
    if let Some(id) = cm.get_param("identifier").cloned() {
        let mut id = localize_identifier(id.as_str(), "qmp", Some(accessed_name.as_str()));
        id = localize_identifier(id.as_str(), "qmp", Some(own_name));
        cm.set_param("identifier", id.as_str());
    }
    cm.set_param("accessed_broker_id", format!("qmp://{accessed_name}").as_str());
    cm.set_param("accessed_broker_type", "qmp");
    cm.set_param("accessed_broker_name", accessed_name.as_str());
}

fn handle_request(
    crud_broker: &Arc<Mutex<CRUDBroker>>,
    mut cm: CapsuleMap,
    remote_addr: SocketAddr,
) -> JuizResult<CapsulePtr> {
    let crud = juiz_lock(crud_broker)?;
    localize_request(&mut cm, crud.name().as_str());
    crud.on_capsule_map_request(cm, Some(remote_addr))
}

/// 1 つのストリームで、要求のフレームを読み、処理して、応答のフレームを書く。
///
/// ブロックする処理なので spawn_blocking の中で呼ぶ。失敗したら必ずエラー応答を返す
/// (応答しないとクライアントは読み取りで失敗する)。区切りがわからなくなったフレームだけは応答しない。
fn serve_stream(
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    remote_addr: SocketAddr,
    limits: FrameLimits,
) -> anyhow::Result<()> {
    let mut reader = BufReader::with_capacity(READ_BUFFER_BYTES, BlockingRecv(recv));
    let mut writer = BlockingSend(send);
    let response = match read_frame(&mut reader, &limits) {
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
                .and_then(|cm| handle_request(&crud_broker, cm, remote_addr))
                .and_then(|capsule| OutgoingFrame::response(id, &capsule));
            match result {
                Ok(response) => response,
                Err(e) => {
                    log::error!("qmp_broker: {class_name}/{function_name} failed. {e:#}");
                    OutgoingFrame::error(id, &class_name, &function_name, format!("{e:#}"))
                }
            }
        }
        Err(FrameReadError::Closed) => return Ok(()),
        Err(FrameReadError::Broken(e)) => return Err(e),
        Err(FrameReadError::Fatal { id, error }) | Err(FrameReadError::Invalid { id, error }) => {
            log::error!("qmp_broker: {error:#}");
            OutgoingFrame::error(id, "", "", format!("{error:#}"))
        }
    };
    drop(reader);
    response.write_to(&mut writer)?;
    writer.finish()
}

pub async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    //envlogger::init();
    log::info!("qmp_broker::on_start(broker_manifest='{broker_manifest:}') called");
    let _ = default_provider().install_default();

    let host = obj_get_str(&broker_manifest, "host")
        .or::<&str>(Ok("0.0.0.0"))
        .unwrap();
    let port = obj_get_i64(&broker_manifest, "port")
        .or::<i64>(Ok(8080))
        .unwrap();
    let idle_timeout = obj_get_i64(&broker_manifest, "idle_timeout")
        .and_then(|v| Ok(Some(v as u64)))
        .or::<Option<u64>>(Ok(None))
        .unwrap();
    let limits = frame_limits(&broker_manifest);
    let address = format!("{:}:{:}", host, port);
    let endpoint = match bind_server(address.as_str(), idle_timeout) {
        Ok(v) => v,
        Err(e) => {
            log::error!("qmp_broker::on_start(broker_manifest='{broker_manifest:}') failed. Error({e:?})");
            return;
        }
    };
    // バインドできたら名前を実際の住所にそろえてから起動済みにする。
    // System は wait_until_started() で起動を待ち、その直後に profile() を読む (http_broker と同じ)
    {
        let mut b = juiz_lock(&crud_broker).unwrap();
        if let Ok(local_addr) = endpoint.local_addr() {
            let host = if local_addr.ip().is_unspecified() {
                "127.0.0.1".to_owned()
            } else {
                local_addr.ip().to_string()
            };
            let _ = b.update_broker_name(format!("{host}:{}", local_addr.port()).as_str());
        }
        b.set_started();
    }
    run_server(
        endpoint,
        Arc::new(move |send, recv, conn: &Connection| {
            serve_stream(send, recv, crud_broker.clone(), conn.remote_address(), limits)
        }),
    )
    .await;
}

fn bind_server(address: &str, idle_timeout: Option<u64>) -> anyhow::Result<Endpoint> {
    let addr: SocketAddr = address
        .parse()
        .with_context(|| format!("invalid qmp broker address '{address}'"))?;
    let (endpoint, _server_cert) = make_server_endpoint(addr, idle_timeout)?;
    Ok(endpoint)
}

/// Runs a QUIC server on the given endpoint.
async fn run_server<F>(endpoint: Endpoint, callback: Arc<F>)
where
    F: Fn(quinn::SendStream, quinn::RecvStream, &Connection) -> anyhow::Result<()>
        + Send
        + Sync
        + 'static,
{
    loop {
        log::trace!("endpoint.accept in run_server()");
        match endpoint.accept().await {
            Some(incoming_conn) => {
                tokio::spawn(handle_incoming(incoming_conn, callback.clone()));
            }
            None => {
                log::error!("endpoint.accept returns None");
                return;
            }
        }
    }
}

async fn handle_incoming<F>(incoming: Incoming, callback: Arc<F>) -> anyhow::Result<()>
where
    F: Fn(quinn::SendStream, quinn::RecvStream, &Connection) -> anyhow::Result<()>
        + Send
        + Sync
        + 'static,
{
    log::trace!("handle_incoming() called");
    match incoming.await {
        Ok(connecting) => handle_connection(connecting, callback).await,
        Err(e) => {
            log::error!("Incoming.accept() failed. Error {e:?}");
            Err(anyhow!(e))
        }
    }
}

async fn handle_connection<F>(conn: Connection, callback: Arc<F>) -> anyhow::Result<()>
where
    F: Fn(quinn::SendStream, quinn::RecvStream, &Connection) -> anyhow::Result<()>
        + Send
        + Sync
        + 'static,
{
    log::trace!("handle_connection(from:{})", conn.remote_address());
    //async {
    loop {
        match conn.accept_bi().await {
            Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                log::info!("Connection Closed.");
                return Ok(());
            }
            Err(e) => {
                log::error!("Connection::accept_bi() error. {e:?}");
                return Err(anyhow!(e));
            }
            Ok((send, recv)) => {
                // CoreBroker の処理はブロックするので、HTTP ブローカーと同じく blocking スレッドで実行する。
                // 非同期タスク内で実行すると、処理中に BrokerProxy (tokio ランタイムを持つ) を作ったり
                // 捨てたりしたときに panic する。失敗してもそのストリームだけを捨て、接続は保つ
                // (接続を閉じると同じ BrokerProxy からの以後の呼び出しがすべて失敗する)。
                let (callback, c) = (callback.clone(), conn.clone());
                let r = tokio::task::spawn_blocking(move || callback(send, recv, &c))
                    .await
                    .map_err(|e| anyhow!(e))
                    .and_then(|r| r);
                if let Err(e) = r {
                    log::error!("qmp_broker: a stream from {} failed. {e:#}", conn.remote_address());
                }
            }
        }
    }
    //}.instrument(make_span(&conn)).await
}
