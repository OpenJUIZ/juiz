//! Container の 3D モデルの CLI (issue #382、仕様 docs/spec/container-model.md §9)
//!
//! ```text
//! juiz container model <id> [--info] [-o <出力先>]   # モデル情報を表示する・中身を書き出す
//! juiz model check <file> [--format <形式>] [--up-axis y|z] [--scale <数>]
//! ```
//!
//! - `container model` は BrokerProxy の `container_model_info` / `container_model` を使う。
//!   HTTP の BrokerProxy は生のバイト列の経路 (§6.2)、それ以外は JSON の経路 (§6.3) で取る。
//! - `model check` は System を起動しない。core の作成時の検査 (`ContainerModel::load`、§5.2) をそのまま
//!   通し、加えて SDF の書き間違い・glTF の外部参照・外接箱の大きさを警告する (§9.2)。

use std::path::{Path, PathBuf};

use clap::Subcommand;
use juiz_core::models::{ContainerModel, ContainerModelData, DEFAULT_MODEL_MAX_BYTES};
use juiz_core::prelude::*;
use juiz_sdk::anyhow::anyhow;
use juiz_sdk::geometry::{Quaternion, Transform, Vector3};
use juiz_sdk::manifests::{ModelFormat, ModelManifest};

use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ModelSubCommands {
    /// Check a model file (.glb / .gltf / .stl / .sdf) without starting a System
    #[clap(arg_required_else_help = true)]
    Check {
        #[arg(help = "Model file")]
        file: String,

        #[arg(
            long = "format",
            help = "Format: glb, gltf, stl, sdf or other (default: from the extension)"
        )]
        format: Option<String>,

        #[arg(
            long = "up-axis",
            help = "Up axis in the file: y or z (default: y for glTF, z for others)"
        )]
        up_axis: Option<String>,

        #[arg(
            long = "scale",
            help = "Meters per unit of the file (default: 1.0; 0.001 for a STL in mm)"
        )]
        scale: Option<f64>,
    },
}

// ---------------------------------------------------------------------------
// juiz container model

/// `juiz container model`
pub(crate) fn on_container_model(
    manifest: Value,
    working_dir: &Path,
    args: Args,
    identifier: String,
    info: bool,
    output: Option<String>,
) -> JuizResult<()> {
    crate::transformations::print_error(crate::transformations::run_with_proxy(
        manifest,
        working_dir,
        &args,
        |proxy| {
            if output.is_none() || info {
                let v = proxy.container_model_info(&identifier)?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
            if let Some(out) = output.as_deref() {
                let d = ContainerModelData::from_value(&proxy.container_model(&identifier)?)?;
                let path = write_model(&d, Path::new(out))?;
                println!(
                    "wrote: {} ({} bytes, {})",
                    path.display(),
                    d.data.len(),
                    d.info["format"].as_str().unwrap_or_default()
                );
            }
            Ok(())
        },
    ))
}

/// 取り出したモデルを書き出す。`out` がディレクトリなら `<file_name>` で書く。書いた先を返す
pub(crate) fn write_model(d: &ContainerModelData, out: &Path) -> JuizResult<PathBuf> {
    let sha = juiz_core::models::sha256_hex(&d.data);
    if !d.sha256().is_empty() && d.sha256() != sha {
        return Err(anyhow!(JuizError::ContainerModelInvalidError {
            message: format!(
                "the received model does not match its sha256 ({} != {sha})",
                d.sha256()
            )
        }));
    }
    let path = if out.is_dir() {
        // file_name はディレクトリを含まない (§8) が、念のため名前の部分だけ使う
        let name = Path::new(d.file_name())
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| "model".into());
        out.join(name)
    } else {
        out.to_path_buf()
    };
    std::fs::write(&path, &d.data)
        .map_err(|e| anyhow!("can not write the model to '{}': {e}", path.display()))?;
    Ok(path)
}

