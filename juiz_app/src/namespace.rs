//! コマンドラインのネームスペース指定 (issue #151)
//!
//! - `juiz -f robot.conf --namespace robot1` で conf の既定ネームスペースを差し替える
//! - `juiz process list --namespace robot1` などで一覧を絞り込み、`--long` でネームスペースを列に出す

use juiz_core::anyhow::{self, anyhow};
use juiz_core::prelude::*;
use juiz_sdk::identifier::{is_in_namespace, validate_namespace};
use juiz_sdk::process_identifier::ProcessIdentifier;

/// 先頭の `/` を外して検査したネームスペース。
fn checked_namespace(namespace: &str) -> JuizResult<String> {
    let namespace = namespace.strip_prefix('/').unwrap_or(namespace);
    validate_namespace(namespace)?;
    Ok(namespace.to_owned())
}

/// conf のトップレベル `namespace`(System の既定ネームスペース)を `namespace` で置き換える。
///
/// `None` なら conf のまま。`/` や空文字はルートになる。
/// オブジェクトごとの `namespace` は今までどおり、ここで決めた既定ネームスペースから解決される。
pub(crate) fn override_namespace(manifest: Value, namespace: Option<&str>) -> JuizResult<Value> {
    let Some(namespace) = namespace else {
        return Ok(manifest);
    };
    let namespace = checked_namespace(namespace)?;
    let mut manifest = manifest;
    match manifest.as_object_mut() {
        Some(map) => {
            map.insert("namespace".to_owned(), jvalue!(namespace));
        }
        None => {
            return Err(anyhow!(JuizError::InvalidValueError {
                message: format!("system manifest must be an object, but {manifest}"),
            }))
        }
    }
    Ok(manifest)
}

/// 一覧の 1 行
pub(crate) struct ListEntry {
    pub namespace: String,
    pub name: String,
    pub type_name: String,
    pub identifier: String,
}

impl From<&ProcessIdentifier> for ListEntry {
    fn from(id: &ProcessIdentifier) -> Self {
        ListEntry {
            namespace: id.namespace.clone(),
            name: id.name.clone(),
            type_name: id.type_name.clone(),
            identifier: id.to_string(),
        }
    }
}

impl From<&ContainerIdentifier> for ListEntry {
    fn from(id: &ContainerIdentifier) -> Self {
        ListEntry {
            namespace: id.namespace.clone(),
            name: id.name.clone(),
            type_name: id.type_name.clone(),
            identifier: id.to_string(),
        }
    }
}

impl TryFrom<&str> for ListEntry {
    type Error = anyhow::Error;

    fn try_from(id: &str) -> JuizResult<Self> {
        let s: IdentifierStruct = id.to_owned().try_into()?;
        Ok(ListEntry {
            namespace: s.namespace,
            name: s.object_name,
            type_name: s.type_name,
            identifier: id.to_owned(),
        })
    }
}

