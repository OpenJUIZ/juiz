//! IPC ブローカーのクライアント (docs/spec/ipc-broker.md §5.2)
//!
//! 要求ごとに param と map を組み立ててフレームで送る。接続は最初の要求で作って使い回し、
//! 要求は 1 つずつ送る (応答を受け取るまで次を送らない)。

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use juiz_sdk::anyhow::anyhow;

use super::ipc_broker::{ipc_broker_name, IPC_TYPE_NAME};
use super::transport::{self, Stream};
use crate::brokers::frame::{read_frame, FrameKind, FrameLimits, FrameReadError, OutgoingFrame};
use crate::brokers::{
    create_broker_proxy_factory_impl, BrokerProxy, BrokerProxyFactory, CRUDBrokerProxy,
    CRUDBrokerProxyHolder,
};
use crate::{core::CoreWorker, prelude::*};

/// 応答の待ち時間の既定値
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

struct Connection {
    reader: BufReader<Stream>,
    writer: Stream,
}

impl Connection {
    fn open(name: &str, timeout: Duration) -> JuizResult<Self> {
        let stream = transport::connect(name)?;
        transport::set_timeout(&stream, Some(timeout))?;
        Ok(Connection {
            reader: BufReader::with_capacity(64 * 1024, transport::try_clone(&stream)?),
            writer: stream,
        })
    }
}

struct State {
    connection: Option<Connection>,
    next_id: u64,
}

pub struct IpcBrokerProxy {
    name: String,
    timeout: Duration,
    limits: FrameLimits,
    state: Mutex<State>,
}

impl IpcBrokerProxy {
    pub fn new(manifest: &Value) -> JuizResult<Self> {
        let name = ipc_broker_name(manifest)?;
        let timeout = manifest
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_TIMEOUT);
        let mut limits = FrameLimits::default();
        if let Some(max) = manifest.get("max_frame_bytes").and_then(|v| v.as_u64()) {
            limits.max_frame_bytes = max;
        }
        Ok(IpcBrokerProxy {
            name,
            timeout,
            limits,
            state: Mutex::new(State {
                connection: None,
                next_id: 1,
            }),
        })
    }

    /// 要求を 1 つ送り、応答を受け取る
    fn request(&self, mut map: CapsuleMap) -> JuizResult<CapsulePtr> {
        // どの名前でアクセスしたかをサーバーに伝える (HTTP の Host ヘッダにあたる)
        map.set_param("accessed_broker_name", self.name.as_str());
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("IpcBrokerProxy lock is poisoned"))?;
        let id = state.next_id;
        state.next_id += 1;
        let frame = OutgoingFrame::request(id, &map)?;

        // 書き終わる前に失敗したら (繋がらない、相手が閉じていた)、接続を作り直して 1 回だけ送り直す
        if let Some(conn) = state.connection.as_ref() {
            if transport::is_closed(&conn.writer) {
                log::debug!("ipc://{}: the connection was closed by the server. Reconnecting.", self.name);
                state.connection = None;
            }
        }
        let mut attempt = 0;
        loop {
            if state.connection.is_none() {
                state.connection = Some(Connection::open(&self.name, self.timeout)?);
            }
            let conn = state.connection.as_mut().unwrap();
            match frame.write_to(&mut conn.writer) {
                Ok(()) => break,
                Err(e) => {
                    state.connection = None;
                    attempt += 1;
                    if attempt > 1 {
                        return Err(e.context(format!("sending a request to ipc://{} failed", self.name)));
                    }
                    log::debug!("ipc://{}: resending the request on a new connection ({e:#})", self.name);
                }
            }
        }

        // 書き終わった後の失敗は、サーバーで処理が済んだかわからないので送り直さない。接続は捨てる
        let conn = state.connection.as_mut().unwrap();
        let response = match read_frame(&mut conn.reader, &self.limits) {
            Ok(f) => f,
            Err(e) => {
                state.connection = None;
                let closed = matches!(e, FrameReadError::Closed);
                let e: juiz_sdk::anyhow::Error = e.into();
                return Err(e.context(format!(
                    "receiving a response from ipc://{} failed{}",
                    self.name,
                    if closed { " (the server closed the connection)" } else { "" }
                )));
            }
        };
        if response.id() != id {
            state.connection = None;
            return Err(anyhow!(
                "ipc://{}: the response id ({}) does not match the request id ({id})",
                self.name,
                response.id()
            ));
        }
        if response.kind() == FrameKind::Request {
            state.connection = None;
        }
        drop(state);
        response.into_capsule_ptr()
    }

    fn crud_request(
        &self,
        method_name: &str,
        class_name: &str,
        function_name: &str,
        map: CapsuleMap,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        let mut map = map;
        for (k, v) in param.iter() {
            map.set_param(k, v);
        }
        map.set_param("method_name", method_name);
        map.set_param("class_name", class_name);
        map.set_param("function_name", function_name);
        self.request(map)
    }
}

/// create の payload (オブジェクトの Value) を CapsuleMap にする。
/// `{"__map__": ..., "__param__": ...}` の形 (container_create など) なら中身を取り出す (HTTP の POST と同じ)
fn value_to_map(payload: Value) -> JuizResult<CapsuleMap> {
    match payload {
        Value::Null => Ok(CapsuleMap::new()),
        v @ Value::Object(_) => CapsuleMap::try_from(v),
        v => Err(anyhow!(JuizError::InvalidValueError {
            message: format!("create payload must be an object ({v})")
        })),
    }
}

impl CRUDBrokerProxy for IpcBrokerProxy {
    fn create(
        &self,
        class_name: &str,
        function_name: &str,
        payload: Value,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("IpcBrokerProxy::create({class_name}, {function_name}) called");
        self.crud_request("create", class_name, function_name, value_to_map(payload)?, param)
    }

    fn delete(
        &self,
        class_name: &str,
        function_name: &str,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("IpcBrokerProxy::delete({class_name}, {function_name}) called");
        self.crud_request("delete", class_name, function_name, CapsuleMap::new(), param)
    }

    fn read(
        &self,
        class_name: &str,
        function_name: &str,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("IpcBrokerProxy::read({class_name}, {function_name}) called");
        self.crud_request("read", class_name, function_name, CapsuleMap::new(), param)
    }

    fn update(
        &self,
        class_name: &str,
        function_name: &str,
        payload: CapsuleMap,
        param: HashMap<String, String>,
    ) -> JuizResult<CapsulePtr> {
        log::trace!("IpcBrokerProxy::update({class_name}, {function_name}) called");
        self.crud_request("update", class_name, function_name, payload, param)
    }
}

fn create_broker_proxy_function(
    _core_broker: &CoreWorker,
    manifest: Value,
) -> JuizResult<Arc<Mutex<dyn BrokerProxy>>> {
    let proxy = IpcBrokerProxy::new(&manifest)?;
    let name = proxy.name.clone();
    Ok(CRUDBrokerProxyHolder::new(
        "IpcBrokerProxy",
        IPC_TYPE_NAME,
        name.as_str(),
        Box::new(proxy),
    )?)
}

pub fn ipc_broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    create_broker_proxy_factory_impl(
        jvalue!({ "type_name": IPC_TYPE_NAME }),
        create_broker_proxy_function,
    )
}
