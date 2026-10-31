use std::{
    collections::HashMap,
    io::BufReader,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
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

use juiz_core::frame::{read_frame, FrameLimits, FrameReadError, OutgoingFrame};

use crate::{frame_limits, BlockingRecv, BlockingSend, READ_BUFFER_BYTES};

/// サーバーへの接続を待つ時間
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// manifest に idle_timeout が無いときのアイドルタイムアウト (秒)
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 30;
/// 使っていない間に接続を保つための送信間隔
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(5);

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
pub struct QuicBrokerProxy {
    //base_url: String,
    name: String,
    rt: Arc<tokio::runtime::Runtime>,
    endpoint: Endpoint,
    connection: Connection,
    limits: FrameLimits,
    next_id: AtomicU64,
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

        log::debug!("[client] connected: addr={}", connection.remote_address());
        // let rt2 = tokio::runtime::Runtime::new().unwrap();

        Ok(QuicBrokerProxy {
            name: name.to_owned(),
            rt: rt.clone(),
            endpoint,
            connection,
            limits: frame_limits(manifest),
            next_id: AtomicU64::new(1),
        })
    }
}

/// 閉じる知らせをサーバーへ送り終えるまで待つ上限
const CLOSE_WAIT: Duration = Duration::from_secs(1);

impl Drop for QuicBrokerProxy {
    /// 接続を明示的に閉じる。閉じずに終わると、サーバー側はアイドルタイムアウトまで
    /// 接続を持ち続け、最後に TimedOut を受け取る (#276)
    fn drop(&mut self) {
        self.connection.close(quinn::VarInt::from_u32(0), b"bye");
        // 閉じる知らせは endpoint の駆動タスクが送るので、送り終えるまで待つ。
        // tokio ランタイムの中から呼ばれても block_on できるよう、別スレッドで待つ (new と同じ)
        let (rt, endpoint) = (self.rt.clone(), self.endpoint.clone());
        let _ = std::thread::spawn(move || {
            rt.block_on(async {
                let _ = tokio::time::timeout(CLOSE_WAIT, endpoint.wait_idle()).await;
            })
        })
        .join();
    }
}

async fn make_endpoint_and_connection(
    name: &str,
    idle_timeout: Option<u64>,
) -> JuizResult<(Endpoint, Connection)> {
    // "localhost:5000" のようなホスト名も使えるように名前解決する。
    // localhost が ::1 と 127.0.0.1 の両方に解決される環境があるので IPv4 を優先する
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(name).await?.collect();
    let server_addr = addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .cloned()
        .ok_or_else(|| anyhow!("can not resolve qmp broker address '{name}'"))?;
    let endpoint = make_client_endpoint(idle_timeout, &server_addr).or_else(|e| {
        log::error!("making client endpoint. Error({e})");
        Err(e)
    })?;
    // サーバーが居ないと QUIC のハンドシェイクは終わらないので、時間を区切る
    let connection = tokio::time::timeout(CONNECT_TIMEOUT, connect(&endpoint, server_addr))
        .await
        .map_err(|_| anyhow!("connecting to qmp broker '{name}' ({server_addr}) timed out"))??;
    Ok((endpoint, connection))
}

/// create の payload (オブジェクトの Value) を CapsuleMap にする。
/// `{"__map__": ..., "__param__": ...}` の形 (container_create など) なら中身を取り出す (IPC と同じ)
fn value_to_map(payload: Value) -> JuizResult<CapsuleMap> {
    match payload {
        Value::Null => Ok(CapsuleMap::new()),
        v @ Value::Object(_) => CapsuleMap::try_from(v),
        v => Err(anyhow!(JuizError::InvalidValueError {
            message: format!("create payload must be an object ({v})")
        })),
    }
}

impl QuicBrokerProxy {
    /// 新しいストリームに要求のフレームを書いて閉じ、応答のフレームを読む
    fn request(&self, frame: &OutgoingFrame) -> JuizResult<CapsulePtr> {
        let id = frame.header().id;
        let (send, recv) = futures::executor::block_on(self.connection.open_bi())
            .map_err(|e| anyhow!(e).context(format!("qmp://{}: can not open a stream", self.name)))?;
        let mut writer = BlockingSend(send);
        frame
            .write_to(&mut writer)
            .and_then(|_| writer.finish())
            .map_err(|e| e.context(format!("sending a request to qmp://{} failed", self.name)))?;
        let mut reader = BufReader::with_capacity(READ_BUFFER_BYTES, BlockingRecv(recv));
        let response = read_frame(&mut reader, &self.limits).map_err(|e| {
            let closed = matches!(e, FrameReadError::Closed);
            let e: anyhow::Error = e.into();
            e.context(format!(
                "receiving a response from qmp://{} failed{}",
                self.name,
                if closed { " (the server closed the stream)" } else { "" }
            ))
        })?;
        if response.id() != id {
            return Err(anyhow!(
                "qmp://{}: the response id ({}) does not match the request id ({id})",
                self.name,
                response.id()
            ));
        }
        response.into_capsule_ptr()
    }

    fn crud_request(
        &self,
        method_name: &str,
        class_name: &str,
        function_name: &str,
        mut map: CapsuleMap,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("QuicBrokerProxy::{method_name}({class_name}, {function_name}) called");
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

impl CRUDBrokerProxy for QuicBrokerProxy {
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

fn make_client_endpoint(
    idle_timeout: Option<u64>,
    server_addr: &SocketAddr,
) -> anyhow::Result<Endpoint> {
    // 127.0.0.1 にバインドすると他のマシンのサーバーに届かないので、すべてのインターフェースを使う
    let bind_addr: SocketAddr = if server_addr.is_ipv4() {
        "0.0.0.0:0".parse()?
    } else {
        "[::]:0".parse()?
    };
    let mut endpoint = Endpoint::client(bind_addr).or_else(|e| {
        log::error!("Endpoint::client() error. Error({e})");
        Err(anyhow!(e))
    })?;
    let mut transport_config = TransportConfig::default();
    // 指定が無ければ、相手が落ちたことに気づけるよう有限のアイドルタイムアウトにし、
    // 使っていない間もキープアライブで接続を保つ
    let idle_timeout = idle_timeout.unwrap_or(DEFAULT_IDLE_TIMEOUT_SECS);
    transport_config.max_idle_timeout(Some(Duration::from_secs(idle_timeout).try_into()?));
    transport_config.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));

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
