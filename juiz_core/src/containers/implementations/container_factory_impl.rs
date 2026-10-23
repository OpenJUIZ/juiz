use std::sync::Arc;

use juiz_sdk::manifests::ContainerProfile;

use crate::prelude::*;

pub type ContainerConstructor = dyn Fn(ContainerManifest, CapsuleMap) -> JuizResult<ContainerPtr>;
#[repr(C)]
pub struct ContainerFactoryImpl {
    core: ObjectCore,
    manifest: ContainerManifest,
    // constructor: Arc<ContainerConstructFunctionTrait<T>>,
    binded_container_constructor: Arc<ContainerConstructor>,
}

impl ContainerFactoryImpl {
    pub fn new(
        manifest: ContainerManifest,
        constructor: Arc<ContainerConstructor>,
    ) -> JuizResult<Self> {
        log::trace!("new({manifest:?}, constructor) called");
        Ok(ContainerFactoryImpl {
            core: ObjectCore::create_factory(
                JuizObjectClass::ContainerFactory("ContainerFactoryImpl"),
                manifest.type_name.clone().as_str(),
            ),
            manifest,
            // constructor: Arc::new(constructor),
            binded_container_constructor: constructor,
        })
    }

    // fn apply_default_manifest(&self, manifest: Value) -> Result<Value, JuizError> {
    //     let mut new_manifest = self.manifest.clone();
    //     for (k, v) in manifest.as_object().unwrap().iter() {
    //         new_manifest.as_object_mut().unwrap().insert(k.to_owned(), v.clone());
    //     }
    //     return Ok(new_manifest);
    // }
}

impl JuizObjectCoreHolder for ContainerFactoryImpl {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for ContainerFactoryImpl {
    fn profile_full(&self) -> JuizResult<Value> {
        let mut v = self.core.profile_full()?;
        let vv = self
            .manifest
            .arguments
            .iter()
            .map(|v| serde_json::to_value(v))
            .collect::<serde_json::Result<Vec<Value>>>()?;
        obj_merge_mut(
            &mut v,
            &jvalue!({
                "arguments": vv,
                "language": self.manifest.language,
            }),
        )?;
        Ok(v)
    }
}

impl ContainerFactory for ContainerFactoryImpl {
    fn create_container(
        &self,
        _core_worker: &mut CoreWorker,
        name: String,
        mut args: CapsuleMap,
    ) -> JuizResult<ContainerPtr> {
        log::trace!(
            "ContainerFactoryImpl::create_container(manifest={:?}) called",
            args
        );
        // 引数に name が無いとき (CLI の --container 起動など) は、
        // 呼び出し側が決めたインスタンス名を使う (issue #46)
        if args.get("name").is_err() {
            args.insert("name".to_owned(), jvalue!(name).into());
        }
        //println!("create_container");
        // ここでデフォルトを与えていく
        // println!("manifest========{:?}", self.manifest);
        for arg in self.manifest.arguments.iter() {
            match args.get(arg.name.as_str()) {
                Ok(_) => {}
                Err(_) => {
                    println!("set default {arg:?}");
                    args.insert(arg.name.clone(), arg.default.clone().into());
                }
            }
        }
        let mut manifest = self.manifest.clone();
        let name_cap = args.get("name")?;

        let name_str = name_cap
            .lock_as_value(|v| -> JuizResult<String> { Ok(v.as_str().unwrap().to_owned()) })??;
        manifest = manifest.name(name_str.as_str());
        // conf で解決したネームスペース (system_builder::namespaces) があれば ID に入れる
        if let Ok(namespace_cap) = args.get("namespace") {
            let namespace = namespace_cap.lock_as_value(|v| -> JuizResult<String> {
                Ok(v.as_str()
                    .ok_or_else(|| {
                        juiz_sdk::anyhow::anyhow!(JuizError::InvalidValueError {
                            message: format!("'namespace' must be a string, but {v}")
                        })
                    })?
                    .to_owned())
            })??;
            manifest = manifest.namespace(namespace.as_str());
        }
        // ファクトリのマニフェストの processes は、この Container 型で使える ContainerProcess の
        // 型の宣言 (Component のマニフェストでは name を持たない) で、インスタンスではない。
        // ContainerProcess のインスタンスは Container を作った後に別途作って登録するので、
        // 作成する Container のマニフェストには引き継がない。
        manifest.processes.clear();
        (self.binded_container_constructor)(manifest, args)
    }

    fn destroy_container(&mut self, c: ContainerPtr) -> JuizResult<ContainerProfile> {
        // todo!()
        log::trace!("ContainerFractoryImpl::destroy_container() called");
        c.lock()?.profile()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Component から読み込んだ Container のマニフェストのように、processes に名前の無い
    /// ContainerProcess の宣言を持つファクトリでも、Container を作れる (issue #29)
    #[test]
    fn create_container_with_unnamed_process_declarations() -> JuizResult<()> {
        let manifest = ContainerManifest::new("counter")
            .add_int_arg("initial", "", 0)
            .add_process(ProcessManifest::new("counter_get"))
            .add_process(ProcessManifest::new("counter_increment"));
        let factory = ContainerFactoryImpl::new(
            manifest,
            Arc::new(
                |cm: ContainerManifest, args: CapsuleMap| -> JuizResult<ContainerPtr> {
                    let value = args.get_int("initial")?;
                    Ok(ContainerPtr::new(ContainerImpl::new(cm, Box::new(value))?))
                },
            ),
        )?;
        let mut worker = CoreWorker::new(uuid::Uuid::new_v4(), jvalue!({}));
        let args: CapsuleMap = jvalue!({"name": "counter0", "initial": 100}).try_into()?;
        let container = factory.create_container(&mut worker, "counter0".to_owned(), args)?;
        let profile = container.lock()?.profile()?;
        assert_eq!(profile.name, "counter0");
        assert_eq!(profile.type_name, "counter");
        // ContainerProcess の宣言はインスタンスのプロファイルには入らない
        assert!(profile.processes.is_empty());
        Ok(())
    }

    /// CLI の `--container` 起動のように引数に name を含めずに呼んでも、
    /// 渡された名前で Container を作れる (issue #46)
    #[test]
    fn create_container_without_name_in_args() -> JuizResult<()> {
        let manifest = ContainerManifest::new("counter").add_int_arg("initial", "", 0);
        let factory = ContainerFactoryImpl::new(
            manifest,
            Arc::new(
                |cm: ContainerManifest, args: CapsuleMap| -> JuizResult<ContainerPtr> {
                    let value = args.get_int("initial")?;
                    Ok(ContainerPtr::new(ContainerImpl::new(cm, Box::new(value))?))
                },
            ),
        )?;
        let mut worker = CoreWorker::new(uuid::Uuid::new_v4(), jvalue!({}));
        let container =
            factory.create_container(&mut worker, "counter0".to_owned(), CapsuleMap::new())?;
        let profile = container.lock()?.profile()?;
        assert_eq!(profile.name, "counter0");
        assert_eq!(profile.type_name, "counter");
        Ok(())
    }
}
