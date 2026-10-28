//! CompositeContainer と、スロットを持つ Composite の Factory (issue #168)
//!
//! 仕様は `docs/spec/composition.md` の §7。
//! CompositeContainer は `ContainerImpl<CompositeSlots>` で、スロット名 → 既存の Container の参照を持つ。
//! CompositeContainerProcess の本体は、CompositeContainer をロックしてスロットを複製したらすぐ放し、
//! そのあと式の木の順に子を呼ぶ。子の ContainerProcess は自分の Container を自分でロックする。

use std::collections::BTreeMap;
use std::sync::Arc;

use juiz_sdk::anyhow::anyhow;
use juiz_sdk::manifests::{CompositeManifest, ProcessProfile, COMPOSITE_CONTAINER_TYPE_NAME};

use crate::connections::ConnectionFactoryImpl;
use crate::containers::{BindedContainerFunctionType, ContainerImpl};
use crate::prelude::*;
use crate::processes::{
    composite_invalid, CompositeBody, CompositeChildFactory, CompositeSlotMap, ProcessImpl,
};

/// CompositeContainer の中身。スロット名 → 参照先の Container
pub struct CompositeSlots {
    slots: CompositeSlotMap,
}

impl CompositeSlots {
    pub fn new(slots: BTreeMap<String, ContainerPtr>) -> Self {
        CompositeSlots { slots }
    }

    pub fn slots(&self) -> &BTreeMap<String, ContainerPtr> {
        &self.slots
    }

    /// `id` の Container を参照しているか
    pub fn refers_to(&self, id: &ContainerIdentifier) -> bool {
        self.slots.values().any(|c| &c.identifier() == id)
    }
}

/// CompositeContainer の実体の型
pub type CompositeContainer = ContainerImpl<CompositeSlots>;

/// CompositeContainer を作る。`slots` の参照先は解決済みの Container
///
/// profile の `slots` には、スロット名 → 参照先の Container の ID を載せる。
pub fn composite_container_new(
    name: &str,
    namespace: &str,
    slots: BTreeMap<String, ContainerPtr>,
) -> JuizResult<ContainerPtr> {
    for slot in slots.keys() {
        juiz_sdk::identifier::validate_name(slot)?;
    }
    let manifest = ContainerManifest::new(COMPOSITE_CONTAINER_TYPE_NAME)
        .name(name)
        .namespace(namespace)
        .factory(COMPOSITE_CONTAINER_TYPE_NAME);
    let slot_ids = slots
        .iter()
        .map(|(k, c)| (k.clone(), c.identifier().into()))
        .collect::<BTreeMap<String, String>>();
    let mut container = CompositeContainer::new(manifest, Box::new(CompositeSlots::new(slots)))?;
    container.profile_mut().slots = Some(slot_ids);
    Ok(ContainerPtr::new(container))
}

/// CompositeContainer をロックしてスロットを複製し、すぐにロックを放す (仕様 §7.3)
fn clone_slots(container: &ContainerPtr) -> JuizResult<CompositeSlotMap> {
    let guard = container.lock()?;
    match guard.downcast_ref::<CompositeContainer>() {
        Some(c) => Ok(c.slots().clone()),
        None => Err(anyhow!(JuizError::ContainerDowncastingError {
            identifier: container.identifier().to_string(),
        })),
    }
}

/// スロットを持つ Composite の Factory (仕様 §3.1、§7)
pub struct CompositeContainerProcessFactory {
    core: ObjectCore,
    manifest: ProcessManifest,
    /// スロット名 → Container の type_name (Composite の宣言)
    slot_types: BTreeMap<String, String>,
    body: Arc<CompositeBody>,
}

impl CompositeContainerProcessFactory {
    /// Composite の定義と子の Factory から Factory を作る
    ///
    /// `lookup` は子の type_name から、同じ juiz プロセスに登録された ProcessFactory か
    /// ContainerProcessFactory を返す。先に登録した、スロットを持たない Composite も子にできる。
    pub fn new<F>(composite: &CompositeManifest, lookup: F) -> JuizResult<Self>
    where
        F: Fn(&str) -> Option<CompositeChildFactory>,
    {
        if !composite.has_containers() {
            return Err(composite_invalid(
                &composite.type_name,
                "",
                "a composite without container slots must be registered as a composite process factory"
                    .to_owned(),
            ));
        }
        let (manifest, body) = CompositeBody::build(composite, &lookup)?;
        Ok(CompositeContainerProcessFactory {
            core: ObjectCore::create_factory(
                JuizObjectClass::ContainerProcessFactory("CompositeContainerProcessFactory"),
                manifest.type_name.clone(),
            ),
            slot_types: composite.containers.clone(),
            body: Arc::new(body),
            manifest,
        })
    }

