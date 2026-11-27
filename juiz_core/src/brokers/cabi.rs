//! C ABI のブローカーのホスト側の共通の処理 (docs/spec/broker-plugin-abi.md §6、issue #636)
//!
//! プラグイン (`broker_factory_cabi` を持つ cdylib) はフレームのバイト列を運ぶだけで、ここが
//! フレームを作る・読む、自分宛ての ID の書き換え ([`localize_request`])、`CRUDBroker` への振り分け、
//! エラー応答、id の照合をする。C の構造体の受け渡しは juiz_sdk の `broker_cabi::host` がする。
//!
//! - サーバー: [`CabiBroker`]。`server_start` を別のスレッドで呼び、返った住所に名前を直して起動済みにする。
//!   要求はプラグインのスレッドから [`CabiServerHandler::handle`] に来る (§6.1)
//! - クライアント: [`CabiBrokerProxy`]。`CRUDBrokerProxyHolder` で包むので、`BrokerProxy` の各メソッドは
//!   qmp・Zenoh と同じになる (§6.2)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{self, Duration};

use juiz_sdk::anyhow::{self, anyhow};
use juiz_sdk::broker_cabi::host::{Client, PluginReader, PluginSender, RunningServer, ServerHandler};
use juiz_sdk::broker_cabi::{JuizBrokerEntryFn, JUIZ_BROKER_FRAME_ERROR, JUIZ_BROKER_SEND_FAILED, JUIZ_OK};
use juiz_sdk::frame::{read_frame, FrameLimits, FrameReadError, IncomingFrame, OutgoingFrame};

use crate::brokers::{
    localize_request, Broker, BrokerFactory, BrokerProxy, BrokerProxyFactory, BrokerPtr, CRUDBroker,
    CRUDBrokerProxy, CRUDBrokerProxyHolder, CoreBrokerPtr,
};
use crate::core::CoreWorker;
use crate::prelude::*;

pub use juiz_sdk::broker_cabi::host::{BrokerEntry, BrokerInfo, SUPPORTED_BROKER_ABI_VERSIONS};

/// manifest の max_frame_bytes から、フレームの上限を求める (§5.5、large-data.md §3.5)
fn frame_limits(manifest: &Value) -> FrameLimits {
    let mut limits = FrameLimits::default();
    if let Some(max) = manifest.get("max_frame_bytes").and_then(|v| v.as_u64()) {
        limits.max_frame_bytes = max;
    }
    limits
}

/// 入口の関数から表を読み込む。組み込み (register_builtin_cabi_broker) とテストで使う
pub fn load_builtin_entry(entry_fn: JuizBrokerEntryFn) -> JuizResult<Arc<BrokerEntry>> {
    // SAFETY: entry_fn は export_broker_plugin! が作った broker_factory_cabi
    unsafe { BrokerEntry::load(entry_fn, "builtin", None) }
}

/// 読み込んだ表から、System に登録するブローカーとブローカープロキシのファクトリを作る
pub fn cabi_broker_factories(
    entry: Arc<BrokerEntry>,
    core_broker: CoreBrokerPtr,
) -> JuizResult<(Arc<Mutex<dyn BrokerFactory>>, Arc<Mutex<dyn BrokerProxyFactory>>)> {
    let type_name = entry.type_name().to_owned();
    let bf = CabiBrokerFactory {
        core: ObjectCore::create_factory(JuizObjectClass::BrokerFactory("CabiBrokerFactory"), &type_name),
        core_broker,
        entry: entry.clone(),
    };
    let bpf = CabiBrokerProxyFactory {
        core: ObjectCore::create_factory(
            JuizObjectClass::BrokerProxyFactory("CabiBrokerProxyFactory"),
            &type_name,
        ),
        entry,
    };
    Ok((Arc::new(Mutex::new(bf)), Arc::new(Mutex::new(bpf))))
}

// ---- サーバー (§6.1) ----

pub struct CabiBrokerFactory {
    core: ObjectCore,
    core_broker: CoreBrokerPtr,
    entry: Arc<BrokerEntry>,
}

