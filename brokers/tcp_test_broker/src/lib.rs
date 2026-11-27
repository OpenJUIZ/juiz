//! 試験用の TCP ブローカー (ブローカーのプラグインの C ABI、docs/spec/broker-plugin-abi.md、issue #636)
//!
//! juiz_sdk のグルー ([`juiz_sdk::broker_cabi`]) だけで作った C ABI のブローカー。juiz_core には依存しない。
//! フレームの中身は読まず、長さ (u64 LE) を前に付けて TCP で送る (§5.1 の「ストリームのプラグインは
//! 自分の形で長さを付ける」)。サーバーは接続ごとにスレッドを 1 本作り、クライアントは接続を使い回す。
//! 速さは考えていない。conf の種類名は `tcptest`、プラグインの名前は `tcp_test_broker`。
//!
//! manifest のキー: `host` (既定 0.0.0.0)、`port` (既定 7510、0 は空いているポート)、`name` (クライアント)、
//! `timeout_ms` (クライアントの応答の待ち時間、既定 10000)

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use juiz_sdk::broker_cabi::{
    BrokerError, BrokerPlugin, ClientHandle, FrameReceiver, FrameSource, ServerHandle, ServerHost,
    JUIZ_BROKER_FRAME_ERROR, JUIZ_BROKER_SEND_FAILED,
};
use juiz_sdk::log;
use serde_json::Value;

/// ブローカーの種類名
pub const TYPE_NAME: &str = "tcptest";
/// 既定のポート
pub const DEFAULT_PORT: u16 = 7510;
/// manifest に timeout_ms が無いときの応答の待ち時間
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// 要求の合間に、止める知らせを確かめる間隔
const POLL: Duration = Duration::from_millis(100);
/// 読み書きのバッファ
const BUF: usize = 64 << 10;

pub struct TcpTestBroker;

juiz_sdk::export_broker_plugin!(TcpTestBroker);

/// IPv6 のアドレスは [] で囲む
fn host_port_string(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    }
}

fn manifest_port(manifest: &Value) -> Result<u16, BrokerError> {
    match manifest.get("port") {
        None | Some(Value::Null) => Ok(DEFAULT_PORT),
        Some(p) => p
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .ok_or_else(|| BrokerError::error(format!("invalid port {p}"))),
    }
}

impl BrokerPlugin for TcpTestBroker {
    const TYPE_NAME: &'static str = TYPE_NAME;
    const DEFAULT_PORT: Option<u16> = Some(DEFAULT_PORT);
    type Server = Server;
    type Client = Client;

    fn start(manifest: Value, host: ServerHost) -> Result<(Server, String), BrokerError> {
        let bind_host = manifest.get("host").and_then(|v| v.as_str()).unwrap_or("0.0.0.0").to_owned();
        let port = manifest_port(&manifest)?;
        let where_ = host_port_string(&bind_host, port);
        let listener = TcpListener::bind(where_.as_str())
            .map_err(|e| BrokerError::error(format!("can not listen on {where_}. {e}")))?;
        let local = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let ip = if local.ip().is_unspecified() { "127.0.0.1".to_owned() } else { local.ip().to_string() };
        let name = host_port_string(&ip, local.port());
        let stop = Arc::new(AtomicBool::new(false));
        let conns: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::default();
        let accept = {
            let stop = stop.clone();
            let conns = conns.clone();
            std::thread::Builder::new()
                .name("tcptest_accept".to_owned())
                .spawn(move || accept_loop(listener, host, stop, conns))?
        };
        log::info!("tcptest: listening on {name}");
        Ok((Server { stop, accept: Some(accept), conns }, name))
    }

    fn connect(manifest: Value) -> Result<Client, BrokerError> {
        let name = manifest
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BrokerError::error("the manifest has no 'name'"))?
            .to_owned();
        let timeout = match manifest.get("timeout_ms").and_then(|v| v.as_u64()) {
            Some(0) => return Err(BrokerError::error("timeout_ms must be positive")),
            Some(ms) => Duration::from_millis(ms),
            None => DEFAULT_TIMEOUT,
        };
        // つながるかをここで確かめる (つながらなければ BrokerProxy を作るところでエラーにする)。接続は使い回す
        let client = Client { name, timeout, pool: Mutex::new(Vec::new()) };
        let first = client.take_conn()?;
        client.pool.lock().unwrap_or_else(|e| e.into_inner()).push(first);
        Ok(client)
    }
}

// ---- サーバー ----

pub struct Server {
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    conns: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

fn accept_loop(listener: TcpListener, host: ServerHost, stop: Arc<AtomicBool>, conns: Arc<Mutex<Vec<JoinHandle<()>>>>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                let stop = stop.clone();
                let spawned = std::thread::Builder::new()
                    .name("tcptest_conn".to_owned())
                    .spawn(move || {
                        if let Err(e) = serve(stream, peer, host, &stop) {
                            log::debug!("tcptest: the connection from {peer} is closed. {e}");
                        }
                    });
                match spawned {
                    Ok(t) => {
                        let mut conns = conns.lock().unwrap_or_else(|e| e.into_inner());
                        conns.retain(|t| !t.is_finished());
                        conns.push(t);
                    }
                    Err(e) => log::error!("tcptest: can not start a thread for {peer}. {e}"),
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                log::warn!("tcptest: accept failed. {e}");
                std::thread::sleep(POLL);
            }
        }
    }
}

