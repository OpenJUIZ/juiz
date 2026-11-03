//! Transformations の API (issue #370、仕様 docs/spec/transformations.md §13.1)
//!
//! | クラス / 関数 | 引数 | 戻り値 |
//! |---|---|---|
//! | `container/pose` (READ) | param `identifier`・`reference` | `PoseStamped` |
//! | `container/pose_state` (READ) | param `identifier`・`since`・`system` (§19.4) | `{identifier, system, version, changed, entries}` |
//! | `container/set_pose` (UPDATE) | param `identifier`、ボディ `reference`・`pose` | `PoseStamped` |
//! | `transform/list` (READ) | | ID の配列 |
//! | `transform/profile_full` (READ) | param `identifier` | profile |
//! | `transform/create` (CREATE) | ボディ `source`・`destination`・`translation`・`rotation` (`__manifest__` に包んでもよい) | profile |
//! | `transform/update` (UPDATE) | param `identifier`、ボディ `translation`・`rotation` (省けば今のまま) | profile |
//! | `transform/destroy` (DELETE) | param `identifier` | profile |
//! | `anchor/list` (READ) | | ID の配列 |
//! | `anchor/update` (UPDATE) | param `identifier` | `{identifier, stamp}` |
//! | `transformations/tree` (READ) | param `anchor` (`identifier` でもよい) | 木 |
//! | `pose_listener/list` (READ) | | ID の配列 |
//! | `pose_listener/profile_full` (READ) | param `identifier` | profile |
//! | `pose_listener/create` (CREATE) | ボディ `container`・`process`・`arg`・`reference`・`on_update` (`__manifest__` に包んでもよい) | profile |
//! | `pose_listener/destroy` (DELETE) | param `identifier` | profile |
//!
//! ボディの値は param で渡してもよい (`reference` など文字列のもの)。
use crate::brokers::TransformationsBrokerProxy;
use crate::transformations::api_types::{
    check_pose_value, check_quaternion_value, check_vector3_value, PoseStateQuery,
};
use juiz_sdk::{
    anyhow::anyhow,
    geometry::{Pose, Quaternion, Vector3},
    log,
    manifests::{PoseListenerManifest, TransformManifest},
    prelude::*,
};

use super::CallbackContainerType;

fn missing(key: &str) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
        key_name: key.to_owned()
    })
}

fn param(args: &CapsuleMap, key: &str) -> JuizResult<String> {
    args.get_param(key).cloned().ok_or_else(|| missing(key))
}

/// ボディの値。無ければ `None`
fn body_value(args: &CapsuleMap, key: &str) -> JuizResult<Option<Value>> {
    match args.get(key) {
        Ok(c) => Ok(Some(c.extract_value()?)),
        Err(_) => Ok(None),
    }
}

/// 文字列の引数。ボディか param
fn string_arg(args: &CapsuleMap, key: &str) -> JuizResult<Option<String>> {
    if let Some(v) = body_value(args, key)? {
        return match v {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s)),
            other => Err(anyhow!(JuizError::InvalidArgumentError {
                message: format!("'{key}' must be a string, but {other}"),
            })),
        };
    }
    Ok(args.get_param(key).cloned())
}

/// `__manifest__` があればそれ、無ければ `keys` のボディの値 (か param) を集めた辞書
fn manifest_value(args: &CapsuleMap, keys: &[&str]) -> JuizResult<Value> {
    if let Some(v) = body_value(args, "__manifest__")? {
        return Ok(v);
    }
    let mut map = serde_json::Map::new();
    for key in keys.iter() {
        if let Some(v) = body_value(args, key)? {
            map.insert((*key).to_owned(), v);
        } else if let Some(v) = args.get_param(key) {
            map.insert((*key).to_owned(), Value::String(v.clone()));
        }
    }
    Ok(Value::Object(map))
}

fn vector3_arg(args: &CapsuleMap, key: &str) -> JuizResult<Option<Vector3>> {
    match body_value(args, key)? {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            check_vector3_value(key, &v)?;
            Ok(Some(serde_json::from_value(v)?))
        }
    }
}

fn quaternion_arg(args: &CapsuleMap, key: &str) -> JuizResult<Option<Quaternion>> {
    match body_value(args, key)? {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            check_quaternion_value(key, &v)?;
            Ok(Some(serde_json::from_value(v)?))
        }
    }
}

fn invalid(what: &str, e: impl std::fmt::Display, v: &Value) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::InvalidArgumentError {
        message: format!("{what} is invalid: {e} ({v})"),
    })
}

/// `transform/create` の manifest。`translation`・`rotation` は省略可 (0 と回転なし)
pub(crate) fn transform_manifest_from_value(v: &Value) -> JuizResult<TransformManifest> {
    if let Some(t) = v.get("translation") {
        check_vector3_value("translation", t)?;
    }
    if let Some(r) = v.get("rotation") {
        check_quaternion_value("rotation", r)?;
    }
    TransformManifest::try_from(v)
}

