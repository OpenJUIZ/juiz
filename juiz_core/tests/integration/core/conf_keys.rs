//! リポジトリにある conf の Process に、juiz が読まないキーが無いこと (issue #401)
//!
//! `publishes` / `subscribes` を `publish` / `subscribe` と書いた conf があり、Topic が黙って
//! 作られていなかった。知らないキーは起動時に警告するようにしたが、リポジトリの conf は
//! このテストで書き間違いを見つける。

use juiz_core::prelude::*;
use juiz_sdk::utils::yaml_conf_load::yaml_conf_load;
use std::path::{Path, PathBuf};

fn collect_confs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name.starts_with('.') || name == "node_modules" {
                continue;
            }
            collect_confs(&path, out);
        } else if name.ends_with(".conf") {
            out.push(path);
        }
    }
}

/// conf の中の Process の書き方 (`processes` と、Container / CompositeContainer の `processes`)
fn process_values(conf: &Value) -> Vec<&Value> {
    let mut values = Vec::new();
    if let Some(ps) = conf.get("processes").and_then(|v| v.as_array()) {
        values.extend(ps.iter());
    }
    for key in ["containers", "composite_containers"] {
        for c in conf
            .get(key)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(ps) = c.get("processes").and_then(|v| v.as_array()) {
                values.extend(ps.iter());
            }
        }
    }
    values
}

#[test]
fn repository_confs_have_no_unknown_process_keys() -> JuizResult<()> {
    let mut confs = Vec::new();
    collect_confs(Path::new(".."), &mut confs);
    confs.sort();
    assert!(!confs.is_empty(), "conf が 1 つも見つからない");

    let mut errors = Vec::new();
    for path in confs.iter() {
        let conf = match yaml_conf_load(path.display().to_string()) {
            Ok(v) => v,
            Err(_) => continue, // System の conf でないもの (YAML でないもの) は対象外
        };
        for p in process_values(&conf) {
            let unknown = ProcessManifest::unknown_conf_keys(p);
            if !unknown.is_empty() {
                errors.push(format!("{}: {}", path.display(), unknown.join(", ")));
            }
        }
    }
    assert!(
        errors.is_empty(),
        "知らないキーがある conf:\n{}",
        errors.join("\n")
    );
    Ok(())
}
