//! pack の Container のモデルの検査(docs/spec/app-package.md §6.3 の 9、docs/spec/container-model.md §10、#383)
//!
//! entry の conf の `containers`・`composite_containers` の各 `model:` について
//!
//! - `path` が相対パスで、entry の conf の親ディレクトリから解決した先がパッケージの中にあり(`..` で外に出ない)、
//!   pack に入るファイル(除外規則の後)である
//! - core の作成時の検査(container-model.md §5.2、`ContainerModel::load`)に通る
//! - SDF なら、`<visual>` の `<mesh><uri>` が .sdf からの相対パスでパッケージの中を指す
//!
//! を確かめる。満たさなければエラー。SDF の `<mesh>` の指すファイルが pack に入らないときと、
//! `://` を含む uri は警告にする(`<mesh>` は今は表示に使わない、container-model.md §3.2.1)。

use super::names::normalize_relative;
use juiz_core::anyhow::{anyhow, bail, Context, Result};
use juiz_core::models::{max_bytes_from_options, ContainerModel};
use juiz_sdk::manifests::{ModelFormat, ModelManifest};
use juiz_sdk::serde_json::Value;
use std::path::Path;

/// conf の Container を並べるキー
const CONTAINER_KEYS: &[&str] = &["containers", "composite_containers"];

/// entry の conf のモデルを確かめる。警告の文を返す
///
/// - `entry`: entry の conf のパッケージの中のパス(正規化済み)
/// - `dir`: パッケージのディレクトリ
/// - `packed`: パッケージの中のパスが pack に入るファイルか
pub(crate) fn check_models(
    conf: &Value,
    entry: &str,
    dir: &Path,
    packed: impl Fn(&str) -> bool,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let entry_dir = parent_of(entry);
    let max_bytes = max_bytes_from_options(conf.get("option"))
        .map_err(|e| anyhow!("entry conf '{entry}': {e}"))?;
    for key in CONTAINER_KEYS {
        let Some(list) = conf.get(*key).filter(|v| !v.is_null()) else {
            continue;
        };
        let Some(list) = list.as_array() else {
            bail!("'{key}' in the entry conf must be a list");
        };
        for (i, c) in list.iter().enumerate() {
            let Some(model) = c.get("model").filter(|v| !v.is_null()) else {
                continue;
            };
            let label = match c.get("name").and_then(Value::as_str) {
                Some(name) => format!("the model of {key}[{i}] '{name}' in the entry conf"),
                None => format!("the model of {key}[{i}] in the entry conf"),
            };
            check_one(model, entry_dir, dir, max_bytes, &packed, &mut warnings)
                .with_context(|| format!("checking {label}"))?;
        }
    }
    Ok(warnings)
}

fn check_one(
    model: &Value,
    entry_dir: &str,
    dir: &Path,
    max_bytes: u64,
    packed: &impl Fn(&str) -> bool,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let manifest = ModelManifest::from_value(model)?;
    let raw = &manifest.path;
    let rel = resolve_inside(entry_dir, raw).ok_or_else(|| {
        anyhow!("'path: {raw}' must be a relative path inside the package (relative to the entry conf, without going out of the package with '..')")
    })?;
    if !packed(&rel) {
        bail!("the model file '{rel}' ('path: {raw}') is not in the package. Check that it exists and is not excluded (dot files, target/, build/, 'exclude' in juizapp.yaml)");
    }
    // core の作成時の検査(大きさの上限・形式の先頭の確かめ)。juiz run でも同じものが走る
    let abs_entry_dir = dir.join(entry_dir);
    let loaded = ContainerModel::load(&manifest, Some(&abs_entry_dir), max_bytes)?;
    if loaded.format() == ModelFormat::Sdf {
        let text = std::fs::read_to_string(dir.join(&rel))
            .with_context(|| format!("cannot read {rel}"))?;
        check_sdf_meshes(&text, &rel, packed, warnings)?;
    }
    Ok(())
}

