//! Transformations (Container の位置姿勢と座標変換) の SDK 側の口 (docs/spec/transformations.md)
//!
//! - [`PoseState`]: 各 Container が持つ「Anchor の ID → (ContainerPose, 時刻, 版番号)」の表 (§6.1)。
//!   Container の利用者の状態とは別のロック ([`PoseStatePtr`]) に置く (§9)。
//! - [`TransformationsRegistry`]: System の Transformations 登録所への口。計算と Transform の書き換えは
//!   core の登録所が行う (Step 2、#369)。SDK はこの trait を通して頼むだけ。
//! - [`ContainerPoseHandle`]: Container ごとの PoseState と登録所への口の組。`ContainerImpl` と
//!   `ContainerPtr` が同じものを指すので、ContainerProcess が Container の書き込みロックを持っている間も
//!   登録所は Container のロックを取らずに PoseState を読み書きできる。
//!   読み出しだけの口 [`ContainerPoseReader`] を別に取り出せる (§14、将来の readonly ContainerProcess 用)。

mod handle;
mod pose_state;

pub use handle::{ContainerPoseHandle, ContainerPoseReader, TransformationsRegistry};
pub use pose_state::{PoseEntry, PoseState, PoseStatePtr};
