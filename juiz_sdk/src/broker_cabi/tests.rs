//! グルーとホストの道具の確かめ。メモリの中だけで要求を渡す試験用のブローカー (mem) を
//! export_broker_plugin! で作り、ホストの道具 ([`super::host`]) から読む

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::host::{BrokerEntry, EchoHandler, PluginReader, PluginSender, ServerHandler};
use super::*;
use crate::prelude::*;

/// 待ち受け中の mem サーバー (名前 → ホストの表)
static SERVERS: Mutex<Option<HashMap<String, ServerHost>>> = Mutex::new(None);
static NEXT: AtomicUsize = AtomicUsize::new(1);

struct MemBroker;
struct MemServer(String);
struct MemClient(String);

impl BrokerPlugin for MemBroker {
    const TYPE_NAME: &'static str = "mem";
    const DEFAULT_PORT: Option<u16> = Some(7999);
    type Server = MemServer;
    type Client = MemClient;

    fn start(manifest: Value, host: ServerHost) -> Result<(MemServer, String), BrokerError> {
        if manifest.get("fail").is_some() {
            return Err(BrokerError::error("can not listen on mem:fail"));
        }
        if manifest.get("panic").is_some() {
            panic!("start panicked");
        }
        let name = format!("mem:{}", NEXT.fetch_add(1, Ordering::SeqCst));
        SERVERS.lock().unwrap().get_or_insert_with(HashMap::new).insert(name.clone(), host);
        log::info!(target: "mem_broker", "listening on {name}");
        Ok((MemServer(name.clone()), name))
    }

    fn connect(manifest: Value) -> Result<MemClient, BrokerError> {
        let name = manifest.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        Ok(MemClient(name.to_owned()))
    }
}

impl ServerHandle for MemServer {
    fn stop(self) {
        SERVERS.lock().unwrap().get_or_insert_with(HashMap::new).remove(&self.0);
    }
}

impl ClientHandle for MemClient {
    fn request(&self, request: FrameSource<'_>, response: FrameReceiver<'_>) -> Result<(), BrokerError> {
        let host = SERVERS
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .get(&self.0)
            .copied()
            .ok_or_else(|| BrokerError::unavailable(format!("{}: no such server", self.0)))?;
        // 要求を通り道 (Vec) に書く
        let len = request.len();
        let mut wire = Vec::with_capacity(len as usize);
        request.write_to(&mut wire)?;
        assert_eq!(wire.len() as u64, len);
        // サーバー側: 要求を読ませ、応答を通り道に書かせる
        let mut reply_wire = Vec::new();
        host.handle("10.0.0.1:1234", &mut Cursor::new(&wire), Some(len), |src| {
            reply_wire.reserve(src.len() as usize);
            src.write_to(&mut reply_wire)
        })?;
        // 応答を読ませる
        let n = reply_wire.len() as u64;
        response.receive(&mut Cursor::new(&reply_wire), Some(n))
    }
}

crate::export_broker_plugin!(MemBroker);

fn entry() -> Arc<BrokerEntry> {
    unsafe { BrokerEntry::load(broker_factory_cabi, "mem (test)", None) }.unwrap()
}

/// 要求を送り、応答のバイト列を返す
fn request(client: &host::Client, bytes: &[u8]) -> Result<Vec<u8>, host::CallFailure> {
    let mut out = Vec::new();
    client.request(
        bytes.len() as u64,
        |w| w.write_all(bytes).map_err(|e| e.to_string()),
        |r| {
            assert_eq!(r.len(), Some(bytes.len() as u64));
            r.read_to_end(&mut out).map(|_| ()).map_err(|e| e.to_string())
        },
    )?;
    Ok(out)
}

