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

use crate::{value_to_vecu8, vecu8_to_value};

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

fn callback(
    request: Vec<u8>,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    remote_addr: SocketAddr,
) -> anyhow::Result<Vec<u8>> {
    let val = vecu8_to_value(request)?;
    let capsule_ptr = juiz_lock(&crud_broker)?.on_value_request(val, Some(remote_addr))?;
    // let cp = to_request(val)?;
    // let class_name = cp.get_param("class_name").unwrap().clone();
    // let method_name = cp.get_param("method_name").unwrap().clone();
    // let function_name = cp.get_param("function_name").unwrap().clone();
    // //let (class_name, function_name, method_name, payload, param) = to_request(&val)?;
    // let capsule_ptr = match method_name.as_str() {
    //     "create" => juiz_lock(&crud_broker)?.create_class(class_name.as_str(), function_name.as_str(), cp),
    //     "delete" => juiz_lock(&crud_broker)?.delete_class(class_name.as_str(), function_name.as_str(), cp),
    //     "read" => juiz_lock(&crud_broker)?.read_class(class_name.as_str(), function_name.as_str(), cp),
    //     "update" => {
    //         on_update(crud_broker, class_name.as_str(), function_name.as_str(), cp, remote_addr)
    //     }
    //     _ => {
    //         Err(anyhow!(JuizError::InvalidValueError{message: format!("qmp_broker received invalid value. Its method name is unknown ({})", method_name)}))
    //     }
    // }?;
    if capsule_ptr.is_value()? {
        capsule_ptr.lock_as_value(|v| value_to_vecu8(v))?
    } else if capsule_ptr.is_empty()? {
        Ok(value_to_vecu8(&jvalue!({}))?)
    // } else if capsule_ptr.is_mat()? {
    //     todo!()
    } else {
        log::error!("capsule_ptr is unknown data {capsule_ptr:?}");
        todo!()
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
    run_server(
        address.parse().unwrap(),
        idle_timeout,
        Arc::new(move |req, conn: &Connection| {
            callback(req, crud_broker.clone(), conn.remote_address())
        }),
    )
    .await;
}

/// Runs a QUIC server bound to given address.
async fn run_server<F>(addr: SocketAddr, idle_timeout: Option<u64>, callback: Arc<F>)
where
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync + 'static,
{
    let (endpoint, _server_cert) = match make_server_endpoint(addr, idle_timeout) {
        Ok(v) => Ok(v),
        Err(e) => {
            println!("calling make_server_endpoint failed. Error {e:?}");
            Err(e)
        }
    }
    .unwrap();
    //let cb = Arc::new(callback);
    loop {
        log::trace!("endpoint.accept in run_server()");
        match endpoint.accept().await {
            Some(incoming_conn) => {
                tokio::spawn(handle_incoming(incoming_conn, callback.clone()));
            }
            None => {
                log::error!("endpoint.accept returns None");
            }
        }
    }
}

async fn handle_incoming<F>(incoming: Incoming, callback: Arc<F>) -> anyhow::Result<()>
where
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync,
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
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync,
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
    F: Fn(Vec<u8>, &Connection) -> anyhow::Result<Vec<u8>> + Send + Sync,
{
    let mut req = recv.read_to_end(64 * 1024).await.or_else(|e| {
        log::error!("RecvStream::read_to_end() failed. {e:?}");
        return Err(anyhow!(e));
    })?;

    let req_msg = rmp_serde::from_slice::<serde_json::Value>(&mut &req.as_mut_slice()[..])?;

    log::trace!("request : {:?}", req_msg);

    let response = callback(req, connection)?;

    send.write_all(&response).await.or_else(|e| {
        log::error!("write error. {e:?}");
        Err(anyhow!(e))
    })?;
    send.finish().or_else(|e| {
        log::error!("finish error. {e:?}");
        Err(anyhow!(e))
    })
}