impl JuizObjectCoreHolder for CabiBrokerFactory {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for CabiBrokerFactory {}

impl BrokerFactory for CabiBrokerFactory {
    fn create_broker(&self, mut manifest: Value) -> JuizResult<BrokerPtr> {
        let info = self.entry.info();
        let type_name = info.type_name.clone();
        if !manifest.is_object() {
            return Err(anyhow!("the manifest of a {type_name} broker must be a map ({manifest})"));
        }
        let obj = manifest.as_object_mut().expect("checked above");
        obj.insert("type_name".to_owned(), jvalue!(type_name));
        // name を省いたときの仮の名前 (§5.3)。起動すると実際に待ち受けた住所に付け直す
        if obj.get("name").and_then(|v| v.as_str()).is_none() {
            let host = obj.get("host").and_then(|v| v.as_str()).unwrap_or("0.0.0.0").to_owned();
            let port = obj.get("port").and_then(|v| v.as_i64()).or(info.default_port.map(i64::from));
            let name = match port {
                Some(p) => format!("{host}:{p}"),
                None => type_name.clone(),
            };
            obj.insert("name".to_owned(), jvalue!(name));
        }
        Ok(BrokerPtr::new(CabiBroker {
            entry: self.entry.clone(),
            crud: Arc::new(Mutex::new(CRUDBroker::new(self.core_broker.clone(), manifest)?)),
            server: Arc::new(Mutex::new(None)),
            starting: None,
        }))
    }
}

/// C ABI のブローカーのサーバー。`CRUDBrokerHolder` と同じく `CRUDBroker` を持ち、
/// 受け取りはプラグインのスレッドがする
pub struct CabiBroker {
    entry: Arc<BrokerEntry>,
    crud: Arc<Mutex<CRUDBroker>>,
    server: Arc<Mutex<Option<RunningServer>>>,
    /// server_start を呼んでいるスレッド
    starting: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for CabiBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CabiBroker")
            .field("entry", &self.entry)
            .field("server", &self.server)
            .finish()
    }
}

/// 0.0.0.0・:: で待ち受けたら、名前は 127.0.0.1 にする (§5.3)
fn normalize_name(name: &str) -> String {
    match name.parse::<SocketAddr>() {
        Ok(addr) if addr.ip().is_unspecified() => format!("127.0.0.1:{}", addr.port()),
        _ => name.to_owned(),
    }
}

impl Broker for CabiBroker {
    fn start(&mut self) -> JuizResult<()> {
        let type_name = self.entry.type_name().to_owned();
        log::trace!("CabiBroker::start(type_name={type_name}) called");
        let entry = self.entry.clone();
        let crud = self.crud.clone();
        let server = self.server.clone();
        let manifest = juiz_lock(&self.crud)?.manifest();
        let handler = Arc::new(CabiServerHandler {
            crud: crud.clone(),
            type_name: type_name.clone(),
            limits: frame_limits(&manifest),
        });
        // server_start は待ち受けを始めるまでブロックするので、別のスレッドで呼ぶ (§6.1)
        let t = std::thread::Builder::new()
            .name(format!("{type_name}_broker_start"))
            .spawn(move || match entry.start_server(&manifest, handler) {
                Ok(running) => {
                    let name = normalize_name(running.name());
                    match server.lock() {
                        Ok(mut s) => *s = Some(running),
                        Err(e) => log::error!("{type_name} broker: {e}"),
                    }
                    match crud.lock() {
                        Ok(mut c) => {
                            let _ = c.update_broker_name(&name);
                            c.set_started();
                            log::info!("{type_name} broker: listening on {name}");
                        }
                        Err(e) => log::error!("{type_name} broker: {e}"),
                    }
                }
                Err(e) => {
                    log::error!("{type_name} broker: {e:#}");
                    if let Ok(mut c) = crud.lock() {
                        c.set_start_failed(format!("{e:#}"));
                    }
                }
            })?;
        self.starting = Some(t);
        Ok(())
    }

