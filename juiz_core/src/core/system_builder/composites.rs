//! conf の `composites:` と `composite_containers:` を読む (issue #169)
//!
//! 仕様は docs/spec/composition.md の §3.1・§3.3・§7.1。
//!
//! ```yaml
//! composites:
//!   - type_name: inc_twice
//!     body:
//!       process: increment_process
//!       args:
//!         arg1: { process: increment_process }
//! composite_containers:
//!   - name: cc0
//!     slots:
//!       counter: container0
//!     processes:
//!       - { type_name: add_and_get, name: ag0 }
//! ```

use std::collections::BTreeMap;

use juiz_sdk::{
    anyhow::{anyhow, Context},
    manifests::CompositeManifest,
};

use crate::{core::system_builder::containers::setup_container_processes, prelude::*};

/// `composites:` の定義を配列の順に Factory として登録する。
/// `containers` (スロット) を持つものは ContainerProcessFactory、持たないものは ProcessFactory になる。
pub(super) fn setup_composites(system: &System, manifest: &Value) -> JuizResult<()> {
    log::trace!("【呼出】setup_composites({manifest})");
    for (i, v) in get_array(manifest)?.iter().enumerate() {
        let composite: CompositeManifest = serde_json::from_value(v.clone()).map_err(|e| {
            anyhow!(JuizError::InvalidSettingError {
                message: format!("composites[{i}] is invalid: {e} ({v})"),
            })
        })?;
        let type_name = composite.type_name.clone();
        let mut cb = system.core_broker().lock_mut()?;
        let worker = cb.worker_mut();
        if composite.containers.is_empty() {
            worker.register_composite(&composite).map(|_| ())
        } else {
            worker.register_composite_container(&composite).map(|_| ())
        }
        .with_context(|| format!("registering composites[{i}] (type_name='{type_name}')"))?;
        log::info!("【作成】Composite (type_name={type_name})");
    }
    log::trace!("【終了】setup_composites()");
    Ok(())
}

/// `composite_containers:` の CompositeContainer と、その上の CompositeContainerProcess を作る。
/// 参照先の Container が出来てから呼ぶ。
pub(super) fn setup_composite_containers(system: &System, manifest: &Value) -> JuizResult<()> {
    log::trace!("【呼出】setup_composite_containers({manifest})");
    for (i, v) in get_array(manifest)?.iter().enumerate() {
        setup_composite_container(system, v)
            .with_context(|| format!("setting up composite_containers[{i}] ({v})"))?;
    }
    log::trace!("【終了】setup_composite_containers()");
    Ok(())
}

fn invalid(message: String) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::InvalidSettingError { message })
}

fn setup_composite_container(system: &System, v: &Value) -> JuizResult<()> {
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| invalid("composite_containers needs 'name' (string)".to_owned()))?;
    // namespace は resolve_namespaces で解決済み
    let namespace = v.get("namespace").and_then(|n| n.as_str()).unwrap_or("");
    let slots = v
        .get("slots")
        .and_then(|s| s.as_object())
        .ok_or_else(|| invalid("composite_containers needs 'slots' (object)".to_owned()))?;
    let mut references = BTreeMap::new();
    for (slot, reference) in slots.iter() {
        references.insert(slot.clone(), slot_reference(slot, reference)?);
    }
    let container = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .create_composite_container(name, namespace, &references)?;

    let values = v.get("processes").and_then(|p| p.as_array());
    let process_manifests = match values {
        None => Vec::new(),
        Some(values) => values
            .iter()
            .map(|p| ProcessManifest::try_from(p.clone()))
            .collect::<JuizResult<Vec<ProcessManifest>>>()?,
    };
    setup_container_processes(system, container, &process_manifests, values)
}

/// スロットの参照先を `create_composite_container` に渡す文字列にする。
/// 名前か ID の文字列、または `{ name, namespace }`。
/// `namespace` は相対なら CompositeContainer のネームスペースから、先頭 `/` なら絶対。
fn slot_reference(slot: &str, reference: &Value) -> JuizResult<String> {
    if let Some(s) = reference.as_str() {
        return Ok(s.to_owned());
    }
    let wrong = || {
        invalid(format!(
            "slot '{slot}' must be a name, an identifier or {{name, namespace}}, but {reference}"
        ))
    };
    let obj = reference.as_object().ok_or_else(wrong)?;
    if obj.keys().any(|k| k != "name" && k != "namespace") {
        return Err(wrong());
    }
    let name = obj.get("name").and_then(|n| n.as_str()).ok_or_else(wrong)?;
    match obj.get("namespace") {
        None => Ok(name.to_owned()),
        Some(ns) => {
            let ns = ns.as_str().ok_or_else(wrong)?;
            if ns.is_empty() {
                Ok(name.to_owned())
            } else if ns == "/" {
                Ok(format!("/{name}"))
            } else {
                Ok(format!("{ns}/{name}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_reference_forms() {
        assert_eq!(slot_reference("s", &jvalue!("c0")).unwrap(), "c0");
        assert_eq!(
            slot_reference("s", &jvalue!("core://core/container/c0::t")).unwrap(),
            "core://core/container/c0::t"
        );
        assert_eq!(slot_reference("s", &jvalue!({"name": "c0"})).unwrap(), "c0");
        assert_eq!(
            slot_reference("s", &jvalue!({"name": "c0", "namespace": "arm"})).unwrap(),
            "arm/c0"
        );
        assert_eq!(
            slot_reference("s", &jvalue!({"name": "c0", "namespace": "/robot2"})).unwrap(),
            "/robot2/c0"
        );
        assert_eq!(
            slot_reference("s", &jvalue!({"name": "c0", "namespace": "/"})).unwrap(),
            "/c0"
        );
        assert!(slot_reference("s", &jvalue!(1)).is_err());
        assert!(slot_reference("s", &jvalue!({"namespace": "arm"})).is_err());
        assert!(slot_reference("s", &jvalue!({"name": "c0", "type_name": "t"})).is_err());
    }
}
