//! Zenoh ブローカー (docs/spec/zenoh-broker.md)
//!
//! 要求も応答も、Zenoh の query / reply の payload にフレーム (docs/spec/large-data.md §3、先頭は "JZF1")
//! を 1 つ丸ごと載せる。key expression は固定の `juiz/v1/crud` で、どの処理かはフレームのヘッダの
//! param で決まる。通り道は TCP だけで、scouting は切る (§4.3)。
//!
//! ブローカーのプラグインの C ABI (docs/spec/broker-plugin-abi.md、issue #637) で作る。フレームを作る・読む、
//! 自分宛ての ID の書き換え、`CRUDBroker` への振り分け、エラー応答はホスト (juiz_core) がし、ここは
//! フレームのバイト列を Zenoh で運ぶことだけをする。juiz_core には依存しない。
//!
//! - プラグインとして読ませる: feature `plugin` (既定オン) の cdylib が `broker_factory_cabi` を持つ。
//! - 組み込みにする: `juiz_core::register_builtin_cabi_broker(zenoh_broker::broker_entry)` を
//!   System の setup() の前に呼ぶ (juiz_app の feature zenoh-broker はこれをする)。

use juiz_sdk::broker_cabi::{BrokerError, BrokerPlugin, ServerHost};
use juiz_sdk::frame::FrameLimits;
use serde_json::Value;

mod client;
mod server;
mod shm;

/// 共有メモリの pool (bytes) を mlock できるか (RLIMIT_MEMLOCK、§11.3)。できなければ理由
pub use shm::memlock_allows;

/// ブローカーの種類名
pub const TYPE_NAME: &str = "zenoh";
/// 要求を受ける key expression (§5.1)
pub const CRUD_KEY: &str = "juiz/v1/crud";
/// 既定のポート。Zenoh の既定 7447 は zenohd が使うので避ける (§4.1)
pub const DEFAULT_PORT: u16 = 7448;

/// Zenoh ブローカー。[`BrokerPlugin`] の実装
pub struct ZenohBroker;

impl BrokerPlugin for ZenohBroker {
    const TYPE_NAME: &'static str = TYPE_NAME;
    const DEFAULT_PORT: Option<u16> = Some(DEFAULT_PORT);
    type Server = server::Server;
    type Client = client::Client;

    fn start(manifest: Value, host: ServerHost) -> Result<(server::Server, String), BrokerError> {
        server::start(manifest, host)
    }

    fn connect(manifest: Value) -> Result<client::Client, BrokerError> {
        client::Client::connect(&manifest)
    }
}

// 入口の表を返す関数。組み込みにするときは register_builtin_cabi_broker にこれを渡す
juiz_sdk::define_broker_entry!(ZenohBroker, pub fn broker_entry);

/// プラグイン (libzenoh_broker.so など) として読み込まれたときの入口 (broker-plugin-abi.md §3.1)。
/// feature plugin (既定オン) のときだけ置く。juiz_app は default-features = false で組み込むので、
/// ほかのブローカーの同じ名前の入口 (#[no_mangle]) と juiz の中でぶつからない (#491)
#[cfg(feature = "plugin")]
#[unsafe(no_mangle)]
pub extern "C" fn broker_factory_cabi() -> *const juiz_sdk::broker_cabi::JuizBrokerEntry {
    broker_entry()
}

/// 大きなフレームを共有メモリに書くか (feature shared-memory があり、manifest の shared_memory が false でなく、
/// pool を mlock できるとき、§11)。manifest は conf の brokers / broker_proxies の項目
pub fn uses_shared_memory(manifest: &Value) -> bool {
    shm::shm_usable(manifest)
}

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
) -> Result<zenoh::Config, BrokerError> {
    let mut config = zenoh::Config::default();
    let mut set = |key: &str, value: String| -> Result<(), BrokerError> {
        config
            .insert_json5(key, value.as_str())
            .map_err(|e| BrokerError::error(format!("zenoh config '{key}' = {value} is invalid. {e}")))
    };
    let json_err = |e: serde_json::Error| BrokerError::error(e.to_string());
    match (listen, connect) {
        (Some(l), _) => {
            set("mode", "\"peer\"".to_owned())?;
            set("listen/endpoints", serde_json::to_string(&[l]).map_err(json_err)?)?;
        }
        (None, Some(c)) => {
            set("mode", "\"client\"".to_owned())?;
            set("connect/endpoints", serde_json::to_string(&[c]).map_err(json_err)?)?;
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
        let extra = extra.as_object().ok_or_else(|| {
            BrokerError::error(format!("zenoh_config must be a map of zenoh config keys ({extra})"))
        })?;
        for (key, value) in extra.iter() {
            set(key, serde_json::to_string(value).map_err(json_err)?)?;
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_for_server_and_client() {
        let limits = FrameLimits::default();
        let c = zenoh_config(&json!({}), Some("tcp/127.0.0.1:7448"), None, &limits).unwrap();
        assert_eq!(c.get_json("mode").unwrap(), "\"peer\"");
        assert_eq!(c.get_json("scouting/multicast/enabled").unwrap(), "false");
        assert_eq!(c.get_json("scouting/gossip/enabled").unwrap(), "false");
        assert_eq!(
            c.get_json("transport/link/rx/max_message_size").unwrap(),
            limits.max_frame_bytes.to_string()
        );
        let c = zenoh_config(&json!({}), None, Some("tcp/127.0.0.1:7448"), &limits).unwrap();
        assert_eq!(c.get_json("mode").unwrap(), "\"client\"");
        assert_eq!(c.get_json("connect/timeout_ms").unwrap(), "5000");
    }

    #[cfg(feature = "shared-memory")]
    #[test]
    fn config_for_shared_memory() {
        let limits = FrameLimits::default();
        let c = zenoh_config(&json!({}), Some("tcp/127.0.0.1:7448"), None, &limits).unwrap();
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
        let c = zenoh_config(&json!({"shm_pool_bytes": 1048576}), None, Some("tcp/127.0.0.1:7448"), &limits).unwrap();
        assert_eq!(
            c.get_json("transport/shared_memory/transport_optimization/pool_size").unwrap(),
            "1048576"
        );
        let c = zenoh_config(&json!({"shared_memory": false}), None, Some("tcp/127.0.0.1:7448"), &limits).unwrap();
        assert_eq!(c.get_json("transport/shared_memory/enabled").unwrap(), "false");
    }

    #[test]
    fn zenoh_config_overrides_and_errors() {
        let limits = FrameLimits::default();
        let c = zenoh_config(
            &json!({ "zenoh_config": { "transport/link/tx/batch_size": 65535 } }),
            Some("tcp/127.0.0.1:7448"),
            None,
            &limits,
        )
        .unwrap();
        assert_eq!(c.get_json("transport/link/tx/batch_size").unwrap(), "65535");
        assert!(zenoh_config(
            &json!({ "zenoh_config": { "no/such/key": 1 } }),
            None,
            Some("tcp/127.0.0.1:7448"),
            &limits
        )
        .is_err());
        assert!(zenoh_config(&json!({ "zenoh_config": [1] }), None, None, &limits).is_err());
    }

    #[test]
    fn entry_has_the_type_name() {
        let entry = broker_entry();
        assert!(!entry.is_null());
        // SAFETY: 表は static
        let entry = unsafe { &*entry };
        assert_eq!(entry.abi_version, juiz_sdk::broker_cabi::BROKER_ABI_VERSION);
    }
}