/// 次の要求の長さを読む。要求の合間に止められたら None
fn read_len(stream: &mut BufReader<TcpStream>, stop: &AtomicBool) -> io::Result<Option<u64>> {
    let mut buf = [0u8; 8];
    let mut got = 0;
    // 合間だけ待ち時間を付け、止める知らせを確かめる
    stream.get_ref().set_read_timeout(Some(POLL))?;
    while got < 8 {
        if stop.load(Ordering::SeqCst) && got == 0 {
            return Ok(None);
        }
        match stream.read(&mut buf[got..]) {
            Ok(0) => {
                return if got == 0 { Ok(None) } else { Err(io::ErrorKind::UnexpectedEof.into()) };
            }
            Ok(n) => got += n,
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => {}
            Err(e) => return Err(e),
        }
    }
    stream.get_ref().set_read_timeout(None)?;
    Ok(Some(u64::from_le_bytes(buf)))
}

/// 1 つの接続の要求を順に処理する
fn serve(stream: TcpStream, peer: SocketAddr, host: ServerHost, stop: &AtomicBool) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    let writer = stream.try_clone()?;
    let mut reader = BufReader::with_capacity(BUF, stream);
    let peer = peer.to_string();
    while let Some(len) = read_len(&mut reader, stop)? {
        let mut body = (&mut reader).take(len);
        let r = host.handle(&peer, &mut body, Some(len), |frame: FrameSource<'_>| {
            let mut w = BufWriter::with_capacity(BUF, &writer);
            w.write_all(&frame.len().to_le_bytes())?;
            frame.write_to(&mut w)?;
            w.flush()?;
            Ok(())
        });
        // ホストが読み終えなかった分は読み飛ばし、区切りを保つ (長さはこちらで付けているので分かる)
        if body.limit() > 0 {
            io::copy(&mut body, &mut io::sink())?;
        }
        match r {
            Ok(()) => {}
            Err(e) if e.code() == JUIZ_BROKER_SEND_FAILED => {
                return Err(io::Error::other(format!("sending a response failed. {e}")));
            }
            Err(e) if e.code() == JUIZ_BROKER_FRAME_ERROR => log::warn!("tcptest: {peer}: {e}"),
            Err(e) => return Err(io::Error::other(e.to_string())),
        }
    }
    Ok(())
}

impl ServerHandle for Server {
    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
        // 処理中の要求が終わるまで待つ (§5.3)。合間の接続は POLL のうちに抜ける
        let conns = std::mem::take(&mut *self.conns.lock().unwrap_or_else(|e| e.into_inner()));
        for t in conns {
            let _ = t.join();
        }
        log::debug!("tcptest: stopped");
    }
}

// ---- クライアント ----

pub struct Client {
    name: String,
    timeout: Duration,
    /// 使い終わった接続。同時の要求には別の接続を開く
    pool: Mutex<Vec<TcpStream>>,
}

impl Client {
    /// `<host>:<port>` (ポートを省くと 7510) を繋ぎ先にする。IPv4 を優先する
    fn resolve(&self) -> Result<SocketAddr, BrokerError> {
        let name = self.name.as_str();
        let with_port = match name.parse::<SocketAddr>() {
            Ok(a) => a.to_string(),
            Err(_) if name.rsplit_once(':').is_some_and(|(h, p)| !h.contains(':') && p.parse::<u16>().is_ok()) => name.to_owned(),
            Err(_) => host_port_string(name.trim_start_matches('[').trim_end_matches(']'), DEFAULT_PORT),
        };
        let addrs: Vec<SocketAddr> = with_port
            .to_socket_addrs()
            .map_err(|e| BrokerError::unavailable(format!("can not resolve '{name}'. {e}")))?
            .collect();
        addrs
            .iter()
            .find(|a| a.is_ipv4())
            .or_else(|| addrs.first())
            .copied()
            .ok_or_else(|| BrokerError::unavailable(format!("can not resolve '{name}'")))
    }

    fn take_conn(&self) -> Result<TcpStream, BrokerError> {
        if let Some(s) = self.pool.lock().unwrap_or_else(|e| e.into_inner()).pop() {
            return Ok(s);
        }
        let addr = self.resolve()?;
        let s = TcpStream::connect_timeout(&addr, self.timeout)
            .map_err(|e| BrokerError::unavailable(format!("can not connect to {addr}. {e}")))?;
        s.set_nodelay(true)?;
        Ok(s)
    }

    fn no_response(&self) -> BrokerError {
        BrokerError::timeout(format!("no response within {} ms", self.timeout.as_millis()))
    }
}

impl ClientHandle for Client {
    fn request(&self, request: FrameSource<'_>, response: FrameReceiver<'_>) -> Result<(), BrokerError> {
        let stream = self.take_conn()?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        {
            let mut w = BufWriter::with_capacity(BUF, &stream);
            w.write_all(&request.len().to_le_bytes())?;
            request.write_to(&mut w)?;
            w.flush()?;
        }
        let mut reader = BufReader::with_capacity(BUF, &stream);
        let mut len = [0u8; 8];
        if let Err(e) = reader.read_exact(&mut len) {
            return Err(match e.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => self.no_response(),
                _ => BrokerError::unavailable(format!("{}: the connection was closed. {e}", self.name)),
            });
        }
        let len = u64::from_le_bytes(len);
        let mut body = (&mut reader).take(len);
        response.receive(&mut body, Some(len)).map_err(|e| match e.code() {
            juiz_sdk::broker_cabi::JUIZ_BROKER_TIMEOUT => self.no_response(),
            _ => e,
        })?;
        // 読み終えて区切りが保てた接続だけを使い回す (待ち時間切れの接続には遅れた応答が来うる)
        if body.limit() == 0 && reader.buffer().is_empty() {
            drop(reader);
            self.pool.lock().unwrap_or_else(|e| e.into_inner()).push(stream);
        }
        Ok(())
    }
}
