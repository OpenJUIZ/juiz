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
    ///
    /// ダウンキャストはこの答えを信じて unsafe なキャストをするので、juiz_sdk の外では上書きできない
    /// (引数の [`sealed::TypeInfoToken`] は juiz_sdk の外から名前で書けない、issue #613)。
    /// 次のように上書きしようとするとコンパイルできない (引数の数が合わない E0050)。
    ///
    /// ```compile_fail,E0050
    /// use juiz_sdk::containers::{Container, ContainerImpl, ContainerTypeInfo};
    /// use juiz_sdk::manifests::{ContainerIdentifier, ContainerProfile};
    /// use juiz_sdk::processes::ProcessPtr;
    /// use juiz_sdk::result::JuizResult;
    /// use juiz_sdk::transformations::ContainerPoseHandle;
    ///
    /// #[derive(Debug)]
    /// struct Fake(ContainerIdentifier, ContainerPoseHandle);
    /// impl std::fmt::Display for Fake {
    ///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "fake") }
    /// }
    /// impl Container for Fake {
    ///     fn identifier(&self) -> ContainerIdentifier { self.0.clone() }
    ///     fn profile(&self) -> JuizResult<ContainerProfile> { unimplemented!() }
    ///     fn process(&self, _: &String) -> Option<ProcessPtr> { None }
    ///     fn processes(&self) -> Vec<ProcessPtr> { vec![] }
    ///     fn register_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessPtr> { Ok(p) }
    ///     fn purge_process(&mut self, _: &String) -> JuizResult<()> { Ok(()) }
    ///     fn clear(&mut self) -> JuizResult<()> { Ok(()) }
    ///     fn pose_handle(&self) -> ContainerPoseHandle { self.1.clone() }
    ///     fn container_type_info(&self) -> ContainerTypeInfo {
    ///         ContainerTypeInfo::of::<ContainerImpl<String>>()
    ///     }
    /// }
    /// ```
    #[doc(hidden)]
    fn container_type_info(&self, _: sealed::TypeInfoToken) -> ContainerTypeInfo {
        ContainerTypeInfo {
            type_name: std::any::type_name::<Self>(),
            sdk_version: env!("CARGO_PKG_VERSION"),
            size: std::mem::size_of_val(self),
            align: std::mem::align_of_val(self),
        }
    }
}

/// [`Container::container_type_info`] を juiz_sdk の外で上書きさせないための型 (issue #613)。
/// モジュールが crate 内だけなので、juiz_sdk の外からは名前で書けない。
pub(crate) mod sealed {
    #[derive(Debug, Clone, Copy)]
    pub struct TypeInfoToken;
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