/// `namespace` で絞り込んだ一覧を文字列にする。
///
/// 既定は今までどおり ID の配列 (`["core://core/process/robot1/inc0::increment", ...]`)。
/// `long` なら `NAMESPACE NAME TYPE ID` の表にする(ルートのネームスペースは `/`)。
pub(crate) fn format_list(
    entries: Vec<ListEntry>,
    namespace: Option<&str>,
    long: bool,
) -> JuizResult<String> {
    let filter = match namespace {
        Some(ns) => checked_namespace(ns)?,
        None => String::new(),
    };
    let entries: Vec<ListEntry> = entries
        .into_iter()
        .filter(|e| is_in_namespace(&e.namespace, &filter))
        .collect();
    if !long {
        let ids: Vec<&String> = entries.iter().map(|e| &e.identifier).collect();
        return Ok(format!("{ids:?}"));
    }
    let rows: Vec<[String; 4]> = entries
        .iter()
        .map(|e| {
            [
                format!("/{}", e.namespace),
                e.name.clone(),
                e.type_name.clone(),
                e.identifier.clone(),
            ]
        })
        .collect();
    let header = [
        "NAMESPACE".to_owned(),
        "NAME".to_owned(),
        "TYPE".to_owned(),
        "ID".to_owned(),
    ];
    let mut widths = header.clone().map(|h| h.len());
    for row in rows.iter() {
        for (w, c) in widths.iter_mut().zip(row.iter()) {
            *w = (*w).max(c.chars().count());
        }
    }
    let lines: Vec<String> = std::iter::once(&header)
        .chain(rows.iter())
        .map(|row| {
            let line = format!(
                "{:w0$}  {:w1$}  {:w2$}  {}",
                row[0],
                row[1],
                row[2],
                row[3],
                w0 = widths[0],
                w1 = widths[1],
                w2 = widths[2]
            );
            line.trim_end().to_owned()
        })
        .collect();
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> ListEntry {
        id.try_into().unwrap()
    }

    fn entries() -> Vec<ListEntry> {
        vec![
            entry("core://core/process/inc0::increment"),
            entry("core://core/process/robot1/inc0::increment"),
            entry("core://core/process/robot1/arm/inc1::increment"),
            entry("http://127.0.0.1:8000/process/robot10/inc0::increment"),
        ]
    }

    #[test]
    fn override_namespace_replaces_default_namespace() {
        let m = jvalue!({"name": "s", "namespace": "conf_ns"});
        let m = override_namespace(m, Some("/robot1/arm")).unwrap();
        assert_eq!(m["namespace"], jvalue!("robot1/arm"));
        let m = override_namespace(m, Some("/")).unwrap();
        assert_eq!(m["namespace"], jvalue!(""));
        let m = override_namespace(jvalue!({"name": "s"}), None).unwrap();
        assert!(m.get("namespace").is_none());
    }

    #[test]
    fn override_namespace_rejects_invalid_namespace() {
        for ns in ["robot1:x", "a|b", "a//b", "robot1/"] {
            assert!(
                override_namespace(jvalue!({}), Some(ns)).is_err(),
                "{ns} should be rejected"
            );
        }
    }

    #[test]
    fn entry_from_identifier_string() {
        let e = entry("http://127.0.0.1:8000/process/robot1/arm/inc1::increment");
        assert_eq!(e.namespace, "robot1/arm");
        assert_eq!(e.name, "inc1");
        assert_eq!(e.type_name, "increment");
    }

    #[test]
    fn format_list_filters_by_namespace() {
        assert_eq!(
            format_list(entries(), Some("robot1"), false).unwrap(),
            r#"["core://core/process/robot1/inc0::increment", "core://core/process/robot1/arm/inc1::increment"]"#
        );
        assert_eq!(
            format_list(entries(), Some("/robot1/arm"), false).unwrap(),
            r#"["core://core/process/robot1/arm/inc1::increment"]"#
        );
        // 指定なし・`/` は絞らない
        assert_eq!(
            format_list(entries(), None, false).unwrap(),
            format_list(entries(), Some("/"), false).unwrap()
        );
        assert!(format_list(entries(), Some("a:b"), false).is_err());
    }

    #[test]
    fn format_list_long_shows_namespace_column() {
        let s = format_list(entries(), Some("robot1"), true).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3, "{s}");
        assert!(lines[0].starts_with("NAMESPACE"), "{s}");
        assert!(lines[1].starts_with("/robot1 "), "{s}");
        assert!(lines[1].contains(" inc0 "), "{s}");
        assert!(lines[2].starts_with("/robot1/arm "), "{s}");
        assert!(
            lines[2].ends_with("core://core/process/robot1/arm/inc1::increment"),
            "{s}"
        );
        let root = format_list(entries(), None, true).unwrap();
        assert!(root.lines().nth(1).unwrap().starts_with("/ "), "{root}");
    }
}
