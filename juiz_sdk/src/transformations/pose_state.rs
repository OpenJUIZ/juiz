use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use crate::geometry::{Pose, Time};

/// ある Anchor の木での、Container の値 (§6.1)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseEntry {
    /// pose_A(C): Anchor A から見た Container C の位置姿勢
    pub pose: Pose,
    /// 計算・書き込みの時刻
    pub stamp: Time,
    /// 版番号。この PoseState の中で書くたびに増える (§6.2 の 2、変わっていない部分木を飛ばすのに使う)
    pub version: u64,
}

/// Container が持つ「Anchor の ID → [`PoseEntry`]」の表 (§6.1)。
///
/// 書くのは core の Transformations 登録所 (Anchor の更新・`set_pose`)。
/// ロックは値の読み書きの間だけ持つ (§9.2)。
#[derive(Debug, Default, Clone)]
pub struct PoseState {
    entries: BTreeMap<String, PoseEntry>,
    last_version: u64,
}

/// Container と ContainerPtr から同じものを指す PoseState (§9.1)
pub type PoseStatePtr = Arc<RwLock<PoseState>>;

impl PoseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_ptr() -> PoseStatePtr {
        Arc::new(RwLock::new(Self::new()))
    }

    /// Anchor `anchor_id` の木での値
    pub fn get(&self, anchor_id: &str) -> Option<&PoseEntry> {
        self.entries.get(anchor_id)
    }

    /// Anchor `anchor_id` の木での値を書き、付けた版番号を返す。
    /// 値は正規化済みであること (登録所が正規化してから書く)
    pub fn set(&mut self, anchor_id: &str, pose: Pose, stamp: Time) -> u64 {
        self.last_version += 1;
        let version = self.last_version;
        self.entries.insert(
            anchor_id.to_owned(),
            PoseEntry {
                pose,
                stamp,
                version,
            },
        );
        version
    }

    /// Anchor `anchor_id` の項目を消す (木から外れた・Anchor が消えた、§6.2 の 3・§10)
    pub fn remove(&mut self, anchor_id: &str) -> Option<PoseEntry> {
        self.entries.remove(anchor_id)
    }

    /// 値を持っている Anchor の ID (文字列順)
    pub fn anchors(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &PoseEntry)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 最後に付けた版番号 (まだ書いていなければ 0)
    pub fn last_version(&self) -> u64 {
        self.last_version
    }
}
