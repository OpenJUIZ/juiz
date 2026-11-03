use std::sync::{Arc, RwLock, Weak};

use anyhow::anyhow;

use super::pose_state::{PoseEntry, PoseState, PoseStatePtr};
use crate::geometry::{Pose, PoseStamped};
use crate::prelude::*;

/// System の Transformations 登録所への口 (docs/spec/transformations.md §7・§9)。
///
/// core の登録所 (Step 2、#369) が実装する。計算規則 (§7.1・§7.2) と Transform の書き換え、
/// StaticAnchor の更新、Pose 通知はすべて実装側で行う。
///
/// ロックの順番 (§9.2): 呼ぶ側は Container の利用者の状態のロックを持っていてよい
/// (ContainerProcess の中から呼ばれる)。実装は Container の利用者の状態のロックを**取ってはならない**。
/// PoseState は [`ContainerPoseHandle::state`] から直接読み書きする。
pub trait TransformationsRegistry: Send + Sync {
    /// Container `container` の、基準 `reference` (ID か名前) から見た ContainerPose (§7.1)
    fn pose(&self, container: &ContainerIdentifier, reference: &str) -> JuizResult<PoseStamped>;

    /// Container `container` を、基準 `reference` から見て `pose` の位置姿勢にする (§7.2)。
    /// 戻り値は書いた後の `pose(container, reference)`
    fn set_pose(
        &self,
        container: &ContainerIdentifier,
        reference: &str,
        pose: &Pose,
    ) -> JuizResult<PoseStamped>;
}

type RegistrySlot = Arc<RwLock<Option<Weak<dyn TransformationsRegistry>>>>;

/// Container ごとの PoseState と登録所への口の組 (§9.1)。
///
/// `ContainerImpl` が作り、`ContainerPtr` が同じものを持つ (clone しても同じ PoseState・同じ登録所を指す)。
/// どちらのロックも Container の利用者の状態のロックとは別なので、ContainerProcess の実行中でも読み書きできる。
#[derive(Clone)]
pub struct ContainerPoseHandle {
    identifier: ContainerIdentifier,
    state: PoseStatePtr,
    registry: RegistrySlot,
}

impl std::fmt::Debug for ContainerPoseHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerPoseHandle")
            .field("identifier", &self.identifier)
            .field("attached", &self.is_attached())
            .finish()
    }
}

impl ContainerPoseHandle {
    pub fn new(identifier: ContainerIdentifier) -> Self {
        Self {
            identifier,
            state: PoseState::new_ptr(),
            registry: Arc::new(RwLock::new(None)),
        }
    }

    pub fn identifier(&self) -> &ContainerIdentifier {
        &self.identifier
    }

    /// この Container の PoseState。登録所が Anchor の更新で書く
    pub fn state(&self) -> PoseStatePtr {
        self.state.clone()
    }

    /// PoseState の、Anchor `anchor_id` の項目の写し (登録所に問い合わせない)
    pub fn entry(&self, anchor_id: &str) -> JuizResult<Option<PoseEntry>> {
        Ok(self.read_state()?.get(anchor_id).copied())
    }

    fn read_state(&self) -> JuizResult<std::sync::RwLockReadGuard<'_, PoseState>> {
        self.state.read().map_err(|_| {
            anyhow!(JuizError::ObjectLockError {
                target: "PoseState".to_owned()
            })
        })
    }

    /// System に登録されたときに、登録所への口をつなぐ (§9.1)。
    /// 登録所が Container を持つので、循環しないよう弱い参照で持つ
    pub fn attach(&self, registry: Weak<dyn TransformationsRegistry>) -> JuizResult<()> {
        *self.write_slot()? = Some(registry);
        Ok(())
    }

    /// 登録所から外す (Container の削除・System の終了)。PoseState も空にする
    pub fn detach(&self) -> JuizResult<()> {
        *self.write_slot()? = None;
        if let Ok(mut s) = self.state.write() {
            s.clear();
        }
        Ok(())
    }

    /// 登録所につながっていて、登録所がまだ生きているか
    pub fn is_attached(&self) -> bool {
        self.registry
            .read()
            .map(|r| r.as_ref().and_then(|w| w.upgrade()).is_some())
            .unwrap_or(false)
    }

    fn write_slot(
        &self,
    ) -> JuizResult<std::sync::RwLockWriteGuard<'_, Option<Weak<dyn TransformationsRegistry>>>>
    {
        self.registry.write().map_err(|_| {
            anyhow!(JuizError::ObjectLockError {
                target: "ContainerPoseHandle".to_owned()
            })
        })
    }

    fn registry(&self) -> JuizResult<Arc<dyn TransformationsRegistry>> {
        let slot = self.registry.read().map_err(|_| {
            anyhow!(JuizError::ObjectLockError {
                target: "ContainerPoseHandle".to_owned()
            })
        })?;
        slot.as_ref().and_then(|w| w.upgrade()).ok_or_else(|| {
            anyhow!(JuizError::TransformationsNotAttachedError {
                container: self.identifier.to_string()
            })
        })
    }

    /// 基準 `reference` (ID か名前) から見たこの Container の ContainerPose (§7.1)
    pub fn pose(&self, reference: &str) -> JuizResult<PoseStamped> {
        // 登録所の Arc は呼び出しの間だけ持つ (読み出しロックは upgrade の間だけ)
        let registry = self.registry()?;
        registry.pose(&self.identifier, reference)
    }

    /// この Container を、基準 `reference` から見て `pose` の位置姿勢にする (§7.2)。
    /// 値は検査・正規化してから登録所に渡す (`InvalidPoseValueError`)
    pub fn set_pose(&self, reference: &str, pose: &Pose) -> JuizResult<PoseStamped> {
        let pose = pose.normalized()?;
        let registry = self.registry()?;
        registry.set_pose(&self.identifier, reference, &pose)
    }

    /// 読み出しだけの口 (§14)
    pub fn reader(&self) -> ContainerPoseReader {
        ContainerPoseReader {
            inner: self.clone(),
        }
    }
}

/// ContainerPose の読み出しだけの口 (§14)。書き込みのメソッドを持たない。
/// 将来の readonly ContainerProcess (#377) や、他言語の読み出し用アクセサに渡す
#[derive(Clone, Debug)]
pub struct ContainerPoseReader {
    inner: ContainerPoseHandle,
}

impl ContainerPoseReader {
    pub fn identifier(&self) -> &ContainerIdentifier {
        self.inner.identifier()
    }

    /// 基準 `reference` から見たこの Container の ContainerPose (§7.1)
    pub fn pose(&self, reference: &str) -> JuizResult<PoseStamped> {
        self.inner.pose(reference)
    }

    /// PoseState の、Anchor `anchor_id` の項目の写し
    pub fn entry(&self, anchor_id: &str) -> JuizResult<Option<PoseEntry>> {
        self.inner.entry(anchor_id)
    }
}
