//! Topic の共有メモリ (方式 B) の設定・数え上げ・送り手のリング (docs/spec/large-data.md §6)
//!
//! conf のトップレベルの `topics:` に Topic ごとに書く。
//!
//! ```yaml
//! topics:
//!   - name: camera/image
//!     shared_memory:
//!       slots: 4            # 既定 4、2 以上
//!       slot_bytes: auto    # 既定 auto (最初の本体の長さを 1 MiB の倍数に切り上げ)
//!       min_bytes: 1048576  # これより小さい本体はフレームに入れて送る (A)。既定 1 MiB
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use juiz_sdk::anyhow::{anyhow, Context};
use juiz_sdk::topic_identifier::resolve_topic_name;

use crate::prelude::*;

pub const DEFAULT_SLOTS: u32 = 4;
pub const DEFAULT_MIN_BYTES: u64 = 1 << 20;
/// slot_bytes が auto のときの切り上げの単位
pub const AUTO_SLOT_UNIT: u64 = 1 << 20;

/// Topic ごとの共有メモリの設定
#[derive(Debug, Clone, PartialEq)]
pub struct SharedMemoryConfig {
    pub slots: u32,
    /// None は auto
    pub slot_bytes: Option<u64>,
    pub min_bytes: u64,
}

impl Default for SharedMemoryConfig {
    fn default() -> Self {
        SharedMemoryConfig {
            slots: DEFAULT_SLOTS,
            slot_bytes: None,
            min_bytes: DEFAULT_MIN_BYTES,
        }
    }
}

fn invalid(message: String) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::InvalidValueError { message })
}

impl SharedMemoryConfig {
    /// `shared_memory:` の値を読む
    pub fn from_value(v: &Value) -> JuizResult<Self> {
        let mut c = SharedMemoryConfig::default();
        let obj = v
            .as_object()
            .ok_or_else(|| invalid(format!("shared_memory must be an object ({v})")))?;
        for (key, value) in obj.iter() {
            match key.as_str() {
                "slots" => {
                    c.slots = value
                        .as_u64()
                        .filter(|s| *s >= 2 && *s <= u32::MAX as u64)
                        .ok_or_else(|| invalid(format!("shared_memory.slots must be an integer of 2 or more ({value})")))?
                        as u32;
                }
                "slot_bytes" => {
                    c.slot_bytes = match value {
                        Value::String(s) if s == "auto" => None,
                        v => Some(
                            v.as_u64()
                                .filter(|b| *b > 0 && *b % 4096 == 0)
                                .ok_or_else(|| invalid(format!("shared_memory.slot_bytes must be \"auto\" or a positive multiple of 4096 ({v})")))?,
                        ),
                    };
                }
                "min_bytes" => {
                    c.min_bytes = value
                        .as_u64()
                        .ok_or_else(|| invalid(format!("shared_memory.min_bytes must be a non-negative integer ({value})")))?;
                }
                other => {
                    return Err(invalid(format!("unknown key '{other}' in shared_memory")));
                }
            }
        }
        Ok(c)
    }
}

/// conf のトップレベルの `topics:` を読み、解決した Topic の名前 → 共有メモリの設定にする。
/// 名前は System の既定ネームスペースで解決する (Topic の名前と同じ、#149)
pub fn topic_settings_from_manifest(manifest: &Value) -> JuizResult<HashMap<String, SharedMemoryConfig>> {
    let mut settings = HashMap::new();
    let Some(topics) = manifest.get("topics") else {
        return Ok(settings);
    };
    let topics = topics
        .as_array()
        .ok_or_else(|| invalid(format!("'topics' must be an array ({topics})")))?;
    let namespace = manifest
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches('/');
    for t in topics.iter() {
        let name = t
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| invalid(format!("each entry of 'topics' must have a 'name' ({t})")))?;
        let name = resolve_topic_name(namespace, name)?;
        if let Some(shm) = t.get("shared_memory") {
            let config = SharedMemoryConfig::from_value(shm)
                .with_context(|| format!("reading shared_memory of topic '{name}'"))?;
            settings.insert(name, config);
        }
    }
    Ok(settings)
}

/// 送り手と受け手の数え上げ (§6.8)。Topic の profile に出す
#[derive(Debug, Default)]
pub struct SharedMemoryStats {
    /// 送り手として設定がある、または受け手として共有メモリで受け取った
    active: AtomicBool,
    slots: AtomicU64,
    slot_bytes: AtomicU64,
    pub sent: AtomicU64,
    pub fallback: AtomicU64,
    pub dropped: AtomicU64,
    pub overwritten: AtomicU64,
}

impl SharedMemoryStats {
    pub fn set_layout(&self, slots: u32, slot_bytes: u64) {
        self.active.store(true, Ordering::Relaxed);
        self.slots.store(slots as u64, Ordering::Relaxed);
        self.slot_bytes.store(slot_bytes, Ordering::Relaxed);
    }

    pub fn set_active(&self) {
        self.active.store(true, Ordering::Relaxed);
    }

