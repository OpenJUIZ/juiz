//! Anchor = 特別な Container (docs/spec/transformations.md §4.1)

use juiz_sdk::manifests::AnchorManifest;

use crate::prelude::*;

/// Anchor の中身。利用者の状態は持たず、更新方式と (Periodic なら) 周期だけを持つ
#[derive(Debug, Clone, Copy)]
pub struct AnchorState {
    manifest: AnchorManifest,
}

impl AnchorState {
    pub fn manifest(&self) -> &AnchorManifest {
        &self.manifest
    }
}

/// Anchor の実体の型
pub type AnchorContainer = ContainerImpl<AnchorState>;

/// Anchor を作る。profile には `anchor: {"kind", "rate"}` を載せる (§4.1)
pub fn anchor_container_new(
    name: &str,
    namespace: &str,
    manifest: AnchorManifest,
) -> JuizResult<ContainerPtr> {
    let type_name = manifest.kind.type_name();
    let mut container_manifest = ContainerManifest::new(type_name)
        .name(name)
        .namespace(namespace)
        .factory(type_name);
    container_manifest.rate = manifest.rate;
    let mut container =
        AnchorContainer::new(container_manifest, Box::new(AnchorState { manifest }))?;
    container.profile_mut().anchor = Some(manifest.to_profile_value());
    Ok(ContainerPtr::new(container))
}
