use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use juiz_core::{
    anyhow::{self, anyhow},
    futures,
    prelude::*,
    tokio::{self},
    CRUDBrokerProxy, CRUDBrokerProxyHolder,
};
use quinn::{
    crypto::rustls::QuicClientConfig, ClientConfig, Connection, Endpoint, TransportConfig,
};
use rustls::{
    crypto::ring::default_provider,
    pki_types::{CertificateDer, ServerName, UnixTime},
};

use crate::{
    payload_to_request_value, to_request_value, value_to_request_value, value_to_vecu8,
    vecu8_to_value,
};

pub(crate) fn create_broker_proxy_function(
    _core_broker: &CoreWorker,
    manifest: Value,
) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
    log::trace!("create_broker_proxy_function({manifest:}) called");
    let name = obj_get_str(&manifest, "name")?;
    Ok(CRUDBrokerProxyHolder::new(
        "QuicBrokerProxy",
        "qmp",
        name,
        Box::new(QuicBrokerProxy::new(&manifest)?),
    )?)
}

//use juiz_core::futures;

#[allow(unused)]
struct QuicBrokerProxy {
    //base_url: String,
    rt: Arc<tokio::runtime::Runtime>,
    endpoint: Endpoint,
    connection: Connection,
}

fn manifest_to_host_and_port(manifest: &Value) -> JuizResult<(&str, &str, i64)> {
    match obj_get_str(manifest, "name") {
        Ok(name) => match name_to_host_and_port(name) {
            Err(e) => {
                log::debug!("manifest.name can not extract to host and port.");
                Err(e)
            }
            Ok((host, port)) => Ok((name, host, port)),
        },
        Err(e) => {
            log::debug!("manifest does not include name");
            Err(e)
        }
    }
}

fn name_to_host_and_port(name: &str) -> JuizResult<(&str, i64)> {
    let mut tokens = name.split(':');
    let host = tokens.next();
    if host.is_none() {
        log::error!("name_to_host_and_port(name={name:}) failed.");
        return Err(anyhow!(JuizError::BrokerNameCanNotResolveToURLError {
            given_name: name.to_owned()
        }));
    }
    let port = tokens.next();
    if port.is_none() {
        log::error!("name_to_host_and_port(name={name:}) failed.");
        return Ok((host.unwrap(), 8080));
    }
    Ok((host.unwrap(), port.unwrap().parse()?))
}

impl QuicBrokerProxy {
    pub fn new(manifest: &Value) -> JuizResult<Self> {
        log::trace!("QuicBrokerProxy::new('{manifest:}') called");
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (name, host, port) = manifest_to_host_and_port(manifest).or_else(|e| {
            log::error!("manifest_to_host_and_port({manifest:}) failed.");
            Err(e)
        })?;
        let idle_timeout = obj_get_i64(&manifest, "idle_timeout")
            .and_then(|v| Ok(Some(v as u64)))
            .or::<Option<u64>>(Ok(None))
            .unwrap();

        let address = format!("{:}:{:}", host, port);
        log::trace!(" - address='{address}'");
        let _ = default_provider().install_default();
        //let rt = tokio::runtime::Handle::try_current()?;
        //let (endpoint, connection) = rt.block_on(make_endpoint_and_connection(name)).or_else(|e|{
        let name_str = name.to_owned();
        let rt2 = rt.clone();
        let handle = std::thread::spawn(move || {
            rt2.block_on(make_endpoint_and_connection(
                name_str.as_str(),
                idle_timeout,
            ))
        });
        // let (endpoint, connection) = ??;//?.or_else(|e|{
        let (endpoint, connection) = match handle.join() {
            Ok(r) => r,
            Err(_) => todo!(),
        }?;
        // let (endpoint, connection) = ??;//?.or_else(|e|{
        //let (endpoint, connection) = futures::executor::block_on(make_endpoint_and_connection(name)).or_else(|e|{
        //let (endpoint, connection) = rt.block_on(make_endpoint_and_connection(name)).or_else(|e|{
        //        log::error!("make_endpoint_and_connection() failed. Error: {e:?}");
        //    Err(e)
        //})?;

        println!("[client] connected: addr={}", connection.remote_address());
        // let rt2 = tokio::runtime::Runtime::new().unwrap();

        Ok(QuicBrokerProxy {
            rt: rt.clone(),
            endpoint,
            connection,
        })
    }
}

async fn make_endpoint_and_connection(
    name: &str,
    idle_timeout: Option<u64>,
) -> JuizResult<(Endpoint, Connection)> {
    let endpoint = make_client_endpoint(idle_timeout).or_else(|e| {
        log::error!("making client endpoint. Error({e})");
        Err(e)
    })?;
    let connection = connect(&endpoint, name.parse()?).await?;
    Ok((endpoint, connection))
}

