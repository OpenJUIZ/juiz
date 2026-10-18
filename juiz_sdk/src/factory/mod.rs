//! juiz関連オブジェクトのfactoryに関する機能パッケージ
//!
//!

use crate::{
    containers::{ContainerImpl, ContainerTypeInfo},
    prelude::*,
};
use anyhow::anyhow;
use std::sync::Arc;

pub struct ProcessFactoryStruct(
    pub ProcessManifest,
    pub fn(CapsuleMap) -> JuizResult<Capsule>,
);

pub fn process_factory(
    manifest: ProcessManifest,
    func: fn(CapsuleMap) -> JuizResult<Capsule>,
) -> ProcessFactoryStruct {
    ProcessFactoryStruct(manifest, func)
}

pub struct ContainerFactoryStruct(
    pub ContainerManifest,
    pub Arc<dyn Fn(ContainerManifest, CapsuleMap) -> JuizResult<ContainerPtr> + 'static>,
);

pub fn container_factory<T: 'static>(
    manifest: ContainerManifest,
    function: impl Fn(CapsuleMap) -> JuizResult<Box<T>> + 'static,
) -> ContainerFactoryStruct {
    ContainerFactoryStruct(manifest, Arc::new(bind_container_constructor(function)))
}

pub fn bind_container_constructor<T: 'static>(
    function: impl Fn(CapsuleMap) -> JuizResult<Box<T>>,
) -> impl Fn(ContainerManifest, CapsuleMap) -> JuizResult<ContainerPtr> {
    move |cn: ContainerManifest, v: CapsuleMap| -> JuizResult<ContainerPtr> {
        Ok(ContainerPtr::new(ContainerImpl::new(
            cn.clone(),
            function(v)?,
        )?))
    }
}

pub struct ContainerProcessFactoryStruct(
    pub ProcessManifest,
    pub Arc<dyn Fn(ContainerPtr, CapsuleMap) -> JuizResult<Capsule> + 'static>,
);

pub fn container_process_factory<T: 'static>(
    manifest: ProcessManifest,
    function: impl Fn(&mut ContainerImpl<T>, CapsuleMap) -> JuizResult<Capsule> + 'static,
) -> ContainerProcessFactoryStruct {
    ContainerProcessFactoryStruct(
        manifest,
        Arc::new(bind_container_process_function(function)),
    )
}

fn bind_container_process_function<T: 'static>(
    function: impl Fn(&mut ContainerImpl<T>, CapsuleMap) -> JuizResult<Capsule>,
) -> impl Fn(ContainerPtr, CapsuleMap) -> JuizResult<Capsule> {
    move |container_ptr: ContainerPtr, capmap: CapsuleMap| -> JuizResult<Capsule> {
        let mut container = container_ptr.lock_mut()?;
        let cn = downcast_container_mut::<T>(&mut *container)?;
        (function)(cn, capmap)
    }
}

/// コンテナを ContainerImpl<T> にダウンキャストする。
///
/// コンテナとコンテナプロセスは別々のプラグイン(共有ライブラリ)でビルドされるので、
/// 同じ型でも TypeId が一致しないことがある(Windows で起きる)。
/// そのときは型名が一致し、juiz_sdk のバージョンと型のサイズ・アラインメントも一致すれば
/// 同じ型とみなしてキャストする。型名だけが一致して他が食い違うときはエラーにする。
fn downcast_container_mut<T: 'static>(
    container: &mut dyn Container,
) -> JuizResult<&mut ContainerImpl<T>> {
    if container.is::<ContainerImpl<T>>() {
        return Ok(container.downcast_mut::<ContainerImpl<T>>().unwrap());
    }
    let expected = ContainerTypeInfo::of::<ContainerImpl<T>>();
    let actual = container.container_type_info();
    if actual.type_name != expected.type_name {
        return Err(anyhow!(JuizError::ContainerDowncastingError {
            identifier: "ContainerPTr".to_owned()
        }));
    }
    if actual != expected {
        return Err(anyhow!(JuizError::ContainerTypeMismatchError {
            expected: format!("{expected:?}"),
            actual: format!("{actual:?}"),
        }));
    }
    log::debug!("downcast container by type info ({expected:?})");
    // SAFETY: 型名、juiz_sdk のバージョン、サイズ、アラインメントが一致するので同じ型とみなす。
    Ok(unsafe { container.downcast_mut_unchecked::<ContainerImpl<T>>() })
}

