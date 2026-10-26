//! conf の `namespace` を解決する (issue #148)
//!
//! ```yaml
//! namespace: robot1          # System 全体の既定ネームスペース (省略時はルート)
//! processes:
//!   - type_name: increment
//!     name: inc0             # => robot1/inc0
//!   - type_name: increment
//!     name: inc0
//!     namespace: /tools      # 先頭 / は絶対指定 => tools/inc0
//! ```
//!
//! 各オブジェクトの `namespace` は、相対なら既定ネームスペースの下、`/` 始まりなら絶対。
//! 名前で参照する箇所 (connections の source・destination、ecs の bind) は、
//! 参照する側 (Connection、EC) のネームスペースで解決する。
//! 解決した結果は先頭の `/` を付けない絶対ネームスペースとして各オブジェクトの `namespace` に書き戻す。
//! 解決は 1 回だけ行う (書き戻した値をもう一度解決すると相対として扱われるため)。

use juiz_sdk::{
    anyhow::{anyhow, Context},
    identifier::{join_namespace, validate_namespace},
};

use crate::prelude::*;

/// `namespace` を解決する。`None` なら `base`、`/` 始まりなら絶対、それ以外は `base` の下
fn resolve(base: &str, namespace: Option<&str>) -> JuizResult<String> {
    let resolved = match namespace {
        None => base.to_owned(),
        Some(ns) => match ns.strip_prefix('/') {
            Some(absolute) => absolute.to_owned(),
            None if ns.is_empty() => base.to_owned(),
            None => join_namespace(base, ns),
        },
    };
    validate_namespace(&resolved)?;
    Ok(resolved)
}

/// オブジェクトの `namespace` キーを読む (無ければ `None`)
fn namespace_of(value: &Value) -> JuizResult<Option<&str>> {
    match value.get("namespace") {
        None => Ok(None),
        Some(v) => v.as_str().map(Some).ok_or_else(|| {
            anyhow!(JuizError::InvalidValueError {
                message: format!("'namespace' must be a string, but {v}")
            })
        }),
    }
}

/// オブジェクトの `namespace` を `base` で解決して書き戻し、解決したネームスペースを返す
fn resolve_object(value: &mut Value, base: &str) -> JuizResult<String> {
    let resolved = resolve(base, namespace_of(value)?)
        .with_context(|| format!("resolving namespace of {value}"))?;
    if let Some(map) = value.as_object_mut() {
        map.insert("namespace".to_owned(), jvalue!(resolved));
    }
    Ok(resolved)
}

/// ID を直接書いた参照 (文字列、または `identifier` / `id` キーを持つ object) か
fn is_identifier_reference(value: &Value) -> bool {
    value.is_string() || value.get("identifier").is_some() || value.get("id").is_some()
}

fn for_each_in_array(
    manifest: &mut Value,
    key: &str,
    mut f: impl FnMut(&mut Value) -> JuizResult<()>,
) -> JuizResult<()> {
    if let Some(array) = manifest.get_mut(key).and_then(|v| v.as_array_mut()) {
        for v in array.iter_mut() {
            f(v)?;
        }
    }
    Ok(())
}