    fn wait_until_started(&mut self, timeout: Duration) -> JuizResult<()> {
        let start_time = time::Instant::now();
        loop {
            {
                let crud = juiz_lock(&self.crud)?;
                if crud.is_started() {
                    return Ok(());
                }
                // server_start の失敗は待たずに理由を返す (§5.3)
                if let Some(reason) = crud.start_error() {
                    return Err(anyhow!(reason));
                }
            }
            if start_time.elapsed() > timeout {
                log::error!("wait_until_started failed. Timeout");
                return Err(anyhow!(JuizError::TimeoutError {}));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn stop(&mut self) -> JuizResult<()> {
        log::trace!("CabiBroker::stop(type_name={}) called", self.entry.type_name());
        if let Some(t) = self.starting.take() {
            let _ = t.join();
        }
        // server_stop は処理中の要求が終わるまで待つ (§5.3)
        let running = juiz_lock(&self.server)?.take();
        drop(running);
        Ok(())
    }

    fn identifier(&self) -> BrokerIdentifier {
        self.crud.lock().unwrap().identifier()
    }

    fn profile(&self) -> BrokerProfile {
        self.crud.lock().unwrap().profile()
    }
}

impl Drop for CabiBroker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// サーバーの要求の処理 (§6.1)。プラグインのスレッドから同時にいくつも呼ばれる
pub struct CabiServerHandler {
    crud: Arc<Mutex<CRUDBroker>>,
    type_name: String,
    limits: FrameLimits,
}

impl CabiServerHandler {
    fn dispatch(&self, mut cm: CapsuleMap, peer: Option<SocketAddr>) -> JuizResult<CapsulePtr> {
        // ロックは名前と dispatcher を取り出す間だけ握り、Process の実行中は外しておく (issue #356)
        let (name, dispatcher) = {
            let crud = juiz_lock(&self.crud)?;
            (crud.name(), crud.dispatcher())
        };
        localize_request(&mut cm, &self.type_name, &name);
        dispatcher.on_capsule_map_request(cm, peer)
    }

    /// 要求を読んで処理し、応答のフレームと handle の戻り値を返す。失敗したらエラー応答のフレームにする
    fn respond(&self, request: &mut PluginReader<'_>, peer: Option<SocketAddr>) -> (OutgoingFrame, i64) {
        let type_name = &self.type_name;
        if let Some(len) = request.len() {
            if len > self.limits.max_frame_bytes {
                let m = format!(
                    "the request frame is too large ({len} bytes > max_frame_bytes {} bytes)",
                    self.limits.max_frame_bytes
                );
                log::error!("{type_name} broker: {m}");
                return (OutgoingFrame::error(0, "", "", m), JUIZ_BROKER_FRAME_ERROR);
            }
        }
        match read_frame(request, &self.limits) {
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
                let result = catch_unwind(AssertUnwindSafe(|| {
                    frame
                        .into_capsule_map()
                        .and_then(|cm| self.dispatch(cm, peer))
                        .and_then(|capsule| OutgoingFrame::response(id, &capsule))
                }))
                .unwrap_or_else(|p| Err(anyhow!(panic_message(p.as_ref()))));
                match result {
                    Ok(response) => (response, JUIZ_OK),
                    Err(e) => {
                        log::error!("{type_name} broker: {class_name}/{function_name} failed. {e:#}");
                        (OutgoingFrame::error(id, &class_name, &function_name, format!("{e:#}")), JUIZ_OK)
                    }
                }
            }
            Err(FrameReadError::Fatal { id, error }) | Err(FrameReadError::Invalid { id, error }) => {
                log::error!("{type_name} broker: {error:#}");
                (OutgoingFrame::error(id, "", "", format!("{error:#}")), JUIZ_BROKER_FRAME_ERROR)
            }
            Err(e) => {
                let e: anyhow::Error = e.into();
                log::error!("{type_name} broker: can not read the request frame. {e:#}");
                (
                    OutgoingFrame::error(0, "", "", format!("can not read the request frame. {e:#}")),
                    JUIZ_BROKER_FRAME_ERROR,
                )
            }
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    let m = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    };
    format!("panicked: {m}")
}

/// フレームを送る。書けなければ、そのことを書いたエラー応答を送り直す
fn send_frame(reply: PluginSender<'_>, frame: &OutgoingFrame) -> Result<(), (i64, Option<String>)> {
    let fallback;
    let (frame, len) = match frame.encoded_len() {
        Ok(len) => (frame, len),
        Err(e) => {
            fallback = OutgoingFrame::error(frame.header().id, "", "", format!("can not write the response frame. {e:#}"));
            let len = fallback.encoded_len().map_err(|e| (JUIZ_BROKER_FRAME_ERROR, Some(format!("{e:#}"))))?;
            (&fallback, len)
        }
    };
    reply.send(len, |w| frame.write_to(w).map_err(|e| format!("{e:#}")))
}

impl ServerHandler for CabiServerHandler {
    fn handle(&self, peer_addr: &str, request: &mut PluginReader<'_>, reply: PluginSender<'_>) -> i64 {
        let peer = peer_addr.parse::<SocketAddr>().ok();
        let (response, code) = self.respond(request, peer);
        match send_frame(reply, &response) {
            Ok(()) => code,
            Err((c, refused)) => {
                log::error!(
                    "{} broker: sending the response failed ({c}). {}",
                    self.type_name,
                    refused.unwrap_or_default()
                );
                JUIZ_BROKER_SEND_FAILED
            }
        }
    }
}

// ---- クライアント (§6.2) ----

pub struct CabiBrokerProxyFactory {
    core: ObjectCore,
    entry: Arc<BrokerEntry>,
}

impl JuizObjectCoreHolder for CabiBrokerProxyFactory {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for CabiBrokerProxyFactory {}

impl BrokerProxyFactory for CabiBrokerProxyFactory {
    fn create_broker_proxy(
        &self,
        _core_worker: &CoreWorker,
        manifest: Value,
    ) -> JuizResult<Arc<RwLock<dyn BrokerProxy>>> {
        log::trace!("CabiBrokerProxyFactory::create_broker_proxy({manifest}) called");
        let name = obj_get_str(&manifest, "name")?.to_owned();
        let proxy = CabiBrokerProxy::new(&self.entry, &manifest)?;
        Ok(CRUDBrokerProxyHolder::new("CabiBrokerProxy", self.entry.type_name(), &name, Box::new(proxy))?)
    }

    fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({ "type_name": self.entry.type_name() }))
    }
}

/// C ABI のブローカーのクライアント。要求のフレームを作って client_request で送り、応答のフレームを読む
pub struct CabiBrokerProxy {
    client: Client,
    type_name: String,
    name: String,
    limits: FrameLimits,
    next_id: AtomicU64,
}

/// ブロックする呼び出しを、tokio のワーカーの中なら block_in_place で囲む (§5.4)
fn blocking<R>(f: impl FnOnce() -> R) -> R {
    match tokio::runtime::Handle::try_current() {
        Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
    }
}

impl CabiBrokerProxy {
    /// manifest は conf の broker_proxies の項目か、ID から作った `{"name": <host:port>}`
    pub fn new(entry: &Arc<BrokerEntry>, manifest: &Value) -> JuizResult<Self> {
        let type_name = entry.type_name().to_owned();
        let name = obj_get_str(manifest, "name")?.to_owned();
        let mut m = manifest.clone();
        if let Some(obj) = m.as_object_mut() {
            obj.insert("type_name".to_owned(), jvalue!(type_name));
        }
        let client = entry
            .connect(&m)
            .map_err(|e| anyhow!("{type_name}://{name}: can not connect. {e:#}"))?;
        Ok(CabiBrokerProxy {
            client,
            limits: frame_limits(manifest),
            type_name,
            name,
            next_id: AtomicU64::new(1),
        })
    }

    /// 入口の関数から直接作る (組み込みのブローカーのテスト用)
    pub fn from_entry_fn(entry_fn: JuizBrokerEntryFn, manifest: &Value) -> JuizResult<Self> {
        Self::new(&load_builtin_entry(entry_fn)?, manifest)
    }

    fn label(&self) -> String {
        format!("{}://{}", self.type_name, self.name)
    }

    /// 要求のフレームを送り、応答を読む
    fn request(&self, frame: &OutgoingFrame) -> JuizResult<CapsulePtr> {
        let id = frame.header().id;
        let len = frame.encoded_len()?;
        if len > self.limits.max_frame_bytes {
            return Err(anyhow!(
                "{}: the request frame is too large ({len} bytes > max_frame_bytes {} bytes)",
                self.label(),
                self.limits.max_frame_bytes
            ));
        }
        let limits = self.limits;
        let mut response: Option<IncomingFrame> = None;
        blocking(|| {
            self.client.request(
                len,
                |w| frame.write_to(w).map_err(|e| format!("{e:#}")),
                |r| {
                    if let Some(l) = r.len() {
                        if l > limits.max_frame_bytes {
                            return Err(format!(
                                "the response frame is too large ({l} bytes > max_frame_bytes {} bytes)",
                                limits.max_frame_bytes
                            ));
                        }
                    }
                    match read_frame(r, &limits) {
                        Ok(f) => {
                            response = Some(f);
                            Ok(())
                        }
                        Err(FrameReadError::Closed) => Err("the response is empty".to_owned()),
                        Err(e) => Err(format!("{:#}", anyhow::Error::from(e))),
                    }
                },
            )
        })
        .map_err(|f| anyhow!("{}: {}", self.label(), f.message))?;
        let response = response.ok_or_else(|| anyhow!("{}: no response", self.label()))?;
        // id のわからない要求へのエラー応答は id 0 (上限を超えて読まなかったときなど、§5.3)。その文を返す
        if response.id() == 0 && response.kind() == juiz_sdk::frame::FrameKind::Error {
            return response.into_capsule_ptr();
        }
        if response.id() != id {
            return Err(anyhow!(
                "{}: the response id ({}) does not match the request id ({id})",
                self.label(),
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
        log::trace!("CabiBrokerProxy({})::{method_name}({class_name}, {function_name}) called", self.type_name);
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

/// create の payload (オブジェクトの Value) を CapsuleMap にする (qmp・Zenoh と同じ)
fn value_to_map(payload: Value) -> JuizResult<CapsuleMap> {
    match payload {
        Value::Null => Ok(CapsuleMap::new()),
        v @ Value::Object(_) => CapsuleMap::try_from(v),
        v => Err(anyhow!(JuizError::InvalidValueError {
            message: format!("create payload must be an object ({v})")
        })),
    }
}

impl CRUDBrokerProxy for CabiBrokerProxy {
    fn create(
        &self,
        class_name: &str,
        function_name: &str,
        payload: Value,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        self.crud_request("create", class_name, function_name, value_to_map(payload)?, param)
    }

    fn delete(&self, class_name: &str, function_name: &str, param: HashMap<String, String>) -> JuizResult<CapsulePtr> {
        self.crud_request("delete", class_name, function_name, CapsuleMap::new(), param)
    }

    fn read(&self, class_name: &str, function_name: &str, param: HashMap<String, String>) -> JuizResult<CapsulePtr> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalized() {
        assert_eq!(normalize_name("0.0.0.0:7500"), "127.0.0.1:7500");
        assert_eq!(normalize_name("[::]:7500"), "127.0.0.1:7500");
        assert_eq!(normalize_name("192.168.0.2:7500"), "192.168.0.2:7500");
        assert_eq!(normalize_name("localhost:7500"), "localhost:7500");
    }

    #[test]
    fn limits_from_manifest() {
        assert_eq!(frame_limits(&jvalue!({"max_frame_bytes": 10})).max_frame_bytes, 10);
        assert_eq!(
            frame_limits(&jvalue!({})).max_frame_bytes,
            FrameLimits::default().max_frame_bytes
        );
    }
}