/// `container list --long` の Container の行の下に出すモデルの行 (§7)
pub(crate) fn model_lines(profile: &ContainerProfile) -> Vec<String> {
    match profile.model.as_ref() {
        Some(m) if !m.is_null() => {
            let format = m["format"].as_str().unwrap_or_default();
            let size = m["size"].as_u64().unwrap_or_default();
            let file_name = m["file_name"].as_str().unwrap_or_default();
            vec![format!(
                "model: {format} {} ({file_name})",
                human_size(size)
            )]
        }
        _ => Vec::new(),
    }
}

/// 1023 B / 1.2 KiB / 3.4 MiB
pub(crate) fn human_size(size: u64) -> String {
    const KIB: f64 = 1024.0;
    let s = size as f64;
    if size < 1024 {
        format!("{size} B")
    } else if s < KIB * KIB {
        format!("{:.1} KiB", s / KIB)
    } else {
        format!("{:.1} MiB", s / KIB / KIB)
    }
}

// ---------------------------------------------------------------------------
// juiz model check

/// `juiz model check` の結果
#[derive(Debug)]
pub(crate) struct CheckReport {
    /// モデル情報 (§8)
    pub info: Value,
    /// 警告
    pub warnings: Vec<String>,
    /// 外接箱の大きさ [m] (scale を掛け、juiz の軸 (Z 上) にそろえたもの)。分からなければ `None`
    pub extent: Option<[f64; 3]>,
}

/// 外接箱が大きすぎる・小さすぎるとみなす値 [m] (§9.2)
const TOO_LARGE_M: f64 = 100.0;
const TOO_SMALL_M: f64 = 0.001;

/// `juiz model check` を実行する。エラーがあれば 1、無ければ (警告だけなら) 0
pub(crate) fn run_model_command(subcommand: &ModelSubCommands) -> i32 {
    match subcommand {
        ModelSubCommands::Check {
            file,
            format,
            up_axis,
            scale,
        } => match check_model_file(
            Path::new(file),
            format.as_deref(),
            up_axis.as_deref(),
            *scale,
        ) {
            Ok(r) => {
                print!("{}", format_report(file, &r));
                0
            }
            Err(e) => {
                println!("error: {e:#}");
                1
            }
        },
    }
}

pub(crate) fn format_report(file: &str, r: &CheckReport) -> String {
    let i = &r.info;
    let mut s = format!(
        "{file}: {} ({}), {} bytes, sha256 {}\n",
        i["format"].as_str().unwrap_or_default(),
        i["media_type"].as_str().unwrap_or_default(),
        i["size"],
        i["sha256"].as_str().unwrap_or_default()
    );
    s += &format!(
        "  up_axis: {}, scale: {}\n",
        i["up_axis"].as_str().unwrap_or_default(),
        i["scale"]
    );
    if let Some([x, y, z]) = r.extent {
        s += &format!(
            "  size: {} x {} x {} m (x, y, z after scale and up_axis)\n",
            num(x),
            num(y),
            num(z)
        );
    }
    for w in r.warnings.iter() {
        s += &format!("warning: {w}\n");
    }
    s += &match r.warnings.len() {
        0 => "OK\n".to_owned(),
        1 => "OK (1 warning)\n".to_owned(),
        n => format!("OK ({n} warnings)\n"),
    };
    s
}

/// 表示用の数 (有効数字 6 桁くらい、余分な 0 は付けない)
fn num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