/// パスの親(パッケージの中のパス)。ルートなら ""
fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// `base`(パッケージの中のディレクトリ)からの相対パス `raw` を、パッケージの中のパスにする。
/// 絶対パス・外に出るもの・空は `None`
fn resolve_inside(base: &str, raw: &str) -> Option<String> {
    // 絶対パスは base と繋ぐ前に断る
    let r = raw.replace('\\', "/");
    let b = r.as_bytes();
    if r.starts_with('/') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':') {
        return None;
    }
    let joined = if base.is_empty() {
        raw.to_owned()
    } else {
        format!("{base}/{raw}")
    };
    normalize_relative(&joined).filter(|p| !p.is_empty())
}

/// SDF の `<visual>` の `<mesh><uri>` がパッケージの中を指すか(container-model.md §3.2.1)
fn check_sdf_meshes(
    text: &str,
    sdf_rel: &str,
    packed: &impl Fn(&str) -> bool,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let doc = roxmltree::Document::parse(text).map_err(|e| anyhow!("{sdf_rel}: {e}"))?;
    let sdf_dir = parent_of(sdf_rel);
    let meshes = doc.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "mesh"
            && n.parent_element()
                .is_some_and(|g| g.tag_name().name() == "geometry")
            && n.parent_element()
                .and_then(|g| g.parent_element())
                .is_some_and(|v| v.tag_name().name() == "visual")
    });
    for mesh in meshes {
        let Some(uri) = mesh
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "uri")
            .and_then(|u| u.text())
            .map(str::trim)
            .filter(|u| !u.is_empty())
        else {
            continue; // juiz model check が警告する
        };
        if uri.contains("://") {
            warnings.push(format!(
                "{sdf_rel}: <mesh><uri>{uri}</uri> is not checked ('model://', 'package://' and URLs are not supported, docs/spec/container-model.md §3.2.1)"
            ));
            continue;
        }
        let Some(rel) = resolve_inside(sdf_dir, uri) else {
            bail!("{sdf_rel}: <mesh><uri>{uri}</uri> must be a relative path inside the package (relative to the .sdf, without going out of the package with '..')");
        };
        if !packed(&rel) {
            warnings.push(format!(
                "{sdf_rel}: the mesh file '{rel}' (<uri>{uri}</uri>) is not in the package"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use juiz_sdk::serde_json::json;
    use std::collections::BTreeSet;

    const STL: &[u8] = b"solid x\nendsolid x\n";
    const SDF: &str = r#"<sdf version="1.9"><model name="m"><link name="l">
<visual name="a"><geometry><box><size>1 1 1</size></box></geometry></visual>
<visual name="b"><geometry><mesh><uri>meshes/body.stl</uri></mesh></geometry></visual>
<collision name="c"><geometry><mesh><uri>/abs/ignored.stl</uri></mesh></geometry></collision>
</link></model></sdf>"#;

    fn setup(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("juiz_pack_models_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("conf/models/meshes")).unwrap();
        std::fs::write(dir.join("conf/models/a.stl"), STL).unwrap();
        std::fs::write(dir.join("conf/models/meshes/body.stl"), STL).unwrap();
        std::fs::write(dir.join("conf/models/m.sdf"), SDF).unwrap();
        std::fs::write(dir.join("conf/models/bad.stl"), b"not stl").unwrap();
        dir
    }

    fn check(dir: &Path, conf: Value, packed: &[&str]) -> Result<Vec<String>> {
        let set: BTreeSet<String> = packed.iter().map(|s| s.to_string()).collect();
        check_models(&conf, "conf/app.conf", dir, |p| set.contains(p))
    }

    fn one(path: &str) -> Value {
        json!({ "containers": [ { "type_name": "static_anchor", "name": "floor", "model": { "path": path } } ] })
    }

    const ALL: &[&str] = &[
        "conf/models/a.stl",
        "conf/models/m.sdf",
        "conf/models/meshes/body.stl",
        "conf/models/bad.stl",
    ];

    #[test]
    fn resolves_inside() {
        assert_eq!(
            resolve_inside("conf", "models/a.stl").as_deref(),
            Some("conf/models/a.stl")
        );
        assert_eq!(
            resolve_inside("conf", "../m/a.stl").as_deref(),
            Some("m/a.stl")
        );
        assert_eq!(resolve_inside("", "./a.stl").as_deref(), Some("a.stl"));
        assert_eq!(resolve_inside("conf", "../../a.stl"), None);
        assert_eq!(resolve_inside("conf", "/abs/a.stl"), None);
        assert_eq!(resolve_inside("conf", "C:\\a.stl"), None);
        assert_eq!(resolve_inside("conf", ".."), None);
    }

    #[test]
    fn accepts_models_inside_the_package() {
        let dir = setup("ok");
        assert!(check(&dir, one("models/a.stl"), ALL).unwrap().is_empty());
        let w = check(&dir, one("models/m.sdf"), ALL).unwrap();
        assert!(w.is_empty(), "{w:?}");
        // composite_containers も見る。model の無いものは飛ばす
        let conf = json!({
            "containers": [ { "type_name": "static_anchor", "name": "a" } ],
            "composite_containers": [ { "name": "cc", "model": { "path": "models/a.stl" } } ],
        });
        assert!(check(&dir, conf, ALL).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_paths_outside_or_not_packed() {
        let dir = setup("ng");
        let e =
            |conf: Value, packed: &[&str]| format!("{:#}", check(&dir, conf, packed).unwrap_err());
        let msg = e(one("../../outside.stl"), ALL);
        assert!(
            msg.contains("inside the package") && msg.contains("'floor'"),
            "{msg}"
        );
        let abs = dir.join("conf/models/a.stl");
        let msg = e(one(abs.to_str().unwrap()), ALL);
        assert!(msg.contains("inside the package"), "{msg}");
        // 除外されたファイル
        let msg = e(one("models/a.stl"), &["conf/models/m.sdf"]);
        assert!(msg.contains("is not in the package"), "{msg}");
        // core の作成時の検査
        let msg = e(one("models/bad.stl"), ALL);
        assert!(msg.contains("bad.stl"), "{msg}");
        // 上限
        let mut conf = one("models/a.stl");
        conf["option"] = json!({ "model": { "max_bytes": 4 } });
        let msg = e(conf, ALL);
        assert!(msg.contains("too large"), "{msg}");
        // キーの誤り
        let msg = e(
            json!({ "containers": [ { "name": "x", "model": { "path": "models/a.stl", "scale": "big" } } ] }),
            ALL,
        );
        assert!(msg.contains("scale"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_sdf_mesh_uri() {
        let dir = setup("mesh");
        // mesh のファイルが入らなければ警告
        let w = check(&dir, one("models/m.sdf"), &["conf/models/m.sdf"]).unwrap();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("conf/models/meshes/body.stl"), "{w:?}");
        // 外を指せばエラー
        for uri in ["../../../x.stl", "/abs/x.stl"] {
            std::fs::write(
                dir.join("conf/models/m.sdf"),
                SDF.replace("meshes/body.stl", uri),
            )
            .unwrap();
            let msg = format!("{:#}", check(&dir, one("models/m.sdf"), ALL).unwrap_err());
            assert!(msg.contains("<mesh><uri>") && msg.contains(uri), "{msg}");
        }
        // model:// は確かめずに警告
        std::fs::write(
            dir.join("conf/models/m.sdf"),
            SDF.replace("meshes/body.stl", "model://robot/body.stl"),
        )
        .unwrap();
        let w = check(&dir, one("models/m.sdf"), ALL).unwrap();
        assert!(w.len() == 1 && w[0].contains("not checked"), "{w:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
