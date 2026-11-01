//! IPC ブローカー (サーバー側、docs/spec/ipc-broker.md)
//!
//! 同じマシンの別プロセスから、ポートを開けずに呼び出すためのブローカー。要求と応答は
//! 生バイトのフレーム (docs/spec/large-data.md §3) で、画像は PNG にせずに送る。
//!
//! 接続を受け付けるたびに普通のスレッドを作り、そのスレッドが相手が閉じるまで
//! フレームを読む → 処理する → 応答を書く、を繰り返す。tokio のタスクにしないのは、
//! 処理中に BrokerProxy や EC のランタイムを作ったり捨てたりしたときの panic を避けるため (#231 と同じ)。

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use juiz_sdk::anyhow::anyhow;

use super::ipc_name::{validate_ipc_name, DEFAULT_IPC_NAME};
use super::shm_delivery::{is_request, Regions, ShmDelivery};
use super::transport::{self, Listener, RecvStream, Stream};
use crate::brokers::core_broker::CoreBrokerPtr;
use crate::brokers::frame::{read_frame, FrameLimits, FrameReadError, OutgoingFrame};
use crate::brokers::{
    broker_factory_impl::create_broker_factory_impl, localize_identifier, Broker, BrokerFactory,
    BrokerPtr, CRUDBroker,
};
use crate::prelude::*;

/// IPC ブローカーの種類名
pub const IPC_TYPE_NAME: &str = "ipc";

/// manifest からブローカーの名前を求める。省略すると juiz。廃止したキーはエラーにする
pub fn ipc_broker_name(manifest: &Value) -> JuizResult<String> {
    if manifest.get("namespace").is_some() {
        return Err(anyhow!(JuizError::InvalidValueError {
            message: "the 'namespace' key of an ipc broker is removed. Use 'name' to choose the socket (docs/spec/ipc-broker.md §4.1)".to_owned()
        }));
    }
    if manifest.get("buffer_size").is_some() {
        log::warn!("the 'buffer_size' key of an ipc broker is removed and ignored");
    }
    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_IPC_NAME)
        .to_owned();
    validate_ipc_name(&name)?;
    Ok(name)
}

fn frame_limits(manifest: &Value) -> FrameLimits {
    let mut limits = FrameLimits::default();
    if let Some(max) = manifest.get("max_frame_bytes").and_then(|v| v.as_u64()) {
        limits.max_frame_bytes = max;
    }
    limits
}

/// 受け付けた接続。止めるときに閉じる
type Connections = Arc<Mutex<HashMap<u64, Stream>>>;

pub struct IpcBroker {
    name: String,
    limits: FrameLimits,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    stopping: Arc<AtomicBool>,
    listener: Option<Arc<Listener>>,
    accept_thread: Option<JoinHandle<()>>,
    connections: Connections,
    /// 共有メモリ (B) の Topic の push の受け箱と配達 (docs/spec/large-data.md §6.5)
    shm: Arc<ShmDelivery>,
}

impl std::fmt::Debug for IpcBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpcBroker").field("name", &self.name).finish()
    }
}

