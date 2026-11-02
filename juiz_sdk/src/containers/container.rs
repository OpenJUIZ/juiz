use std::fmt::{Debug, Display};

use crate::{
    manifests::ContainerProfile, prelude::*, processes::ProcessPtr,
    transformations::ContainerPoseHandle,
};
use mopa::mopafy;

pub trait Container: Display + Debug + mopa::Any {
    fn identifier(&self) -> ContainerIdentifier;

    fn profile(&self) -> JuizResult<ContainerProfile>;

    // fn manifest(&self) -> &ContainerManifest;

    fn process(&self, name_or_id: &String) -> Option<ProcessPtr>;

    fn processes(&self) -> Vec<ProcessPtr>;

    fn register_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessPtr>;

    fn purge_process(&mut self, name_or_id: &String) -> JuizResult<()>;

    fn clear(&mut self) -> JuizResult<()>;

    /// ContainerPose の口 (PoseState と Transformations 登録所への口、docs/spec/transformations.md §9.1)。
    /// 利用者の状態とは別のロックで、clone しても同じものを指す。`ContainerPtr` は作られたときにこれを受け取る
    fn pose_handle(&self) -> ContainerPoseHandle;

    /// 実装の型の情報。プラグイン(共有ライブラリ)をまたいで TypeId が一致しないときのダウンキャストに使う。
    fn container_type_info(&self) -> ContainerTypeInfo {
        ContainerTypeInfo {
            type_name: std::any::type_name::<Self>(),
            sdk_version: env!("CARGO_PKG_VERSION"),
            size: std::mem::size_of_val(self),
            align: std::mem::align_of_val(self),
        }
    }
}

/// コンテナの型の情報。型名、ビルドに使った juiz_sdk のバージョン、型のサイズとアラインメント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerTypeInfo {
    pub type_name: &'static str,
    pub sdk_version: &'static str,
    pub size: usize,
    pub align: usize,
}

impl ContainerTypeInfo {
    pub fn of<T: 'static>() -> Self {
        ContainerTypeInfo {
            type_name: std::any::type_name::<T>(),
            sdk_version: env!("CARGO_PKG_VERSION"),
            size: std::mem::size_of::<T>(),
            align: std::mem::align_of::<T>(),
        }
    }
}

mopafy!(Container);