/// モデルのファイルを確かめる (§9.2)。core の作成時の検査 (§5.2) に通らなければエラー
pub(crate) fn check_model_file(
    file: &Path,
    format: Option<&str>,
    up_axis: Option<&str>,
    scale: Option<f64>,
) -> JuizResult<CheckReport> {
    let path = std::path::absolute(file).unwrap_or_else(|_| file.to_path_buf());
    let mut conf = jvalue!({ "path": path.to_string_lossy() });
    if let Some(f) = format {
        conf["format"] = jvalue!(f);
    }
    if let Some(a) = up_axis {
        conf["up_axis"] = jvalue!(a);
    }
    if let Some(s) = scale {
        conf["scale"] = jvalue!(s);
    }
    let manifest = ModelManifest::from_value(&conf)?;
    let model = ContainerModel::load(&manifest, None, DEFAULT_MODEL_MAX_BYTES)?;
    let info = model.info();
    let scale = info["scale"].as_f64().unwrap_or(1.0);
    let up_y = info["up_axis"].as_str() == Some("y");

    let mut warnings = Vec::new();
    let read =
        || std::fs::read(&path).map_err(|e| anyhow!("can not read '{}': {e}", path.display()));
    let bounds = match model.format() {
        ModelFormat::Glb => {
            let bytes = read()?;
            match glb_json(&bytes) {
                Ok(json) => gltf_check(&json, &mut warnings),
                Err(why) => {
                    warnings.push(format!("can not read the JSON chunk of the .glb: {why}"));
                    None
                }
            }
        }
        ModelFormat::Gltf => {
            let json: Value = serde_json::from_slice(&read()?)?;
            gltf_check(&json, &mut warnings)
        }
        ModelFormat::Stl => stl_bounds(&read()?, &mut warnings),
        ModelFormat::Sdf => {
            let bytes = read()?;
            let text = String::from_utf8_lossy(&bytes);
            let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            sdf_check(&text, &dir, &mut warnings)
        }
        ModelFormat::Other => None,
    };

    let extent = bounds.map(|b| {
        let e = [
            (b.max[0] - b.min[0]) * scale,
            (b.max[1] - b.min[1]) * scale,
            (b.max[2] - b.min[2]) * scale,
        ];
        // up_axis: y はファイルの +Y を juiz の +Z に、+Z を −Y にする (§4.2)
        if up_y {
            [e[0], e[2], e[1]]
        } else {
            e
        }
    });
    if let Some(e) = extent {
        let largest = e.iter().cloned().fold(0.0, f64::max);
        let shown = format!("{} x {} x {} m", num(e[0]), num(e[1]), num(e[2]));
        if largest > TOO_LARGE_M {
            let mut w = format!("the model is very large ({shown}, more than {TOO_LARGE_M} m)");
            if model.format() == ModelFormat::Stl && manifest.scale.is_none() {
                w += ". STL has no unit; if it is in millimeters, write scale: 0.001 in the conf (--scale 0.001 here)";
            } else {
                w += ". Check the scale";
            }
            warnings.push(w);
        } else if largest < TOO_SMALL_M {
            warnings.push(format!(
                "the model is very small ({shown}, less than 1 mm). Check the scale"
            ));
        }
    }
    Ok(CheckReport {
        info,
        warnings,
        extent,
    })
}

/// 外接箱 (ファイルの座標、scale を掛ける前)
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    fn empty() -> Bounds {
        Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    fn add(&mut self, p: [f64; 3]) {
        if !p.iter().all(|v| v.is_finite()) {
            return;
        }
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }

    fn is_empty(&self) -> bool {
        self.min[0] > self.max[0]
    }

    fn into_option(self) -> Option<Bounds> {
        (!self.is_empty()).then_some(self)
    }
}

// ---- STL ----

fn stl_bounds(bytes: &[u8], warnings: &mut Vec<String>) -> Option<Bounds> {
    let mut b = Bounds::empty();
    let binary = bytes.len() >= 84 && {
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        bytes.len() == 84 + 50 * n
    };
    if binary {
        let f = |o: usize| {
            f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as f64
        };
        let n = (bytes.len() - 84) / 50;
        for t in 0..n {
            for v in 0..3 {
                let o = 84 + 50 * t + 12 + 12 * v;
                b.add([f(o), f(o + 4), f(o + 8)]);
            }
        }
    } else {
        let text = String::from_utf8_lossy(bytes);
        for line in text.lines() {
            let mut it = line.split_whitespace();
            if it.next() != Some("vertex") {
                continue;
            }
            let v: Vec<f64> = it.filter_map(|s| s.parse().ok()).collect();
            if v.len() == 3 {
                b.add([v[0], v[1], v[2]]);
            }
        }
    }
    if b.is_empty() {
        warnings.push("the STL has no triangles".to_owned());
    }
    b.into_option()
}

// ---- glTF ----