#[test]
fn entry_table_and_info() {
    let e = entry();
    let info = e.info();
    assert_eq!(info.type_name, "mem");
    assert_eq!(info.abi_version, BROKER_ABI_VERSION);
    assert_eq!(info.plugin_name, "juiz_sdk");
    assert_eq!(info.default_port, Some(7999));
    assert!(info.build_info.as_deref().unwrap().contains("juiz_sdk="));
    // 表の印 (§9.3)
    let t = unsafe { &*broker_factory_cabi() };
    let marker = unsafe { std::ffi::CStr::from_ptr(t.abi_marker) };
    assert_eq!(marker.to_str().unwrap(), BROKER_ABI_MARKER);
    assert_eq!(t.struct_size as usize, std::mem::size_of::<JuizBrokerEntry>());
}

#[test]
fn loopback_echo() {
    let e = entry();
    let server = e.start_server(&jvalue!({}), Arc::new(EchoHandler::default())).unwrap();
    assert!(server.name().starts_with("mem:"), "{}", server.name());
    let client = e.connect(&jvalue!({ "name": server.name() })).unwrap();
    assert_eq!(request(&client, b"hello").unwrap(), b"hello");
    // 大きなもの、空のもの
    let big: Vec<u8> = (0..1_000_000u32).map(|i| i as u8).collect();
    assert_eq!(request(&client, &big).unwrap(), big);
    assert_eq!(request(&client, b"").unwrap(), b"");
    // 並行に
    std::thread::scope(|s| {
        for i in 0..8u8 {
            let client = &client;
            s.spawn(move || assert_eq!(request(client, &[i; 100]).unwrap(), vec![i; 100]));
        }
    });
    drop(server);
    let f = request(&client, b"x").unwrap_err();
    assert_eq!(f.code, JUIZ_BROKER_UNAVAILABLE);
    assert!(f.message.contains("no such server"), "{}", f.message);
}

#[test]
fn server_start_errors() {
    let e = entry();
    let err = e.start_server(&jvalue!({"fail": true}), Arc::new(EchoHandler::default())).unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("broker_factory_cabi.server_start"), "{s}");
    assert!(s.contains("can not listen on mem:fail"), "{s}");
    let err = e.start_server(&jvalue!({"panic": true}), Arc::new(EchoHandler::default())).unwrap_err();
    assert!(format!("{err}").contains("panicked: start panicked"), "{err}");
}

/// ホストの処理: 受け取ったものを数え、決まった応答を返す。"refuse" なら読まずに断る
struct Recording {
    peers: Mutex<Vec<String>>,
}

impl ServerHandler for Recording {
    fn handle(&self, peer_addr: &str, request: &mut PluginReader<'_>, reply: PluginSender<'_>) -> i64 {
        self.peers.lock().unwrap().push(peer_addr.to_owned());
        let mut buf = Vec::new();
        request.read_to_end(&mut buf).unwrap();
        if buf == b"panic" {
            panic!("handler panicked");
        }
        if buf == b"short" {
            // 長さより短く書くと、ホストの write_to はエラーになる
            return match reply.send(10, |w| w.write_all(b"abc").map_err(|e| e.to_string())) {
                Ok(()) => JUIZ_OK,
                Err((code, refused)) => {
                    assert!(refused.unwrap().contains("wrote 3 bytes, not 10 bytes"));
                    code
                }
            };
        }
        let answer = format!("got {}", buf.len());
        match reply.send(answer.len() as u64, |w| w.write_all(answer.as_bytes()).map_err(|e| e.to_string())) {
            Ok(()) => JUIZ_OK,
            Err((code, _)) => code,
        }
    }
}

#[test]
fn handler_errors_and_peer() {
    let e = entry();
    let handler = Arc::new(Recording { peers: Mutex::new(Vec::new()) });
    let server = e.start_server(&jvalue!({}), handler.clone()).unwrap();
    let client = e.connect(&jvalue!({ "name": server.name() })).unwrap();
    assert_eq!(request(&client, b"12345").unwrap(), b"got 5");
    assert_eq!(handler.peers.lock().unwrap().as_slice(), ["10.0.0.1:1234"]);
    // ホストの処理が panic しても外へ出ない
    let f = request(&client, b"panic").unwrap_err();
    assert_eq!(f.code, JUIZ_BROKER_ERROR);
    // 応答を書き切れなかった
    let f = request(&client, b"short").unwrap_err();
    assert_eq!(f.code, JUIZ_BROKER_SEND_FAILED, "{f:?}");
}