/// System の manifest の各オブジェクトの `namespace` を解決した manifest を返す
pub(super) fn resolve_namespaces(manifest: &Value) -> JuizResult<Value> {
    let mut manifest = manifest.clone();
    // System の既定ネームスペースは常に絶対 (先頭の / はあっても無くてもよい)
    let default_namespace = resolve("", namespace_of(&manifest)?)
        .context("resolving the default namespace of the system")?;

    for_each_in_array(&mut manifest, "processes", |p| {
        resolve_object(p, &default_namespace).map(|_| ())
    })?;

    // CompositeContainer も Container と同じ規則。スロットの名前は作るときに CompositeContainer のネームスペースで解決する (issue #169)
    for key in ["containers", "composite_containers"] {
        for_each_in_array(&mut manifest, key, |c| {
            let container_namespace = resolve_object(c, &default_namespace)?;
            // ContainerProcess は所属 Container と同じネームスペースに置く
            for_each_in_array(c, "processes", |cp| {
                let cp_namespace = resolve(&default_namespace, namespace_of(cp)?)?;
                if namespace_of(cp)?.is_some() && cp_namespace != container_namespace {
                    return Err(anyhow!(JuizError::InvalidNameError {
                    message: format!(
                        "ContainerProcess must be in the same namespace as its Container ('{container_namespace}'), but '{cp_namespace}' ({cp})"
                    )
                }));
                }
                if let Some(map) = cp.as_object_mut() {
                    map.insert("namespace".to_owned(), jvalue!(container_namespace));
                }
                Ok(())
            })
        })?;
    }

    for_each_in_array(&mut manifest, "ecs", |ec| {
        let ec_namespace = resolve_object(ec, &default_namespace)?;
        // bind する Process は EC のネームスペースで解決する
        for_each_in_array(ec, "bind", |b| {
            if !is_identifier_reference(b) {
                resolve_object(b, &ec_namespace)?;
            }
            Ok(())
        })
    })?;

    for_each_in_array(&mut manifest, "connections", |c| {
        let connection_namespace = resolve_object(c, &default_namespace)?;
        // source・destination は Connection のネームスペースで解決する
        for key in ["source", "destination"] {
            if let Some(end) = c.get_mut(key) {
                if !is_identifier_reference(end) {
                    resolve_object(end, &connection_namespace)?;
                }
            }
        }
        Ok(())
    })?;

    if let Some(map) = manifest.as_object_mut() {
        map.insert("namespace".to_owned(), jvalue!(default_namespace));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_and_absolute() {
        assert_eq!(resolve("", None).unwrap(), "");
        assert_eq!(resolve("robot1", None).unwrap(), "robot1");
        assert_eq!(resolve("robot1", Some("")).unwrap(), "robot1");
        assert_eq!(resolve("robot1", Some("arm")).unwrap(), "robot1/arm");
        assert_eq!(resolve("", Some("arm")).unwrap(), "arm");
        assert_eq!(resolve("robot1", Some("/tools")).unwrap(), "tools");
        assert_eq!(resolve("robot1", Some("/")).unwrap(), "");
        assert!(resolve("robot1", Some("a:b")).is_err());
        assert!(resolve("robot1", Some("a//b")).is_err());
        assert!(resolve("robot1", Some("arm/")).is_err());
    }

    #[test]
    fn resolve_namespaces_of_objects() {
        let manifest = jvalue!({
            "namespace": "/robot1",
            "processes": [
                {"type_name": "increment", "name": "inc0"},
                {"type_name": "increment", "name": "inc0", "namespace": "arm"},
                {"type_name": "increment", "name": "inc0", "namespace": "/tools"},
            ],
            "containers": [
                {"type_name": "c", "name": "c0", "namespace": "arm",
                 "processes": [{"type_name": "cp", "name": "cp0"}]},
            ],
            "ecs": [
                {"type_name": "timer_ec", "name": "ec0",
                 "bind": [
                    {"type_name": "increment", "name": "inc0"},
                    {"type_name": "increment", "name": "inc0", "namespace": "/tools"},
                    "core://core/process/inc0::increment",
                 ]},
            ],
            "connections": [
                {"namespace": "arm",
                 "source": {"type_name": "increment", "name": "inc0"},
                 "destination": {"type_name": "increment", "name": "inc0", "namespace": "/robot1"},
                 "arg_name": "arg1"},
            ],
        });
        let m = resolve_namespaces(&manifest).unwrap();
        assert_eq!(m["namespace"], jvalue!("robot1"));
        assert_eq!(m["processes"][0]["namespace"], jvalue!("robot1"));
        assert_eq!(m["processes"][1]["namespace"], jvalue!("robot1/arm"));
        assert_eq!(m["processes"][2]["namespace"], jvalue!("tools"));
        assert_eq!(m["containers"][0]["namespace"], jvalue!("robot1/arm"));
        assert_eq!(
            m["containers"][0]["processes"][0]["namespace"],
            jvalue!("robot1/arm")
        );
        assert_eq!(m["ecs"][0]["namespace"], jvalue!("robot1"));
        assert_eq!(m["ecs"][0]["bind"][0]["namespace"], jvalue!("robot1"));
        assert_eq!(m["ecs"][0]["bind"][1]["namespace"], jvalue!("tools"));
        assert_eq!(
            m["ecs"][0]["bind"][2],
            jvalue!("core://core/process/inc0::increment")
        );
        assert_eq!(
            m["connections"][0]["source"]["namespace"],
            jvalue!("robot1/arm")
        );
        assert_eq!(
            m["connections"][0]["destination"]["namespace"],
            jvalue!("robot1")
        );
    }

    #[test]
    fn resolve_namespaces_of_composite_containers() {
        let manifest = jvalue!({
            "namespace": "robot1",
            "composite_containers": [
                {"name": "cc0", "slots": {"s": "c0"},
                 "processes": [{"type_name": "cp", "name": "cp0"}]},
                {"name": "cc1", "namespace": "/tools", "slots": {"s": "c0"}},
            ],
        });
        let m = resolve_namespaces(&manifest).unwrap();
        let cc = &m["composite_containers"];
        assert_eq!(cc[0]["namespace"], jvalue!("robot1"));
        assert_eq!(cc[0]["processes"][0]["namespace"], jvalue!("robot1"));
        assert_eq!(cc[1]["namespace"], jvalue!("tools"));
        // スロットの名前はここでは書き換えない
        assert_eq!(cc[0]["slots"]["s"], jvalue!("c0"));

        let other = jvalue!({
            "composite_containers": [
                {"name": "cc0", "namespace": "robot1", "slots": {},
                 "processes": [{"type_name": "cp", "name": "cp0", "namespace": "robot2"}]},
            ],
        });
        assert!(resolve_namespaces(&other).is_err());
    }

    #[test]
    fn container_process_in_other_namespace_is_error() {
        let manifest = jvalue!({
            "containers": [
                {"type_name": "c", "name": "c0", "namespace": "robot1",
                 "processes": [{"type_name": "cp", "name": "cp0", "namespace": "robot2"}]},
            ],
        });
        assert!(resolve_namespaces(&manifest).is_err());
    }

    #[test]
    fn invalid_default_namespace_is_error() {
        assert!(resolve_namespaces(&jvalue!({"namespace": "robot|1"})).is_err());
        assert!(resolve_namespaces(&jvalue!({"namespace": 1})).is_err());
    }
}
