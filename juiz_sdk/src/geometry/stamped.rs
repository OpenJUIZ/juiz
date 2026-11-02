//! 時刻と座標系つきの値 (`juiz_std.common.Time` / `Header`、`juiz_std.geometry.PoseStamped` / `TransformStamped`)
//!
//! ContainerPose の読み出しは [`PoseStamped`] で返る。`header.frame_id` は基準 Container の名前
//! (ネームスペースがあるときは `<ns>/<name>`、docs/spec/transformations.md §5.4)。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use super::pose::{Pose, Transform};
use crate::prelude::*;

/// 時刻 (`juiz_std.common.Time`)。UNIX 時刻の秒と秒未満 [ns]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(default)]
pub struct Time {
    pub sec: i64,
    /// 秒未満 [ns] (0〜999999999)
    pub nanosec: i32,
}

impl Time {
    pub const fn new(sec: i64, nanosec: i32) -> Self {
        Self { sec, nanosec }
    }

    /// 今の時刻
    pub fn now() -> Self {
        SystemTime::now().into()
    }

    /// 秒で表したもの
    pub fn as_secs_f64(&self) -> f64 {
        self.sec as f64 + self.nanosec as f64 * 1e-9
    }
}

impl From<SystemTime> for Time {
    fn from(t: SystemTime) -> Self {
        match t.duration_since(UNIX_EPOCH) {
            Ok(d) => Time::new(d.as_secs() as i64, d.subsec_nanos() as i32),
            Err(e) => {
                // UNIX 時刻より前。秒を切り下げて nanosec を 0〜999999999 に保つ
                let d: Duration = e.duration();
                let (sec, nanos) = (d.as_secs() as i64, d.subsec_nanos() as i32);
                if nanos == 0 {
                    Time::new(-sec, 0)
                } else {
                    Time::new(-sec - 1, 1_000_000_000 - nanos)
                }
            }
        }
    }
}

/// 時刻と座標系の名前 (`juiz_std.common.Header`)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Header {
    pub stamp: Time,
    /// 座標系の名前。juiz では Container の名前
    pub frame_id: String,
}

impl Header {
    pub fn new(stamp: Time, frame_id: &str) -> Self {
        Self {
            stamp,
            frame_id: frame_id.to_owned(),
        }
    }
}

/// 時刻と基準つきの ContainerPose (`juiz_std.geometry.PoseStamped`)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PoseStamped {
    pub header: Header,
    pub pose: Pose,
}

impl PoseStamped {
    pub fn new(reference: &str, stamp: Time, pose: Pose) -> Self {
        Self {
            header: Header::new(stamp, reference),
            pose,
        }
    }

    /// 基準 Container の名前 (`header.frame_id`)
    pub fn reference(&self) -> &str {
        &self.header.frame_id
    }

    pub fn stamp(&self) -> Time {
        self.header.stamp
    }
}

/// 時刻と端点つきの Transform (`juiz_std.geometry.TransformStamped`)。
/// `header.frame_id` が始点 (親)、`child_frame_id` が終点 (子)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransformStamped {
    pub header: Header,
    pub child_frame_id: String,
    pub transform: Transform,
}

impl TransformStamped {
    pub fn new(source: &str, destination: &str, stamp: Time, transform: Transform) -> Self {
        Self {
            header: Header::new(stamp, source),
            child_frame_id: destination.to_owned(),
            transform,
        }
    }

    pub fn source(&self) -> &str {
        &self.header.frame_id
    }

    pub fn destination(&self) -> &str {
        &self.child_frame_id
    }
}

macro_rules! value_conversions {
    ($t:ty, $name:expr) => {
        impl From<$t> for Value {
            fn from(v: $t) -> Value {
                serde_json::to_value(v).expect("geometry values always serialize")
            }
        }

        impl From<$t> for Capsule {
            fn from(v: $t) -> Capsule {
                Value::from(v).into()
            }
        }

        impl TryFrom<&Value> for $t {
            type Error = anyhow::Error;
            fn try_from(value: &Value) -> Result<Self, Self::Error> {
                <$t>::deserialize(value).map_err(|e| {
                    anyhow!(JuizError::DataConversionError {
                        message: format!("{} can not be read from {value}: {e}", $name)
                    })
                })
            }
        }

        impl TryFrom<Value> for $t {
            type Error = anyhow::Error;
            fn try_from(value: Value) -> Result<Self, Self::Error> {
                <$t>::try_from(&value)
            }
        }
    };
}

value_conversions!(Time, "Time");
value_conversions!(Header, "Header");
value_conversions!(PoseStamped, "PoseStamped");
value_conversions!(TransformStamped, "TransformStamped");