pub struct ContainerStackFactoryStruct(
    pub ContainerManifest,
    pub Arc<dyn Fn(ContainerManifest, ContainerPtr) -> JuizResult<ContainerPtr> + 'static>,
);

pub fn container_stack_factory<T: 'static>(
    manifest: ContainerManifest,
    function: impl Fn(ContainerManifest, ContainerPtr) -> JuizResult<Box<T>> + 'static,
) -> ContainerStackFactoryStruct {
    ContainerStackFactoryStruct(
        manifest,
        Arc::new(bind_container_stack_constructor(function)),
    )
}

pub fn bind_container_stack_constructor<T: 'static>(
    function: impl Fn(ContainerManifest, ContainerPtr) -> JuizResult<Box<T>>,
) -> impl Fn(ContainerManifest, ContainerPtr) -> JuizResult<ContainerPtr> {
    move |cn: ContainerManifest, cp: ContainerPtr| -> JuizResult<ContainerPtr> {
        Ok(ContainerPtr::new(ContainerImpl::new(
            cn.clone(),
            function(cn, cp)?,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::ProcessPtr;
    use std::fmt::{Debug, Display};

    /// 別のプラグインでビルドされたコンテナの代わり。中身は ContainerImpl<i64> で、
    /// container_type_info が返す情報だけを差し替えられる。
    #[repr(transparent)]
    struct ForeignContainer(ContainerImpl<i64>);

    thread_local! {
        static REPORTED_INFO: std::cell::RefCell<Option<ContainerTypeInfo>> = Default::default();
    }

    impl Debug for ForeignContainer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Debug::fmt(&self.0, f)
        }
    }

    impl Display for ForeignContainer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            Display::fmt(&self.0, f)
        }
    }

    impl Container for ForeignContainer {
        fn identifier(&self) -> ContainerIdentifier {
            self.0.identifier()
        }
        fn profile(&self) -> JuizResult<crate::manifests::ContainerProfile> {
            self.0.profile()
        }
        fn process(&self, name_or_id: &String) -> Option<ProcessPtr> {
            self.0.process(name_or_id)
        }
        fn processes(&self) -> Vec<ProcessPtr> {
            self.0.processes()
        }
        fn register_process(&mut self, p: ProcessPtr) -> JuizResult<ProcessPtr> {
            self.0.register_process(p)
        }
        fn purge_process(&mut self, name_or_id: &String) -> JuizResult<()> {
            self.0.purge_process(name_or_id)
        }
        fn clear(&mut self) -> JuizResult<()> {
            self.0.clear()
        }
        fn container_type_info(&self) -> ContainerTypeInfo {
            REPORTED_INFO.with(|i| i.borrow().clone().unwrap())
        }
    }

    fn downcast_with(info: ContainerTypeInfo) -> JuizResult<i64> {
        REPORTED_INFO.with(|i| *i.borrow_mut() = Some(info));
        let mut c = ForeignContainer(
            ContainerImpl::new(ContainerManifest::new("i64").name("c0"), Box::new(42)).unwrap(),
        );
        Ok(*downcast_container_mut::<i64>(&mut c)?.t)
    }

    #[test]
    fn downcast_by_type_info_and_reject_mismatch() {
        let same = ContainerTypeInfo::of::<ContainerImpl<i64>>();
        assert_eq!(downcast_with(same.clone()).unwrap(), 42);

        let other_sdk = ContainerTypeInfo {
            sdk_version: "0.0.0",
            ..same.clone()
        };
        let err = downcast_with(other_sdk).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::ContainerTypeMismatchError { .. })
        ));

        let other_size = ContainerTypeInfo {
            size: same.size + 8,
            ..same.clone()
        };
        assert!(downcast_with(other_size).is_err());

        let other_type = ContainerTypeInfo {
            type_name: "other::Type",
            ..same
        };
        let err = downcast_with(other_type).unwrap_err();
        assert!(matches!(
            err.downcast_ref::<JuizError>(),
            Some(JuizError::ContainerDowncastingError { .. })
        ));
    }
}
