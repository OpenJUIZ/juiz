use std::fmt::{Debug, Display};

use crate::{manifests::ContainerProfile, prelude::*, processes::ProcessPtr};
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

    /// 実装の型名。プラグイン(共有ライブラリ)をまたいで TypeId が一致しないときのダウンキャストに使う。
    fn container_type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

mopafy!(Container);