/// .glb の最初のチャンク (JSON) を読む
fn glb_json(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 20 {
        return Err("too short".to_owned());
    }
    let len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if &bytes[16..20] != b"JSON" {
        return Err("the first chunk is not JSON".to_owned());
    }
    let chunk = bytes
        .get(20..20 + len)
        .ok_or_else(|| "the JSON chunk is cut off".to_owned())?;
    serde_json::from_slice(chunk).map_err(|e| e.to_string())
}

/// 外部参照の警告と外接箱 (ノードの変換を掛けた、POSITION の accessor の min / max から)
fn gltf_check(json: &Value, warnings: &mut Vec<String>) -> Option<Bounds> {
    for key in ["buffers", "images"] {
        for (i, e) in json[key].as_array().into_iter().flatten().enumerate() {
            if let Some(uri) = e["uri"].as_str() {
                if !uri.starts_with("data:") {
                    warnings.push(format!(
                        "{key}[{i}].uri refers to an external file '{uri}'. Only this file is served; \
                         the Web UI can not show it (docs/spec/container-model.md §3.3)"
                    ));
                }
            }
        }
    }
    let mut b = Bounds::empty();
    let nodes = json["nodes"].as_array();
    let scene = json["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = json["scenes"]
        .get(scene)
        .and_then(|s| s["nodes"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|n| n.as_u64())
                .map(|n| n as usize)
                .collect()
        })
        .unwrap_or_default();
    if roots.is_empty() || nodes.is_none() {
        // シーンが無ければノードの変換は掛けずに全部の mesh を見る
        for m in 0..json["meshes"].as_array().map(|a| a.len()).unwrap_or(0) {
            gltf_mesh_bounds(json, m, &Mat4::identity(), &mut b);
        }
    } else {
        for r in roots {
            gltf_node_bounds(json, r, &Mat4::identity(), &mut b, 0);
        }
    }
    b.into_option()
}

fn gltf_node_bounds(json: &Value, node: usize, parent: &Mat4, b: &mut Bounds, depth: usize) {
    let Some(n) = json["nodes"].get(node) else {
        return;
    };
    if depth > 64 {
        return;
    }
    let m = parent.mul(&Mat4::from_gltf_node(n));
    if let Some(mesh) = n["mesh"].as_u64() {
        gltf_mesh_bounds(json, mesh as usize, &m, b);
    }
    for c in n["children"].as_array().into_iter().flatten() {
        if let Some(c) = c.as_u64() {
            gltf_node_bounds(json, c as usize, &m, b, depth + 1);
        }
    }
}

fn gltf_mesh_bounds(json: &Value, mesh: usize, m: &Mat4, b: &mut Bounds) {
    for p in json["meshes"][mesh]["primitives"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(a) = p["attributes"]["POSITION"].as_u64() else {
            continue;
        };
        let acc = &json["accessors"][a as usize];
        let (Some(min), Some(max)) = (vec3(&acc["min"]), vec3(&acc["max"])) else {
            continue;
        };
        for i in 0..8 {
            let c = [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ];
            b.add(m.apply(c));
        }
    }
}

fn vec3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

/// 列優先の 4x4 (glTF の matrix と同じ並び)
#[derive(Debug, Clone, Copy)]
struct Mat4([f64; 16]);

impl Mat4 {
    fn identity() -> Mat4 {
        let mut m = [0.0; 16];
        for i in 0..4 {
            m[i * 5] = 1.0;
        }
        Mat4(m)
    }

    fn at(&self, row: usize, col: usize) -> f64 {
        self.0[col * 4 + row]
    }

    fn mul(&self, rhs: &Mat4) -> Mat4 {
        let mut m = [0.0; 16];
        for r in 0..4 {
            for c in 0..4 {
                m[c * 4 + r] = (0..4).map(|k| self.at(r, k) * rhs.at(k, c)).sum();
            }
        }
        Mat4(m)
    }

    fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let mut out = [0.0; 3];
        for (r, o) in out.iter_mut().enumerate() {
            *o = self.at(r, 0) * p[0] + self.at(r, 1) * p[1] + self.at(r, 2) * p[2] + self.at(r, 3);
        }
        out
    }

    /// glTF のノードの matrix か、translation・rotation・scale (T · R · S)
    fn from_gltf_node(n: &Value) -> Mat4 {
        if let Some(a) = n["matrix"].as_array() {
            if a.len() == 16 {
                let mut m = [0.0; 16];
                for (i, v) in a.iter().enumerate() {
                    m[i] = v.as_f64().unwrap_or(0.0);
                }
                return Mat4(m);
            }
        }
        let t = vec3(&n["translation"]).unwrap_or([0.0; 3]);
        let s = vec3(&n["scale"]).unwrap_or([1.0; 3]);
        let q = n["rotation"]
            .as_array()
            .filter(|a| a.len() == 4)
            .map(|a| {
                Quaternion::new(
                    a[0].as_f64().unwrap_or(0.0),
                    a[1].as_f64().unwrap_or(0.0),
                    a[2].as_f64().unwrap_or(0.0),
                    a[3].as_f64().unwrap_or(1.0),
                )
            })
            .unwrap_or_default();
        let q = q.normalized().unwrap_or_default();
        let mut m = [0.0; 16];
        for (c, axis) in [
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        ]
        .iter()
        .enumerate()
        {
            let r = q.rotate(axis);
            m[c * 4] = r.x * s[c];
            m[c * 4 + 1] = r.y * s[c];
            m[c * 4 + 2] = r.z * s[c];
        }
        m[12] = t[0];
        m[13] = t[1];
        m[14] = t[2];
        m[15] = 1.0;
        Mat4(m)
    }
}

// ---- SDF (§3.2) ----

/// SDF の部分集合の書き間違いを警告し、基本形の外接箱を返す
fn sdf_check(text: &str, dir: &Path, warnings: &mut Vec<String>) -> Option<Bounds> {
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return None; // core の検査で弾かれている
    };
    let Some(model) = doc
        .root_element()
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "model")
    else {
        return None;
    };
    let model_name = model.attribute("name").unwrap_or_default();
    let mut b = Bounds::empty();
    let model_pose = sdf_pose(model, &[], "<model>", warnings);
    let mut joints = 0;
    let mut shapes = 0;
    for child in model.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "link" => {
                let link_name = child.attribute("name").unwrap_or_default();
                let what = format!("<link name=\"{link_name}\">");
                let link_pose =
                    model_pose.compose(&sdf_pose(child, &[model_name], &what, warnings));
                for visual in child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "visual")
                {
                    let visual_name = visual.attribute("name").unwrap_or_default();
                    let what = format!("<visual name=\"{visual_name}\"> in {what}");
                    let pose = link_pose.compose(&sdf_pose(visual, &[link_name], &what, warnings));
                    sdf_material(visual, &what, warnings);
                    if let Some(local) = sdf_geometry(visual, &what, dir, warnings) {
                        shapes += 1;
                        for i in 0..8 {
                            let c = Vector3::new(
                                if i & 1 == 0 {
                                    local.min[0]
                                } else {
                                    local.max[0]
                                },
                                if i & 2 == 0 {
                                    local.min[1]
                                } else {
                                    local.max[1]
                                },
                                if i & 4 == 0 {
                                    local.min[2]
                                } else {
                                    local.max[2]
                                },
                            );
                            let p = pose.transform_point(&c);
                            b.add([p.x, p.y, p.z]);
                        }
                    }
                }
            }
            "joint" => joints += 1,
            "model" => warnings.push(format!(
                "nested <model name=\"{}\"> is skipped (docs/spec/container-model.md §3.2)",
                child.attribute("name").unwrap_or_default()
            )),
            _ => {}
        }
    }
    if joints > 0 {
        warnings.push(format!(
            "{joints} <joint> found. Links are drawn fixed at their <pose>. \
             Split moving parts into Containers connected by Transforms, or use the import (Step 9 #388)"
        ));
    }
    if shapes == 0 {
        warnings.push("no shape to show (no <visual> with a box, cylinder, sphere, capsule, ellipsoid or plane)".to_owned());
    }
    b.into_option()
}