    /// CompositeContainer に、この Composite が宣言したスロットがすべてあり、型が合うか (仕様 §7.1)
    fn check_slots(&self, container: &ContainerPtr) -> JuizResult<()> {
        let slots = clone_slots(container).map_err(|_| {
            composite_invalid(
                &self.manifest.type_name,
                "",
                format!(
                    "container '{}' is not a {COMPOSITE_CONTAINER_TYPE_NAME}",
                    container.identifier()
                ),
            )
        })?;
        for (slot, type_name) in self.slot_types.iter() {
            let c = slots.get(slot).ok_or_else(|| {
                composite_invalid(
                    &self.manifest.type_name,
                    "",
                    format!(
                        "container slot '{slot}' is not bound in '{}'",
                        container.identifier()
                    ),
                )
            })?;
            let actual = c.identifier().type_name;
            if &actual != type_name {
                return Err(composite_invalid(
                    &self.manifest.type_name,
                    "",
                    format!(
                        "container slot '{slot}' needs '{type_name}' but '{}' is bound in '{}'",
                        c.identifier(),
                        container.identifier()
                    ),
                ));
            }
        }
        Ok(())
    }

    fn function(&self, trace: bool) -> BindedContainerFunctionType {
        let body = self.body.clone();
        Arc::new(move |container: ContainerPtr, args: CapsuleMap| {
            let slots = clone_slots(&container)?;
            body.call(&slots, &args, trace)
        })
    }
}

impl std::fmt::Debug for CompositeContainerProcessFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeContainerProcessFactory")
            .field("core", &self.core)
            .field("manifest", &self.manifest)
            .field("slot_types", &self.slot_types)
            .finish()
    }
}

impl JuizObjectCoreHolder for CompositeContainerProcessFactory {
    fn core(&self) -> &ObjectCore {
        &self.core
    }
}

impl JuizObject for CompositeContainerProcessFactory {
    fn profile_full(&self) -> JuizResult<Value> {
        let mut v = self.core.profile_full()?;
        let arguments = self
            .manifest
            .arguments
            .iter()
            .map(|v| serde_json::to_value(v))
            .collect::<serde_json::Result<Vec<Value>>>()?;
        obj_merge_mut(
            &mut v,
            &jvalue!({
                "arguments": arguments,
                "language": self.manifest.language,
                "container_type": self.manifest.container_type,
                "composition": serde_json::to_value(&self.manifest.composition)?,
            }),
        )?;
        Ok(v)
    }
}

impl ContainerProcessFactory for CompositeContainerProcessFactory {
    fn create_container_process(
        &self,
        container: ContainerPtr,
        manifest: &ProcessManifest,
    ) -> JuizResult<ProcessPtr> {
        log::trace!(
            "CompositeContainerProcessFactory::create_container_process(manifest={manifest}) called"
        );
        self.check_slots(&container)?;
        let trace = manifest.trace;
        let mut instance_manifest = self.manifest.build_instance_manifest(manifest.clone())?;
        instance_manifest.language = self.manifest.language.clone();
        instance_manifest.factory = self.manifest.factory.clone();
        // memo は Composite のインスタンス単位 (仕様 §5.3)
        instance_manifest.use_memo = manifest.use_memo;
        let function = self.function(trace);
        Ok(ProcessPtr::new(
            ProcessImpl::new_from_clousure_and_class_name(
                JuizObjectClass::ContainerProcess("CompositeContainerProcessImpl"),
                instance_manifest,
                move |args| function(container.clone(), args),
                Box::new(ConnectionFactoryImpl::new()),
            )?,
        ))
    }

    fn destroy_container_process(&mut self, proc: ProcessPtr) -> JuizResult<ProcessProfile> {
        // ContainerProcessFactoryImpl と同じく、CompositeContainer への参照はプロセスのクロージャが持つ
        proc.lock()?.profile()
    }

    fn manifest(&self) -> JuizResult<ProcessManifest> {
        Ok(self.manifest.clone())
    }

    fn body(&self) -> JuizResult<BindedContainerFunctionType> {
        Ok(self.function(false))
    }
}