#[test]
fn host_refuses_response() {
    let e = entry();
    let server = e.start_server(&jvalue!({}), Arc::new(EchoHandler::default())).unwrap();
    let client = e.connect(&jvalue!({ "name": server.name() })).unwrap();
    let f = client
        .request(
            3,
            |w| w.write_all(b"abc").map_err(|e| e.to_string()),
            |_r| Err("the response is too large".to_owned()),
        )
        .unwrap_err();
    assert_eq!(f.code, JUIZ_BROKER_HOST_REFUSED);
    assert_eq!(f.message, "the response is too large");
    // 書く関数が失敗したとき
    let f = client.request(3, |_w| Err("can not encode".to_owned()), |_r| Ok(())).unwrap_err();
    assert_eq!(f.code, JUIZ_BROKER_HOST_REFUSED);
    assert_eq!(f.message, "can not encode");
}

#[test]
fn abi_version_is_checked() {
    static mut BAD: JuizBrokerEntry = JuizBrokerEntry::new(
        glue_info_stub,
        glue::init,
        glue::server_start::<MemBroker>,
        glue::server_stop::<MemBroker>,
        glue::client_connect::<MemBroker>,
        glue::client_request::<MemBroker>,
        glue::client_close::<MemBroker>,
    );
    extern "C" fn glue_info_stub(out: *const JuizBytesSink) -> i64 {
        glue::info::<MemBroker>(out, "bad", "0.0.0")
    }
    extern "C" fn bad_entry() -> *const JuizBrokerEntry {
        std::ptr::addr_of!(BAD)
    }
    unsafe { (*std::ptr::addr_of_mut!(BAD)).abi_version = 2 };
    let err = unsafe { BrokerEntry::load(bad_entry, "bad", None) }.unwrap_err();
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::BrokerPluginAbiVersionError { plugin_abi: 2, .. })
    ));
    assert!(format!("{err}").contains("Update juiz."), "{err}");
    unsafe {
        (*std::ptr::addr_of_mut!(BAD)).abi_version = 1;
        (*std::ptr::addr_of_mut!(BAD)).client_close = None;
    }
    let err = unsafe { BrokerEntry::load(bad_entry, "bad", None) }.unwrap_err();
    assert!(format!("{err}").contains("no 'client_close'"), "{err}");
    // info の abi_version が表と違う
    extern "C" fn mismatching_info(out: *const JuizBytesSink) -> i64 {
        let b = br#"{"type_name":"bad","abi_version":3,"plugin_name":"bad","plugin_version":"0"}"#;
        unsafe { ((*out).write.unwrap())((*out).ctx, JuizBytes::from_slice(b)) };
        0
    }
    unsafe {
        (*std::ptr::addr_of_mut!(BAD)).client_close = Some(glue::client_close::<MemBroker>);
        (*std::ptr::addr_of_mut!(BAD)).info = Some(mismatching_info);
    }
    let err = unsafe { BrokerEntry::load(bad_entry, "bad", None) }.unwrap_err();
    assert!(format!("{err}").contains("does not match the entry table"), "{err}");
}

#[test]
fn broker_error_from_io() {
    use std::io::{Error, ErrorKind};
    assert_eq!(BrokerError::from(Error::from(ErrorKind::ConnectionRefused)).code(), JUIZ_BROKER_UNAVAILABLE);
    assert_eq!(BrokerError::from(Error::from(ErrorKind::TimedOut)).code(), JUIZ_BROKER_TIMEOUT);
    assert_eq!(BrokerError::from(Error::other("x")).code(), JUIZ_BROKER_ERROR);
    assert_eq!(BrokerError::with_code(5, "x").code(), JUIZ_BROKER_ERROR);
    assert_eq!(BrokerError::timeout("a").context("b").message(), "b. a");
    assert_eq!(error_code_name(JUIZ_BROKER_TIMEOUT), "JUIZ_BROKER_TIMEOUT");
}