/// 空白区切りの数を `n` 個読む
fn numbers(text: &str, n: usize) -> Result<Vec<f64>, String> {
    let v: Result<Vec<f64>, _> = text.split_whitespace().map(|s| s.parse::<f64>()).collect();
    let v = v.map_err(|_| format!("'{}' is not a list of numbers", text.trim()))?;
    if v.len() != n {
        return Err(format!(
            "needs {n} numbers, but '{}' has {}",
            text.trim(),
            v.len()
        ));
    }
    if !v.iter().all(|x| x.is_finite()) {
        return Err(format!("'{}' has a non-finite number", text.trim()));
    }
    Ok(v)
}

fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

/// 要素の直下の `<pose>` (無ければ単位)。`parents` は relative_to に書いてよい名前
fn sdf_pose(
    node: roxmltree::Node,
    parents: &[&str],
    what: &str,
    warnings: &mut Vec<String>,
) -> Transform {
    let Some(p) = child(node, "pose") else {
        return Transform::identity();
    };
    if let Some(r) = p.attribute("relative_to") {
        if !r.is_empty() && !parents.contains(&r) {
            warnings.push(format!(
                "<pose relative_to=\"{r}\"> of {what} is ignored. Only the parent element ({}) is supported",
                if parents.is_empty() { "none".to_owned() } else { parents.join(", ") }
            ));
        }
    }
    let v = match numbers(p.text().unwrap_or_default(), 6) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!(
                "<pose> of {what}: {e} (x y z roll pitch yaw). It is ignored"
            ));
            return Transform::identity();
        }
    };
    let k = if p.attribute("degrees") == Some("true") {
        std::f64::consts::PI / 180.0
    } else {
        1.0
    };
    Transform::new(
        Vector3::new(v[0], v[1], v[2]),
        Quaternion::from_rpy(v[3] * k, v[4] * k, v[5] * k),
    )
}

/// `<material>` の `<diffuse>`・`<ambient>` が 3 つか 4 つの 0〜1 の数か
fn sdf_material(visual: roxmltree::Node, what: &str, warnings: &mut Vec<String>) {
    let Some(m) = child(visual, "material") else {
        return;
    };
    for key in ["diffuse", "ambient"] {
        let Some(c) = child(m, key) else {
            continue;
        };
        let text = c.text().unwrap_or_default();
        let v = numbers(text, 4).or_else(|_| numbers(text, 3));
        match v {
            Err(_) => warnings.push(format!(
                "<{key}> of {what}: '{}' must be 3 or 4 numbers (r g b [a])",
                text.trim()
            )),
            Ok(v) if v.iter().any(|x| !(0.0..=1.0).contains(x)) => warnings.push(format!(
                "<{key}> of {what}: '{}' has a value outside 0 to 1",
                text.trim()
            )),
            Ok(_) => {}
        }
    }
}

