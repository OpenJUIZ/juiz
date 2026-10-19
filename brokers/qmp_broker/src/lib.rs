use std::sync::{Arc, Mutex};

use juiz_core::{
    anyhow::{self, anyhow},
    prelude::*,
    {create_broker_factory_impl, create_broker_proxy_factory_impl, CRUDBroker, CRUDBrokerHolder},
};

mod qmp_broker;
mod qmp_broker_proxy;

/// 1 回のリクエスト/応答で読み取る最大バイト数
pub(crate) const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;

/// サーバー側の処理が失敗したときに返す応答。クライアントはこのキーを見てエラーにする
pub(crate) const ERROR_KEY: &str = "__qmp_error__";

/// ストリームを最後まで読む。RecvStream::read_to_end は 0 で埋めた領域を確保してから
/// 写すので、届いた順に Vec へ足していく（0 埋めの分だけ速い）。
pub(crate) async fn read_all(recv: &mut quinn::RecvStream) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    while let Some(chunk) = recv.read_chunk(usize::MAX, true).await? {
        if buf.len() + chunk.bytes.len() > MAX_MESSAGE_SIZE {
            return Err(anyhow!("message exceeds {MAX_MESSAGE_SIZE} bytes"));
        }
        buf.extend_from_slice(&chunk.bytes);
    }
    Ok(buf)
}

pub(crate) fn error_response(e: &anyhow::Error) -> anyhow::Result<Vec<u8>> {
    value_to_vecu8(&jvalue!({ ERROR_KEY: format!("{e:?}") }))
}

pub(crate) fn value_to_vecu8(val: &Value) -> anyhow::Result<Vec<u8>> {
    rmp_serde::to_vec(val).or_else(|e| Err(anyhow!(e)))
}

pub(crate) fn vecu8_to_value(mut vecu8: Vec<u8>) -> anyhow::Result<Value> {
    rmp_serde::from_slice::<serde_json::Value>(&mut vecu8.as_mut_slice()[..])
        .or_else(|e| Err(anyhow!(e)))
}

pub(crate) fn value_to_request_value(
    class_name: &str,
    function_name: &str,
    method_name: &str,
    payload: Value,
    mut param: std::collections::HashMap<String, String>,
) -> Value {
    param.insert("class_name".to_owned(), class_name.into());
    param.insert("function_name".to_owned(), function_name.into());
    param.insert("method_name".to_owned(), method_name.into());
    jvalue!({
        "__map__": payload,
        "__param__": param
    })
}

pub(crate) fn payload_to_request_value(
    class_name: &str,
    function_name: &str,
    method_name: &str,
    mut payload: CapsuleMap,
    param: std::collections::HashMap<String, String>,
) -> Value {
    payload.set_param("class_name", class_name.into());
    payload.set_param("function_name", function_name.into());
    payload.set_param("method_name", method_name.into());
    for (k, p) in param.iter() {
        payload.set_param(k.as_str(), p.as_str());
    }
    payload.try_into().unwrap()
}

pub(crate) fn to_request_value(
    class_name: &str,
    function_name: &str,
    method_name: &str,
    mut param: std::collections::HashMap<String, String>,
) -> Value {
    param.insert("class_name".to_owned(), class_name.into());
    param.insert("function_name".to_owned(), function_name.into());
    param.insert("method_name".to_owned(), method_name.into());
    jvalue!({
        "__map__": {},
        "__param__": param
    })
}

// pub(crate) fn to_request(value: Value) -> anyhow::Result<CapsuleMap> {
//     value.try_into()
// }

pub async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    qmp_broker::on_start(broker_manifest, crud_broker).await
}

#[no_mangle]
pub unsafe extern "Rust" fn broker_factory(
    core_broker: CoreBrokerPtr,
) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    let _ = juiz_core::env_logger::try_init();

    fn create_broker_function(
        core_broker: CoreBrokerPtr,
        manifest: Value,
    ) -> JuizResult<BrokerPtr> {
        Ok(BrokerPtr::new(CRUDBrokerHolder::new(
            "QuinnBroker",
            "qmp",
            core_broker,
            &on_start,
            manifest.clone(),
        )?))
    }

    let manifest = jvalue!({
        "type_name": "qmp"
    });

    create_broker_factory_impl(core_broker, manifest, create_broker_function)
}

#[no_mangle]
pub unsafe extern "Rust" fn broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>
{
    let manifest = jvalue!({
        "type_name": "qmp"
    });
    create_broker_proxy_factory_impl(manifest, qmp_broker_proxy::create_broker_proxy_function)
}
