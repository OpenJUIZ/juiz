//! Transformations (Container の位置姿勢と座標変換) の core 側 (docs/spec/transformations.md §4〜§11、Step 2 #369)
//!
//! - [`Transformations`]: System の Transformations 登録所。Transform の一覧・各 Anchor の木・Pose 通知の登録を持ち、
//!   `juiz_sdk::transformations::TransformationsRegistry` を実装する。`CoreStore::transformations` にある。
//! - Anchor (`periodic_anchor` / `static_anchor`) は System に組み込みの Container ([`AnchorContainer`])。
//!   Factory を持たず、`CoreWorker::create_container_ref` が type_name を見て直接作る。

mod anchor;
mod registry;
mod tree;

pub use anchor::{anchor_container_new, AnchorContainer, AnchorState};
pub use registry::{Transformations, TransformationsPtr};