impl IpcBroker {
    pub fn new(core_broker: CoreBrokerPtr, manifest: Value) -> JuizResult<Self> {
        let name = ipc_broker_name(&manifest)?;
        let limits = frame_limits(&manifest);
        let mut manifest = manifest;
        manifest
            .as_object_mut()
            .ok_or_else(|| anyhow!("ipc broker manifest must be an object"))?
            .insert("name".to_owned(), jvalue!(name));
        let crud_broker = Arc::new(Mutex::new(CRUDBroker::new(core_broker.clone(), manifest)?));
        let (crud, own_name) = (crud_broker.clone(), name.clone());
        let shm = Arc::new(ShmDelivery::new(
            name.clone(),
            core_broker,
            Arc::new(move |cm| handle_request(&crud, &own_name, cm)),
        ));
        Ok(IpcBroker {
            name,
            limits,
            shm,
            crud_broker,
            stopping: Arc::new(AtomicBool::new(false)),
            listener: None,
            accept_thread: None,
            connections: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

/// 自分を指す ipc://<name>/... の識別子を core://core/... にし、どの名前でアクセスされたかを param に書く
/// (qmp の localize_request と同じ。IPC には相手のアドレスが無いので requesting_host_addr は付けない)
fn localize_request(cm: &mut CapsuleMap, own_name: &str) {
    if let Some(id) = cm.get_param("identifier").cloned() {
        let id = localize_identifier(id.as_str(), IPC_TYPE_NAME, Some(own_name));
        cm.set_param("identifier", id.as_str());
    }
    cm.set_param("accessed_broker_id", format!("ipc://{own_name}").as_str());
    cm.set_param("accessed_broker_type", IPC_TYPE_NAME);
    cm.set_param("accessed_broker_name", own_name);
}

fn handle_request(
    crud_broker: &Arc<Mutex<CRUDBroker>>,
    own_name: &str,
    mut cm: CapsuleMap,
) -> JuizResult<CapsulePtr> {
    localize_request(&mut cm, own_name);
    // ロックは dispatcher を取り出す間だけ握り、Process の実行中は外しておく (issue #356)
    let dispatcher = juiz_lock(crud_broker)?.dispatcher();
    dispatcher.on_capsule_map_request(cm, None)
}

fn write_error(stream: &mut Stream, id: u64, class_name: &str, function_name: &str, e: &juiz_sdk::anyhow::Error) -> JuizResult<()> {
    OutgoingFrame::error(id, class_name, function_name, format!("{e:#}")).write_to(stream)
}

/// 1 つの接続を、相手が閉じるまで処理する
fn serve_connection(
    stream: Stream,
    crud_broker: Arc<Mutex<CRUDBroker>>,
    own_name: String,
    limits: FrameLimits,
    stopping: Arc<AtomicBool>,
    shm: Arc<ShmDelivery>,
) -> JuizResult<()> {
    let mut reader = BufReader::with_capacity(
        64 * 1024,
        RecvStream::new(transport::try_clone(&stream)?),
    );
    let mut writer = stream;
    // この接続で対応付けた共有メモリの領域。接続が閉じたら外す (§6.6)
    let mut regions = Regions::new();
    loop {
        let frame = match read_frame(&mut reader, &limits) {
            Ok(f) => f,
            Err(FrameReadError::Closed) => return Ok(()),
            Err(FrameReadError::Broken(e)) => {
                if stopping.load(Ordering::SeqCst) {
                    return Ok(());
                }
                return Err(e);
            }
            Err(FrameReadError::Fatal { id, error }) => {
                log::error!("ipc://{own_name}: {error:#}. The connection is closed.");
                let _ = write_error(&mut writer, id, "", "", &error);
                return Ok(());
            }
            Err(FrameReadError::Invalid { id, error }) => {
                log::error!("ipc://{own_name}: {error:#}");
                write_error(&mut writer, id, "", "", &error)?;
                continue;
            }
        };
        let id = frame.id();
        let class_name = frame
            .header
            .param
            .as_ref()
            .and_then(|p| p.get("class_name").cloned())
            .unwrap_or_default();
        let function_name = frame
            .header
            .param
            .as_ref()
            .and_then(|p| p.get("function_name").cloned())
            .unwrap_or_default();
        let result = if is_request(&frame, "shm", "attach") {
            #[cfg(unix)]
            let r = shm.attach(&frame, &mut regions, reader.get_mut().take_fd());
            #[cfg(windows)]
            let r = shm.attach(&frame, &mut regions);
            r.and_then(|_| OutgoingFrame::response(id, &CapsulePtr::new()))
        } else if frame.has_shared_body() {
            // 本体を読まずにすぐ応答する (§6.5)
            shm.enqueue(frame, &regions)
                .and_then(|_| OutgoingFrame::response(id, &CapsulePtr::new()))
        } else {
            frame
                .into_capsule_map()
                .and_then(|cm| handle_request(&crud_broker, &own_name, cm))
                .and_then(|capsule| OutgoingFrame::response(id, &capsule))
        };
        // 失敗したら必ずエラー応答を返し、接続は保つ
        match result {
            Ok(response) => response.write_to(&mut writer)?,
            Err(e) => {
                log::error!("ipc://{own_name}: {class_name}/{function_name} failed. {e:#}");
                write_error(&mut writer, id, &class_name, &function_name, &e)?;
            }
        }
    }
}

impl Broker for IpcBroker {
    fn start(&mut self) -> JuizResult<()> {
        log::trace!("IpcBroker::start(name={}) called", self.name);
        let listener = Arc::new(transport::bind(&self.name)?);
        log::info!("ipc://{} listens on {}", self.name, listener.location());
        self.stopping.store(false, Ordering::SeqCst);

        let l = listener.clone();
        let crud = self.crud_broker.clone();
        let name = self.name.clone();
        let limits = self.limits;
        let stopping = self.stopping.clone();
        let connections = self.connections.clone();
        let shm = self.shm.clone();
        let next_conn_id = AtomicU64::new(0);
        self.accept_thread = Some(
            std::thread::Builder::new()
                .name(format!("ipc-accept-{name}"))
                .spawn(move || loop {
                    let stream = l.accept();
                    if stopping.load(Ordering::SeqCst) {
                        return;
                    }
                    let stream = match stream {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("ipc://{name}: accept failed. {e}");
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                    };
                    let conn_id = next_conn_id.fetch_add(1, Ordering::SeqCst);
                    if let Ok(c) = transport::try_clone(&stream) {
                        if let Ok(mut conns) = connections.lock() {
                            conns.insert(conn_id, c);
                        }
                    }
                    let (crud, conn_name, stopping, connections, shm) = (
                        crud.clone(),
                        name.clone(),
                        stopping.clone(),
                        connections.clone(),
                        shm.clone(),
                    );
                    let r = std::thread::Builder::new()
                        .name(format!("ipc-conn-{name}-{conn_id}"))
                        .spawn(move || {
                            if let Err(e) =
                                serve_connection(stream, crud, conn_name.clone(), limits, stopping, shm)
                            {
                                log::warn!("ipc://{conn_name}: connection {conn_id} closed. {e:#}");
                            }
                            if let Ok(mut conns) = connections.lock() {
                                conns.remove(&conn_id);
                            }
                        });
                    if let Err(e) = r {
                        log::error!("ipc://{name}: can not start a connection thread. {e}");
                    }
                })?,
        );
        self.listener = Some(listener);
        juiz_lock(&self.crud_broker)?.set_started();
        Ok(())
    }

    fn wait_until_started(&mut self, _timeout: Duration) -> JuizResult<()> {
        // start() の中で待ち受けを始めているので、待つことは無い
        if juiz_lock(&self.crud_broker)?.is_started() {
            Ok(())
        } else {
            Err(anyhow!(JuizError::TimeoutError {}))
        }
    }

    fn stop(&mut self) -> JuizResult<()> {
        log::trace!("IpcBroker::stop(name={}) called", self.name);
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(listener) = self.listener.as_ref() {
            listener.wake();
        }
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
        // ソケットのファイルは Listener の drop で消す
        self.listener = None;
        if let Ok(mut conns) = self.connections.lock() {
            for (_, c) in conns.drain() {
                transport::shutdown(&c);
            }
        }
        self.shm.stop();
        juiz_lock(&self.crud_broker)?.clear_started();
        Ok(())
    }

    fn identifier(&self) -> BrokerIdentifier {
        self.crud_broker.lock().unwrap().identifier()
    }

    fn profile(&self) -> BrokerProfile {
        self.crud_broker.lock().unwrap().profile()
    }
}

impl Drop for IpcBroker {
    fn drop(&mut self) {
        if self.listener.is_some() {
            let _ = self.stop();
        }
    }
}

pub fn ipc_broker_factory(core_broker: CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    fn create_broker_function(core_broker: CoreBrokerPtr, manifest: Value) -> JuizResult<BrokerPtr> {
        Ok(BrokerPtr::new(IpcBroker::new(core_broker, manifest)?))
    }
    create_broker_factory_impl(
        core_broker,
        jvalue!({ "type_name": IPC_TYPE_NAME }),
        create_broker_function,
    )
}
