//! conf の `transforms:` と `pose_listeners:` (docs/spec/transformations.md §11)

use juiz_sdk::anyhow::Context;
use juiz_sdk::manifests::{PoseListenerManifest, TransformManifest};

use crate::prelude::*;

/// `transforms:` をすべて登録してから木を検査し (§5.2)、続けて `pose_listeners:` を登録する。
/// 名前は System の既定ネームスペース (解決済みの manifest の `namespace`) から解決する
pub(super) fn setup_transformations(system: &System, manifest: &Value) -> JuizResult<()> {
    let namespace = manifest
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    // 終点が別 System の Container のものは、subsystems / mastersystem とつないだ後で回す (§19.2 の 8)
    let transforms = TransformManifest::list_from_conf(manifest)
        .context("reading 'transforms' in conf")?
        .into_iter()
        .filter(|t| !is_forwarded(t))
        .collect::<Vec<TransformManifest>>();
    let listeners = PoseListenerManifest::list_from_conf(manifest)
        .context("reading 'pose_listeners' in conf")?;
    if transforms.is_empty() && listeners.is_empty() {
        return Ok(());
    }
    let mut cb = system.core_broker().lock_mut()?;
    let worker = cb.worker_mut();
    if !transforms.is_empty() {
        worker
            .store()
            .transformations
            .create_transforms(&transforms, &namespace)
            .map_err(|e| {
                log::error!("loading 'transforms' in conf failed: {e}");
                e
            })
            .context("loading 'transforms' in conf")?;
    }
    for l in listeners.into_iter() {
        let desc = format!("{l:?}");
        worker
            .create_pose_listener(l, &namespace)
            .map_err(|e| {
                log::error!("loading 'pose_listeners' ({desc}) in conf failed: {e}");
                e
            })
            .with_context(|| format!("loading 'pose_listeners' ({desc}) in conf"))?;
    }
    Ok(())
}

/// 終点が別 System の Container (ブローカー付きの ID) の Transform (§19.2 の 5)
fn is_forwarded(t: &TransformManifest) -> bool {
    t.source_system.is_none() && crate::brokers::remote_container_id(&t.destination).is_some()
}

/// conf の `transforms:` のうち、終点が別 System の Container のものを、終点の System に回して作る (§19.2 の 5・8)。
/// `subsystems:` / `mastersystem:` とのつなぎが済んだ後に呼ぶ。失敗は conf の読み込みエラー (§11)
pub(super) fn setup_forwarded_transforms(system: &System, manifest: &Value) -> JuizResult<()> {
    let namespace = manifest
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let transforms = TransformManifest::list_from_conf(manifest)
        .context("reading 'transforms' in conf")?
        .into_iter()
        .filter(is_forwarded)
        .collect::<Vec<TransformManifest>>();
    if transforms.is_empty() {
        return Ok(());
    }
    let cb = system.core_broker().lock()?;
    for t in transforms.iter() {
        let desc = format!("{} -> {}", t.source, t.destination);
        cb.transform_create_in(t, &namespace)
            .map_err(|e| {
                log::error!("loading 'transforms' ({desc}) in conf failed: {e:#}");
                e
            })
            .with_context(|| format!("loading 'transforms' ({desc}) in conf"))?;
    }
    Ok(())
}

/// System の起動の最後に Periodic Anchor のスレッドを始める (§4.3)
pub(crate) fn start_transformations(system: &System) -> JuizResult<()> {
    system
        .core_broker()
        .lock()?
        .worker()
        .store()
        .transformations
        .start()
}
