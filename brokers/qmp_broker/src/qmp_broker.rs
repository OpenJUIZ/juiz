use juiz_core::anyhow::Context;
use juiz_core::{
    anyhow::{self, anyhow},
    prelude::*,
    tokio, CRUDBroker,
};
use std::sync::Mutex;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};

use bytes::Bytes;
use quinn::ServerConfig;
use quinn::{Connection, Endpoint, Incoming};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivatePkcs8KeyDer;

use crate::{error_response, read_all, value_to_vecu8, vecu8_to_value};
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
fn localize_request(mut val: Value, own_name: &str) -> Value {
    let Some(param) = val
        .as_object_mut()
        .and_then(|o| o.get_mut("__param__"))
        .and_then(|p| p.as_object_mut())
    else {
        return val;
    };
    let accessed_name = param
        .get("accessed_broker_name")
        .and_then(|v| v.as_str())
        .unwrap_or(own_name)
        .to_owned();
    if let Some(id) = param.get("identifier").and_then(|v| v.as_str()) {
        let mut id = localize_identifier(id, "qmp", Some(accessed_name.as_str()));
        id = localize_identifier(id.as_str(), "qmp", Some(own_name));
        param.insert("identifier".to_owned(), jvalue!(id));
    }
    param.insert(
        "accessed_broker_id".to_owned(),
        jvalue!(format!("qmp://{accessed_name}")),
    );
    param.insert("accessed_broker_type".to_owned(), jvalue!("qmp"));
    param.insert("accessed_broker_name".to_owned(), jvalue!(accessed_name));
    val
}

fn callback(
    request: Vec<u8>,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    remote_addr: SocketAddr,
) -> anyhow::Result<Vec<u8>> {
    let val = vecu8_to_value(request)?;
    let crud = juiz_lock(&crud_broker)?;
    let val = localize_request(val, crud.name().as_str());
    let capsule_ptr = crud.on_value_request(val, Some(remote_addr))?;
    if capsule_ptr.is_value()? {
        capsule_ptr.lock_as_value(|v| value_to_vecu8(v))?
    } else if capsule_ptr.is_empty()? {
        Ok(value_to_vecu8(&jvalue!({}))?)
    } else {
        Err(anyhow!(JuizError::InvalidValueError {
            message: format!("qmp_broker can not send this data ({capsule_ptr:?})")
        }))
    }
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
        Arc::new(move |req, conn: &Connection| {
            callback(req, crud_broker.clone(), conn.remote_address())
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
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync + 'static,
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
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync + 'static,
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
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync + 'static,
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
            Ok(mut s) => {
                let (send, recv) = &mut s;
                match handle_stream(send, recv, callback.clone(), &conn).await {
                    Ok(_) => {}
                    Err(e) => {
                        return Err(anyhow!(e));
                    }
                }
            }
        }
    }
    //}.instrument(make_span(&conn)).await
}

async fn handle_stream<F>(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    callback: Arc<F>,
    connection: &Connection,
) -> anyhow::Result<()>
where
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync + 'static,
{
    let req = read_all(recv).await.or_else(|e| {
        log::error!("read_all() failed. {e:?}");
        return Err(anyhow!(e));
    })?;

    // CoreBroker の処理はブロックするので、HTTP ブローカーと同じく blocking スレッドで実行する。
    // 非同期タスク内で実行すると、処理中に BrokerProxy (tokio ランタイムを持つ) を作ったり
    // 捨てたりしたときに panic する。
    let conn = connection.clone();
    let response = tokio::task::spawn_blocking(move || callback(req, &conn))
        .await
        .map_err(|e| anyhow!(e))
        .and_then(|r| r);
    // エラーでも応答を返し、接続は保つ。応答しないとクライアントは読み取りで失敗し、
    // 接続を閉じると同じ BrokerProxy からの以後の呼び出しがすべて失敗する。
    let response = match response {
        Ok(v) => v,
        Err(e) => {
            log::error!("qmp_broker request failed. Error({e:?})");
            error_response(&e)?
        }
    };

    // write_all は送信バッファへ複製するので、Vec をそのまま Bytes にして渡す。
    send.write_chunk(Bytes::from(response)).await.or_else(|e| {
        log::error!("write error. {e:?}");
        Err(anyhow!(e))
    })?;
    send.finish().or_else(|e| {
        log::error!("finish error. {e:?}");
        Err(anyhow!(e))
    })
}