pub(crate) fn container_read_callbacks(cbs: &mut CallbackContainerType) {
    cbs.insert("pose", |cb, args| {
        log::debug!("[READ  ] container/pose called");
        let id = param(&args, "identifier")?;
        let reference = string_arg(&args, "reference")?.ok_or_else(|| missing("reference"))?;
        Ok(serde_json::to_value(cb.lock()?.container_pose(&id, &reference)?)?.into())
    });
    cbs.insert("pose_state", |cb, args| {
        log::debug!("[READ  ] container/pose_state called");
        let mut query = PoseStateQuery::new(&param(&args, "identifier")?);
        if let Some(s) = string_arg(&args, "since")? {
            query.since = Some(s.parse::<u64>().map_err(|e| {
                anyhow!(JuizError::InvalidArgumentError {
                    message: format!("'since' must be an unsigned integer: {e} ({s})"),
                })
            })?);
        }
        query.system = string_arg(&args, "system")?;
        Ok(cb.lock()?.container_pose_state(&query)?.into())
    });
}

pub(crate) fn container_update_callbacks(cbs: &mut CallbackContainerType) {
    cbs.insert("set_pose", |cb, args| {
        log::debug!("[UPDATE] container/set_pose called");
        let id = param(&args, "identifier")?;
        let reference = string_arg(&args, "reference")?.ok_or_else(|| missing("reference"))?;
        let v = body_value(&args, "pose")?.ok_or_else(|| missing("pose"))?;
        check_pose_value(&v)?;
        let pose: Pose = serde_json::from_value(v.clone()).map_err(|e| invalid("pose", e, &v))?;
        Ok(serde_json::to_value(cb.lock()?.container_set_pose(&id, &reference, &pose)?)?.into())
    });
}

pub(crate) fn transform_create_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("create", |cb, args| {
        log::debug!("[CREATE] transform/create called");
        let v = manifest_value(
            &args,
            &[
                "source",
                "destination",
                "translation",
                "rotation",
                "rate",
                "timeout",
            ],
        )?;
        let manifest = transform_manifest_from_value(&v)?;
        Ok(cb.lock()?.transform_create(&manifest)?.into())
    });
    cbs
}

pub(crate) fn transform_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |cb, _args| {
        log::debug!("[READ  ] transform/list called");
        Ok(serde_json::to_value(cb.lock()?.transform_list()?)?.into())
    });
    cbs.insert("profile_full", |cb, args| {
        log::debug!("[READ  ] transform/profile_full called");
        Ok(cb
            .lock()?
            .transform_profile_full(&param(&args, "identifier")?)?
            .into())
    });
    cbs
}

pub(crate) fn transform_update_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("update", |cb, args| {
        log::debug!("[UPDATE] transform/update called");
        let id = param(&args, "identifier")?;
        let translation = vector3_arg(&args, "translation")?;
        let rotation = quaternion_arg(&args, "rotation")?;
        Ok(cb
            .lock()?
            .transform_update(&id, translation, rotation)?
            .into())
    });
    cbs
}

pub(crate) fn transform_delete_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("destroy", |cb, args| {
        log::debug!("[DELETE] transform/destroy called");
        Ok(cb
            .lock()?
            .transform_destroy(&param(&args, "identifier")?)?
            .into())
    });
    cbs
}

pub(crate) fn anchor_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |cb, _args| {
        log::debug!("[READ  ] anchor/list called");
        Ok(serde_json::to_value(cb.lock()?.anchor_list()?)?.into())
    });
    cbs
}

pub(crate) fn anchor_update_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("update", |cb, args| {
        log::debug!("[UPDATE] anchor/update called");
        Ok(cb
            .lock()?
            .anchor_update(&param(&args, "identifier")?)?
            .into())
    });
    cbs
}

pub(crate) fn transformations_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("tree", |cb, args| {
        log::debug!("[READ  ] transformations/tree called");
        let anchor = match string_arg(&args, "anchor")? {
            Some(a) => a,
            None => param(&args, "identifier").map_err(|_| missing("anchor"))?,
        };
        Ok(cb.lock()?.transformations_tree(&anchor)?.into())
    });
    cbs
}

pub(crate) fn pose_listener_create_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("create", |cb, args| {
        log::debug!("[CREATE] pose_listener/create called");
        let mut v = manifest_value(
            &args,
            &["container", "process", "arg", "reference", "on_update"],
        )?;
        // param で来た on_update は文字列
        if let Some(Value::String(s)) = v.get("on_update").cloned() {
            v["on_update"] =
                Value::Bool(s.parse::<bool>().map_err(|e| invalid("on_update", e, &v))?);
        }
        let manifest: PoseListenerManifest = serde_json::from_value(v.clone()).map_err(|e| {
            anyhow!(JuizError::PoseListenerError {
                message: format!("{v}: {e}")
            })
        })?;
        Ok(cb.lock_mut()?.pose_listener_create(&manifest)?.into())
    });
    cbs
}

pub(crate) fn pose_listener_read_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("list", |cb, _args| {
        log::debug!("[READ  ] pose_listener/list called");
        Ok(serde_json::to_value(cb.lock()?.pose_listener_list()?)?.into())
    });
    cbs.insert("profile_full", |cb, args| {
        log::debug!("[READ  ] pose_listener/profile_full called");
        Ok(cb
            .lock()?
            .pose_listener_profile_full(&param(&args, "identifier")?)?
            .into())
    });
    cbs
}

pub(crate) fn pose_listener_delete_callbacks() -> CallbackContainerType {
    let mut cbs = CallbackContainerType::new();
    cbs.insert("destroy", |cb, args| {
        log::debug!("[DELETE] pose_listener/destroy called");
        Ok(cb
            .lock()?
            .pose_listener_destroy(&param(&args, "identifier")?)?
            .into())
    });
    cbs
}
