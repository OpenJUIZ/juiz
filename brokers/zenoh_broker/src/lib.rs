//! Zenoh ブローカー (docs/spec/zenoh-broker.md)
//!
//! 要求も応答も、Zenoh の query / reply の payload にフレーム (docs/spec/large-data.md §3、先頭は "JZF1")
//! を 1 つ丸ごと載せる。key expression は固定の `juiz/v1/crud` で、どの処理かはフレームのヘッダの
//! param で決まる。通り道は TCP だけで、scouting は切る (§4.3)。

use std::sync::{Arc, Mutex};

use juiz_core::{
    anyhow::{self, anyhow},
    frame::FrameLimits,
    prelude::*,
    {create_broker_factory_impl, create_broker_proxy_factory_impl, CRUDBroker, CRUDBrokerHolder},
};

mod shm;
mod zenoh_broker;
mod zenoh_broker_proxy;

/// Zenoh の CRUD を直接使うとき (list の namespace 絞り込みなど BrokerProxy に無い引数を渡すとき) に使う
pub use zenoh_broker_proxy::ZenohBrokerProxy;
/// 共有メモリの pool (bytes) を mlock できるか (RLIMIT_MEMLOCK、§11.3)。できなければ理由
pub use shm::memlock_allows;

/// ブローカーの種類名
pub const TYPE_NAME: &str = "zenoh";
/// 要求を受ける key expression (§5.1)
pub const CRUD_KEY: &str = "juiz/v1/crud";
/// 既定のポート。Zenoh の既定 7447 は zenohd が使うので避ける (§4.1)
pub const DEFAULT_PORT: u16 = 7448;

/// manifest の max_frame_bytes から、フレームの上限を求める (docs/spec/large-data.md §3.5)
pub(crate) fn frame_limits(manifest: &Value) -> FrameLimits {
    let mut limits = FrameLimits::default();
    if let Some(max) = manifest.get("max_frame_bytes").and_then(|v| v.as_u64()) {
        limits.max_frame_bytes = max;
    }
    limits
}

/// Zenoh の設定を組み立てる (§4.3)。`listen` か `connect` の locator のどちらかを渡す
pub(crate) fn zenoh_config(
    manifest: &Value,
    listen: Option<&str>,
    connect: Option<&str>,
    limits: &FrameLimits,
) -> anyhow::Result<zenoh::Config> {
    let mut config = zenoh::Config::default();
    let mut set = |key: &str, value: String| -> anyhow::Result<()> {
        config
            .insert_json5(key, value.as_str())
            .map_err(|e| anyhow!("zenoh config '{key}' = {value} is invalid. {e}"))
    };
    match (listen, connect) {
        (Some(l), _) => {
            set("mode", "\"peer\"".to_owned())?;
            set("listen/endpoints", serde_json::to_string(&[l])?)?;
        }
        (None, Some(c)) => {
            set("mode", "\"client\"".to_owned())?;
            set("connect/endpoints", serde_json::to_string(&[c])?)?;
            set("connect/timeout_ms", "5000".to_owned())?;
        }
        (None, None) => {}
    }
    set("scouting/multicast/enabled", "false".to_owned())?;
    set("scouting/gossip/enabled", "false".to_owned())?;
    set(
        "transport/link/rx/max_message_size",
        limits.max_frame_bytes.to_string(),
    )?;
    // 共有メモリ (§11)。feature shared-memory が無いと Zenoh の設定にこのキーが無いので触らない
    if cfg!(feature = "shared-memory") {
        if shm::shm_usable(manifest) {
            set(
                "transport/shared_memory/transport_optimization/pool_size",
                shm::shm_pool_bytes(manifest).to_string(),
            )?;
        } else {
            set("transport/shared_memory/enabled", "false".to_owned())?;
        }
    }
    if let Some(extra) = manifest.get("zenoh_config") {
        let extra = extra
            .as_object()
            .ok_or_else(|| anyhow!("zenoh_config must be a map of zenoh config keys ({extra})"))?;
        for (key, value) in extra.iter() {
            set(key, serde_json::to_string(value)?)?;
        }
    }
    Ok(config)
}

/// Zenoh のエラー (Box<dyn Error>) を anyhow にする
pub(crate) fn zerr(e: zenoh::Error) -> anyhow::Error {
    anyhow!("{e}")
}

pub async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    zenoh_broker::on_start(broker_manifest, crud_broker).await
}

/// Zenoh ブローカーのファクトリを作る
pub fn zenoh_broker_factory(core_broker: CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    fn create_broker_function(core_broker: CoreBrokerPtr, mut manifest: Value) -> JuizResult<BrokerPtr> {
        // name は省いてよい (§4.1)。起動すると実際に listen した住所に付け直す
        if manifest.get("name").and_then(|v| v.as_str()).is_none() {
            let host = obj_get_str(&manifest, "host").unwrap_or("0.0.0.0").to_owned();
            let port = obj_get_i64(&manifest, "port").unwrap_or(DEFAULT_PORT as i64);
            manifest["name"] = jvalue!(format!("{host}:{port}"));
        }
        Ok(BrokerPtr::new(CRUDBrokerHolder::new(
            "ZenohBroker",
            TYPE_NAME,
            core_broker,
            &on_start,
            manifest,
        )?))
    }
    create_broker_factory_impl(core_broker, jvalue!({ "type_name": TYPE_NAME }), create_broker_function)
}