    pub fn add(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// profile の "shared_memory"。使っていなければ None。slot_bytes は決まるまで "auto"
    pub fn profile(&self) -> Option<Value> {
        if !self.active.load(Ordering::Relaxed) {
            return None;
        }
        let slot_bytes = match self.slot_bytes.load(Ordering::Relaxed) {
            0 => jvalue!("auto"),
            b => jvalue!(b),
        };
        Some(jvalue!({
            "slots": self.slots.load(Ordering::Relaxed),
            "slot_bytes": slot_bytes,
            "sent": self.sent.load(Ordering::Relaxed),
            "fallback": self.fallback.load(Ordering::Relaxed),
            "dropped": self.dropped.load(Ordering::Relaxed),
            "overwritten": self.overwritten.load(Ordering::Relaxed),
        }))
    }
}

/// 送り手の Topic が持つ共有メモリ。リングは最初に B で送るときに作る
pub struct TopicSharedMemory {
    pub config: SharedMemoryConfig,
    stats: Arc<SharedMemoryStats>,
    #[cfg(feature = "ipc-broker")]
    ring: std::sync::Mutex<Option<Arc<crate::brokers::ipc::shm::ShmRing>>>,
    #[allow(unused)]
    warned_fallback: AtomicBool,
}

/// リングに書いた本体と、IPC で送る本体の説明 (shm 付き)
#[cfg(feature = "ipc-broker")]
pub struct SharedPush {
    pub ring: Arc<crate::brokers::ipc::shm::ShmRing>,
    pub desc: crate::brokers::frame::BodyDesc,
}

impl TopicSharedMemory {
    pub fn new(config: SharedMemoryConfig, stats: Arc<SharedMemoryStats>) -> Self {
        stats.set_layout(config.slots, config.slot_bytes.unwrap_or(0));
        TopicSharedMemory {
            config,
            stats,
            #[cfg(feature = "ipc-broker")]
            ring: std::sync::Mutex::new(None),
            warned_fallback: AtomicBool::new(false),
        }
    }

    /// 本体を共有メモリに書く (§6.3)。B で送らないもの (本体に出さない値、min_bytes より小さい、
    /// 枠より大きい) は None で、呼び出し側はフレームに入れて送る (A)
    #[cfg(feature = "ipc-broker")]
    pub fn prepare(&self, topic_name: &str, capsule: &CapsulePtr) -> JuizResult<Option<SharedPush>> {
        use crate::brokers::frame::SharedBody;
        use crate::brokers::ipc::shm::{RingLayout, ShmRing};

        let Some(body) = SharedBody::from_capsule(capsule)? else {
            return Ok(None);
        };
        let len = body.len();
        if len < self.config.min_bytes || len == 0 {
            return Ok(None);
        }
        let ring = {
            let mut guard = self
                .ring
                .lock()
                .map_err(|_| anyhow!("TopicSharedMemory lock is poisoned"))?;
            match guard.as_ref() {
                Some(r) => r.clone(),
                None => {
                    let slot_bytes = self
                        .config
                        .slot_bytes
                        .unwrap_or_else(|| len.div_ceil(AUTO_SLOT_UNIT) * AUTO_SLOT_UNIT);
                    let ring = Arc::new(ShmRing::create(RingLayout::new(self.config.slots, slot_bytes)?)?);
                    log::info!(
                        "Topic({topic_name}): shared memory ring is created ({} slots x {slot_bytes} bytes)",
                        self.config.slots
                    );
                    self.stats.set_layout(self.config.slots, slot_bytes);
                    *guard = Some(ring.clone());
                    ring
                }
            }
        };
        if len > ring.layout().slot_bytes {
            SharedMemoryStats::add(&self.stats.fallback);
            if !self.warned_fallback.swap(true, Ordering::Relaxed) {
                log::warn!(
                    "Topic({topic_name}): the body ({len} bytes) is larger than a shared memory slot ({} bytes). It is sent through the socket. Set shared_memory.slot_bytes in the conf to use shared memory.",
                    ring.layout().slot_bytes
                );
            }
            return Ok(None);
        }
        let shm = ring.write(len, |dst| body.copy_into(dst))?;
        SharedMemoryStats::add(&self.stats.sent);
        let mut desc = body.desc;
        desc.set_shm(Some(shm));
        Ok(Some(SharedPush { ring, desc }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_value() {
        assert_eq!(SharedMemoryConfig::from_value(&jvalue!({})).unwrap(), SharedMemoryConfig::default());
        let c = SharedMemoryConfig::from_value(&jvalue!({"slots": 3, "slot_bytes": 8192, "min_bytes": 0})).unwrap();
        assert_eq!((c.slots, c.slot_bytes, c.min_bytes), (3, Some(8192), 0));
        assert_eq!(SharedMemoryConfig::from_value(&jvalue!({"slot_bytes": "auto"})).unwrap().slot_bytes, None);
        for bad in [
            jvalue!({"slots": 1}),
            jvalue!({"slot_bytes": 1000}),
            jvalue!({"slot_bytes": "big"}),
            jvalue!({"min_bytes": -1}),
            jvalue!({"slot": 4}),
            jvalue!(4),
        ] {
            assert!(SharedMemoryConfig::from_value(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn settings_resolve_names() {
        let s = topic_settings_from_manifest(&jvalue!({
            "namespace": "robot1",
            "topics": [
                {"name": "camera/image", "shared_memory": {}},
                {"name": "/abs", "shared_memory": {"slots": 2}},
                {"name": "no_shm"},
            ]
        }))
        .unwrap();
        assert_eq!(s.len(), 2);
        assert!(s.contains_key("robot1/camera/image"));
        assert_eq!(s["abs"].slots, 2);
        assert!(topic_settings_from_manifest(&jvalue!({"topics": [{"shared_memory": {}}]})).is_err());
    }
}