async fn write_and_then<T, F: Fn(Vec<u8>) -> anyhow::Result<T>>(
    connection: &Connection,
    request: Vec<u8>,
    callback: F,
) -> anyhow::Result<T> {
    log::trace!("write_and_then() called");
    let (mut send, mut recv) = connection.open_bi().await.or_else(|e| {
        log::error!("connection.open_bi failed. {e:?}");
        Err(anyhow!(e))
    })?;
    send.write_all(&request).await.or_else(|e| {
        log::error!("SendStream.write_all failed. {e:?}");
        Err(anyhow!(e))
    })?;
    send.finish().or_else(|e| {
        log::error!("SendStream.finish() failed. {e:?}");
        Err(anyhow!(e))
    })?;
    callback(recv.read_to_end(64 * 1024).await.or_else(|e| {
        log::error!("RecvStream.read_to_end() failed. {e:?}");
        Err(anyhow!(e))
    })?)
}

fn response_to_capsule_ptr(response: Value) -> JuizResult<CapsulePtr> {
    Ok(response.into())
}

async fn write_and_then_value<T, F: Fn(serde_json::Value) -> anyhow::Result<T>>(
    connection: &Connection,
    request: serde_json::Value,
    callback: F,
) -> anyhow::Result<T> {
    write_and_then(connection, value_to_vecu8(&request)?, |response| {
        callback(vecu8_to_value(response)?)
    })
    .await
}

impl CRUDBrokerProxy for QuicBrokerProxy {
    fn create(
        &self,
        class_name: &str,
        function_name: &str,
        payload: Value,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!(
            "QuicBrokerProxy::create({class_name},{function_name},{payload:?},{param:?}) called"
        );
        let request = value_to_request_value(class_name, function_name, "create", payload, param);
        futures::executor::block_on(write_and_then_value(
            &self.connection,
            request,
            |response| response_to_capsule_ptr(response),
        ))
    }

    fn delete(
        &self,
        class_name: &str,
        function_name: &str,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("QuicBrokerProxy::delete({class_name},{function_name},{param:?}) called");
        let request = to_request_value(class_name, function_name, "delete", param);
        futures::executor::block_on(write_and_then_value(
            &self.connection,
            request,
            |response| response_to_capsule_ptr(response),
        ))
    }

    fn read(
        &self,
        class_name: &str,
        function_name: &str,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("QuicBrokerProxy::read({class_name},{function_name},{param:?}) called");
        let request = to_request_value(class_name, function_name, "read", param);
        futures::executor::block_on(write_and_then_value(
            &self.connection,
            request,
            |response| response_to_capsule_ptr(response),
        ))
    }

    fn update(
        &self,
        class_name: &str,
        function_name: &str,
        payload: CapsuleMap,
        param: std::collections::HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!(
            "QuicBrokerProxy::update({class_name},{function_name},{payload:?},{param:?}) called"
        );
        let request = payload_to_request_value(class_name, function_name, "update", payload, param);
        futures::executor::block_on(write_and_then_value(
            &self.connection,
            request,
            |response| response_to_capsule_ptr(response),
        ))
    }
}

fn make_client_endpoint(idle_timeout: Option<u64>) -> anyhow::Result<Endpoint> {
    let mut endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap()).or_else(|e| {
        log::error!("Endpoint::client() error. Error({e})");
        Err(anyhow!(e))
    })?;
    let mut transport_config = TransportConfig::default();
    if idle_timeout.is_none() {
        transport_config.max_idle_timeout(None);
    } else {
        transport_config
            .max_idle_timeout(Some(Duration::from_secs(idle_timeout.unwrap()).try_into()?));
    }

    let mut client_config = ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(SkipServerVerification::new())
                .with_no_client_auth(),
        )
        .or_else(|e| {
            log::error!("configuring endpoint error. Error({e})");
            Err(anyhow!(e))
        })?,
    ));
    client_config.transport_config(Arc::new(transport_config));
    // let transport_config = TransportConfig::
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

async fn connect(endpoint: &Endpoint, server_addr: SocketAddr) -> anyhow::Result<Connection> {
    log::trace!("connect(addr='{server_addr}') called");
    endpoint
        .connect(server_addr, "juiz_server")
        .or_else(|e| {
            log::error!("endpoint.connect failed. {e:?}");
            Err(anyhow!(e))
        })?
        .await
        .or_else(|e| {
            log::error!("connection.await failed. {e:?}");
            Err(anyhow!(e))
        })
        .and_then(|connection| {
            log::debug!("connected to {server_addr}");
            Ok(connection)
        })
}

/// Dummy certificate verifier that treats any certificate as valid.
/// NOTE, such verification is vulnerable to MITM attacks, but convenient for testing.
#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