/// Zenoh ブローカープロキシのファクトリを作る
pub fn zenoh_broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    create_broker_proxy_factory_impl(
        jvalue!({ "type_name": TYPE_NAME }),
        zenoh_broker_proxy::create_broker_proxy_function,
    )
}

/// Zenoh ブローカーを組み込みのブローカーとして登録する。これ以降に setup() する System は、
/// 設定ファイルの plugins.broker_factories でプラグインを読み込まなくても zenoh を使える。
pub fn register() {
    juiz_core::register_builtin_broker_factory(zenoh_broker_factory, zenoh_broker_proxy_factory);
}

// 以下はプラグイン (libzenoh_broker.so など) として読み込まれたときの入口。
// feature plugin (既定オン) のときだけ置く。juiz_app は default-features = false で組み込むので、
// qmp_broker の同じ名前の入口 (#[no_mangle]) と juiz の中でぶつからない (#491)

#[cfg(feature = "plugin")]
#[no_mangle]
pub unsafe extern "Rust" fn broker_factory(
    core_broker: CoreBrokerPtr,
) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    let _ = juiz_core::env_logger::try_init();
    zenoh_broker_factory(core_broker)
}

#[cfg(feature = "plugin")]
/// プラグインをビルドしたコンパイラと juiz_sdk の版を返す (juiz_sdk::build_info、issue #450)
#[no_mangle]
pub extern "C" fn broker_factory_build_info() -> *const std::os::raw::c_char {
    juiz_sdk::build_info::build_info_ptr()
}

#[cfg(feature = "plugin")]
#[no_mangle]
pub unsafe extern "Rust" fn broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>>
{
    zenoh_broker_proxy_factory()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_for_server_and_client() {
        let limits = FrameLimits::default();
        let c = zenoh_config(&jvalue!({}), Some("tcp/127.0.0.1:7448"), None, &limits).unwrap();
        assert_eq!(c.get_json("mode").unwrap(), "\"peer\"");
        assert_eq!(c.get_json("scouting/multicast/enabled").unwrap(), "false");
        assert_eq!(c.get_json("scouting/gossip/enabled").unwrap(), "false");
        assert_eq!(
            c.get_json("transport/link/rx/max_message_size").unwrap(),
            limits.max_frame_bytes.to_string()
        );
        let c = zenoh_config(&jvalue!({}), None, Some("tcp/127.0.0.1:7448"), &limits).unwrap();
        assert_eq!(c.get_json("mode").unwrap(), "\"client\"");
        assert_eq!(c.get_json("connect/timeout_ms").unwrap(), "5000");
    }

    #[cfg(feature = "shared-memory")]
    #[test]
    fn config_for_shared_memory() {
        let limits = FrameLimits::default();
        let c = zenoh_config(&jvalue!({}), Some("tcp/127.0.0.1:7448"), None, &limits).unwrap();
        // pool (64 MiB) を mlock できない環境 (一般ユーザーの GitHub Actions など) では切る
        let usable = shm::memlock_allows(shm::DEFAULT_SHM_POOL_BYTES).is_ok();
        assert_eq!(c.get_json("transport/shared_memory/enabled").unwrap(), usable.to_string());
        if !usable {
            return;
        }
        assert_eq!(
            c.get_json("transport/shared_memory/transport_optimization/pool_size").unwrap(),
            shm::DEFAULT_SHM_POOL_BYTES.to_string()
        );
        let c = zenoh_config(&jvalue!({"shm_pool_bytes": 1048576}), None, Some("tcp/127.0.0.1:7448"), &limits).unwrap();
        assert_eq!(
            c.get_json("transport/shared_memory/transport_optimization/pool_size").unwrap(),
            "1048576"
        );
        let c = zenoh_config(&jvalue!({"shared_memory": false}), None, Some("tcp/127.0.0.1:7448"), &limits).unwrap();
        assert_eq!(c.get_json("transport/shared_memory/enabled").unwrap(), "false");
    }

    #[test]
    fn zenoh_config_overrides_and_errors() {
        let limits = FrameLimits::default();
        let c = zenoh_config(
            &jvalue!({ "zenoh_config": { "transport/link/tx/batch_size": 65535 } }),
            Some("tcp/127.0.0.1:7448"),
            None,
            &limits,
        )
        .unwrap();
        assert_eq!(c.get_json("transport/link/tx/batch_size").unwrap(), "65535");
        assert!(zenoh_config(
            &jvalue!({ "zenoh_config": { "no/such/key": 1 } }),
            None,
            Some("tcp/127.0.0.1:7448"),
            &limits
        )
        .is_err());
        assert!(zenoh_config(&jvalue!({ "zenoh_config": [1] }), None, None, &limits).is_err());
    }
}
