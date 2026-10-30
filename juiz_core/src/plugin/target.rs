//! conf の plugins の path に書いた `{target}` の置き換え(docs/spec/app-package.md §5.2、#284)

use crate::prelude::*;

/// juiz をビルドしたときの target triple (例: x86_64-unknown-linux-gnu)
pub const HOST_TARGET: &str = env!("JUIZ_TARGET");

const TARGET_PLACEHOLDER: &str = "{target}";

/// manifest の plugins の下の `path` に含まれる `{target}` を HOST_TARGET に置き換える。
///
/// plugins の下のすべての種類(process_factories、container_factories とその processes、
/// container_process_factories、components、broker_factories など)が対象。
/// `{target}` 以外の `{…}` はそのまま残す。
pub fn replace_target_in_plugin_paths(mut manifest: Value) -> Value {
    if let Some(plugins) = manifest.get_mut("plugins") {
        replace_target_in_paths(plugins, HOST_TARGET);
    }
    manifest
}

fn replace_target_in_paths(v: &mut Value, target: &str) {
    match v {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match child {
                    Value::String(s) if key == "path" && s.contains(TARGET_PLACEHOLDER) => {
                        *s = s.replace(TARGET_PLACEHOLDER, target);
                    }
                    _ => replace_target_in_paths(child, target),
                }
            }
        }
        Value::Array(arr) => arr
            .iter_mut()
            .for_each(|c| replace_target_in_paths(c, target)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_target_is_not_empty() {
        assert!(!HOST_TARGET.is_empty());
        assert!(!HOST_TARGET.contains('{'));
    }

    #[test]
    fn replaces_target_in_every_plugin_kind() {
        let m = jvalue!({
            "plugins": {
                "broker_factories": { "b": { "path": "plugins/{target}" } },
                "process_factories": {
                    "p": { "path": "plugins/{target}/x" },
                    "py": { "language": "python", "path": "python" }
                },
                "container_factories": {
                    "c": { "path": "plugins/{target}", "processes": { "cp": { "path": "{target}/{target}" } } }
                },
                "container_process_factories": { "cpf": { "path": "a/{target}" } },
                "components": { "comp": { "path": "plugins/{target}" } },
                "ec_factories": { "e": { "path": "plugins/{target}" } }
            }
        });
        let r = replace_target_in_plugin_paths(m);
        let t = HOST_TARGET;
        let p = &r["plugins"];
        assert_eq!(p["broker_factories"]["b"]["path"], format!("plugins/{t}"));
        assert_eq!(
            p["process_factories"]["p"]["path"],
            format!("plugins/{t}/x")
        );
        assert_eq!(p["process_factories"]["py"]["path"], "python");
        assert_eq!(
            p["container_factories"]["c"]["path"],
            format!("plugins/{t}")
        );
        assert_eq!(
            p["container_factories"]["c"]["processes"]["cp"]["path"],
            format!("{t}/{t}")
        );
        assert_eq!(
            p["container_process_factories"]["cpf"]["path"],
            format!("a/{t}")
        );
        assert_eq!(p["components"]["comp"]["path"], format!("plugins/{t}"));
        assert_eq!(p["ec_factories"]["e"]["path"], format!("plugins/{t}"));
    }

    #[test]
    fn keeps_other_placeholders_and_outside_plugins() {
        let m = jvalue!({
            "name": "{target}",
            "processes": [{ "path": "{target}" }],
            "plugins": { "process_factories": { "p": { "path": "{TARGET}/{os}/{target}", "name": "{target}" } } }
        });
        let r = replace_target_in_plugin_paths(m);
        assert_eq!(r["name"], "{target}");
        assert_eq!(r["processes"][0]["path"], "{target}");
        let p = &r["plugins"]["process_factories"]["p"];
        assert_eq!(p["path"], format!("{{TARGET}}/{{os}}/{HOST_TARGET}"));
        assert_eq!(p["name"], "{target}");
    }

    #[test]
    fn manifest_without_plugins_is_unchanged() {
        let m = jvalue!({ "name": "s" });
        assert_eq!(replace_target_in_plugin_paths(m.clone()), m);
    }
}