/// `<geometry>` の基本形の、visual の座標での外接箱。表示しないもの (mesh・誤り) は `None`
fn sdf_geometry(
    visual: roxmltree::Node,
    what: &str,
    dir: &Path,
    warnings: &mut Vec<String>,
) -> Option<Bounds> {
    let Some(g) = child(visual, "geometry") else {
        warnings.push(format!("{what} has no <geometry>"));
        return None;
    };
    let Some(shape) = g.children().find(|n| n.is_element()) else {
        warnings.push(format!("<geometry> of {what} is empty"));
        return None;
    };
    let kind = shape.tag_name().name();
    let visual_what = what;
    let what = format!("<{kind}> of {what}");
    let get = |key: &str,
               n: usize,
               default: Option<&str>,
               warnings: &mut Vec<String>|
     -> Option<Vec<f64>> {
        let text = match (child(shape, key).and_then(|c| c.text()), default) {
            (Some(t), _) => t.to_owned(),
            (None, Some(d)) => d.to_owned(),
            (None, None) => {
                warnings.push(format!("{what} has no <{key}>"));
                return None;
            }
        };
        match numbers(&text, n) {
            Ok(v) => Some(v),
            Err(e) => {
                warnings.push(format!("<{key}> of {what}: {e}"));
                None
            }
        }
    };
    let positive = |key: &str, v: &[f64], warnings: &mut Vec<String>| -> bool {
        if v.iter().all(|x| *x > 0.0) {
            true
        } else {
            warnings.push(format!("<{key}> of {what} must be positive"));
            false
        }
    };
    let bx = |x: f64, y: f64, z: f64| Bounds {
        min: [-x, -y, -z],
        max: [x, y, z],
    };
    match kind {
        "box" => {
            let s = get("size", 3, None, warnings)?;
            positive("size", &s, warnings).then(|| bx(s[0] / 2.0, s[1] / 2.0, s[2] / 2.0))
        }
        "cylinder" | "capsule" => {
            let r = get("radius", 1, None, warnings);
            let l = get("length", 1, None, warnings);
            let (r, l) = (r?[0], l?[0]);
            if !positive("radius", &[r], warnings) || !(l >= 0.0) {
                if l < 0.0 {
                    warnings.push(format!("<length> of {what} must not be negative"));
                }
                return None;
            }
            let half = if kind == "capsule" {
                l / 2.0 + r
            } else {
                l / 2.0
            };
            Some(bx(r, r, half))
        }
        "sphere" => {
            let r = get("radius", 1, None, warnings)?;
            positive("radius", &r, warnings).then(|| bx(r[0], r[0], r[0]))
        }
        "ellipsoid" => {
            let r = get("radii", 3, None, warnings)?;
            positive("radii", &r, warnings).then(|| bx(r[0], r[1], r[2]))
        }
        "plane" => {
            let n = get("normal", 3, Some("0 0 1"), warnings);
            let s = get("size", 2, None, warnings);
            let (n, s) = (n?, s?);
            if !positive("size", &s, warnings) {
                return None;
            }
            let n = Vector3::new(n[0], n[1], n[2]);
            let z = Vector3::new(0.0, 0.0, 1.0);
            let len = n.norm();
            if len == 0.0 {
                warnings.push(format!("<normal> of {what} must not be zero"));
                return None;
            }
            let axis = z.cross(&n);
            let angle = (z.dot(&n) / len).clamp(-1.0, 1.0).acos();
            let q = Quaternion::from_axis_angle(axis, angle).unwrap_or_else(|_| {
                if z.dot(&n) < 0.0 {
                    Quaternion::new(1.0, 0.0, 0.0, 0.0)
                } else {
                    Quaternion::identity()
                }
            });
            let mut b = Bounds::empty();
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let p = q.rotate(&Vector3::new(x * s[0] / 2.0, y * s[1] / 2.0, 0.0));
                b.add([p.x, p.y, p.z]);
            }
            Some(b)
        }
        "mesh" => {
            warnings.push(format!(
                "{what} is not shown in the Web UI for now (docs/spec/container-model.md §3.2.1)"
            ));
            match child(shape, "uri").and_then(|u| u.text()).map(str::trim) {
                None | Some("") => warnings.push(format!("{what} has no <uri>")),
                Some(uri) if uri.contains("://") => warnings.push(format!(
                    "<uri> of {what}: '{uri}' is not supported. Write a path relative to the .sdf"
                )),
                Some(uri) => {
                    let p = dir.join(uri);
                    let ext = ModelFormat::from_path(&p);
                    if !p.is_file() {
                        warnings.push(format!("<uri> of {what}: '{uri}' is not found"));
                    } else if !matches!(ext, ModelFormat::Glb | ModelFormat::Stl) {
                        warnings.push(format!("<uri> of {what}: '{uri}' should be a .glb or .stl"));
                    }
                }
            }
            if child(shape, "scale").is_some() {
                let _ = get("scale", 3, None, warnings);
            }
            None
        }
        other => {
            warnings.push(format!(
                "unknown geometry <{other}> in {visual_what} is not shown (box, cylinder, sphere, capsule, ellipsoid, plane, mesh)"
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests;
