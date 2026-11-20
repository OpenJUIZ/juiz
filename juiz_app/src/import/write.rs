//! 中間の形から conf の YAML とモデルのファイルを作る (container-model.md §13.1)

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use juiz_sdk::geometry::Transform;
use juiz_sdk::manifests::{ModelFormat, FRAME_TYPE_NAME, STATIC_ANCHOR_TYPE_NAME};

use super::robot::{Geometry, Link, Robot, Visual};
use super::{ImportArgs, ImportOutput, Kind, Resolver};

/// juiz の名前に使えない文字 (`/` `:` `|`) を `_` にする
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if matches!(c, '/' | ':' | '|') { '_' } else { c })
        .collect();
    if s.is_empty() {
        "_".to_owned()
    } else {
        s
    }
}

/// `..` と `.` を字面で畳む (ファイルは見ない)
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `to` を `from_dir` からの相対パス (`/` 区切り) にする。根が違えば絶対パスのまま
pub(crate) fn relative(from_dir: &Path, to: &Path) -> String {
    let from = normalize(&std::path::absolute(from_dir).unwrap_or_else(|_| from_dir.to_path_buf()));
    let to = normalize(&std::path::absolute(to).unwrap_or_else(|_| to.to_path_buf()));
    let fc: Vec<_> = from.components().collect();
    let tc: Vec<_> = to.components().collect();
    let common = fc.iter().zip(tc.iter()).take_while(|(a, b)| a == b).count();
    let same_root =
        common > 0 && !matches!(fc.first(), Some(Component::Prefix(_)) if fc.first() != tc.first());
    if !same_root {
        return to.to_string_lossy().replace('\\', "/");
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..fc.len() {
        parts.push("..".to_owned());
    }
    for c in &tc[common..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn q(s: &str) -> String {
    juiz_core::prelude::serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

/// 数の表し方 (1e-12 未満は 0)
fn n(v: f64) -> String {
    let v = if v.abs() < 1e-12 { 0.0 } else { v };
    let s = format!("{v}");
    if s == "-0" {
        "0".to_owned()
    } else {
        s
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex_color(c: [f64; 4]) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if (c[3] - 1.0).abs() < 1e-9 {
        format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            b(c[0]),
            b(c[1]),
            b(c[2]),
            b(c[3])
        )
    }
}

fn is_identity(t: &Transform) -> bool {
    t.approx_eq(&Transform::identity(), 1e-12)
}

fn write_transform(out: &mut String, indent: &str, t: &Transform) {
    let tr = &t.translation;
    let r = &t.rotation;
    let _ = writeln!(
        out,
        "{indent}translation: {{x: {}, y: {}, z: {}}}",
        n(tr.x),
        n(tr.y),
        n(tr.z)
    );
    let _ = writeln!(
        out,
        "{indent}rotation: {{x: {}, y: {}, z: {}, w: {}}}",
        n(r.x),
        n(r.y),
        n(r.z),
        n(r.w)
    );
}

fn pose_text(t: &Transform) -> String {
    let (r, p, y) = t.rotation.to_rpy();
    format!(
        "{} {} {} {} {} {}",
        n(t.translation.x),
        n(t.translation.y),
        n(t.translation.z),
        n(r),
        n(p),
        n(y)
    )
}

/// conf の `model:` に書くもの
struct ModelSpec {
    path: PathBuf,
    scale: Option<f64>,
    origin: Transform,
    color: Option<String>,
}

struct Builder<'a> {
    resolver: &'a Resolver,
    models_dir: PathBuf,
    copy_meshes: bool,
    files: Vec<(PathBuf, Vec<u8>)>,
    /// 元 → 写す先
    copies: HashMap<PathBuf, PathBuf>,
    warnings: Vec<String>,
}

impl Builder<'_> {
    /// メッシュを解決し、`--copy-meshes` なら写す先を決める。使うパス (絶対) を返す
    fn mesh_path(&mut self, uri: &str, what: &str) -> Option<PathBuf> {
        let found = match self.resolver.resolve(uri) {
            Ok(p) => normalize(&std::path::absolute(&p).unwrap_or(p)),
            Err(e) => {
                self.warnings.push(format!("mesh of {what}: {e}"));
                return None;
            }
        };
        if !self.copy_meshes {
            return Some(found);
        }
        if let Some(to) = self.copies.get(&found) {
            return Some(to.clone());
        }
        let file_name = found
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mesh".to_owned());
        let dir = self.models_dir.join("meshes");
        let mut to = dir.join(&file_name);
        let mut i = 1;
        while self.copies.values().any(|v| *v == to) {
            let p = Path::new(&file_name);
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = p
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            to = dir.join(format!("{stem}_{i}{ext}"));
            i += 1;
        }
        if ModelFormat::from_path(&found) == ModelFormat::Gltf {
            self.warnings.push(format!(
                "mesh of {what} is a .gltf. Its external files (buffers / images) are not copied"
            ));
        }
        self.copies.insert(found, to.clone());
        Some(to)
    }

    /// link のモデル。visual が無ければ None
    fn link_model(&mut self, link: &Link, file_stem: &str) -> Option<ModelSpec> {
        if link.visuals.is_empty() {
            return None;
        }
        let what = format!("link '{}'", link.name);
        // visual が 1 つでメッシュなら、そのファイルを直接指す
        if let [Visual {
            geometry: Geometry::Mesh { uri, scale },
            origin,
            color,
            ..
        }] = link.visuals.as_slice()
        {
            let uniform =
                (scale[0] - scale[1]).abs() < 1e-12 && (scale[0] - scale[2]).abs() < 1e-12;
            if uniform {
                let path = self.mesh_path(uri, &what)?;
                let format = ModelFormat::from_path(&path);
                if format == ModelFormat::Other || format == ModelFormat::Sdf {
                    self.warnings.push(format!(
                        "mesh of {what} ({}) is not shown in the Web UI (only .glb / .gltf / .stl). \
                         Convert it to .glb to see it",
                        path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()
                    ));
                }
                return Some(ModelSpec {
                    path,
                    scale: (scale[0] != 1.0).then_some(scale[0]),
                    origin: *origin,
                    // glTF は色を持つので使わない (§5.1)
                    color: color
                        .filter(|_| !matches!(format, ModelFormat::Glb | ModelFormat::Gltf))
                        .map(hex_color),
                });
            }
            self.warnings.push(format!(
                "mesh of {what} has a non-uniform scale {:?}; it is written in an SDF (not shown in the Web UI)",
                scale
            ));
        }
        // それ以外は link ごとの SDF (§3.2 の部分集合)
        let path = self.models_dir.join(format!("{file_stem}.sdf"));
        let sdf_dir = self.models_dir.clone();
        let mut s = String::new();
        let _ = writeln!(s, "<?xml version=\"1.0\"?>");
        let _ = writeln!(
            s,
            "<!-- juiz import: visuals of link '{}' -->",
            xml(&link.name).replace("--", "- -")
        );
        let _ = writeln!(s, "<sdf version=\"1.9\">");
        let _ = writeln!(s, "  <model name=\"{}\">", xml(&link.name));
        let _ = writeln!(s, "    <link name=\"{}\">", xml(&link.name));
        let mut has_mesh = false;
        for v in &link.visuals {
            let _ = writeln!(s, "      <visual name=\"{}\">", xml(&v.name));
            if !is_identity(&v.origin) {
                let _ = writeln!(s, "        <pose>{}</pose>", pose_text(&v.origin));
            }
            let g = match &v.geometry {
                Geometry::Box(b) => format!(
                    "<box><size>{} {} {}</size></box>",
                    n(b[0]),
                    n(b[1]),
                    n(b[2])
                ),
                Geometry::Cylinder { radius, length } => format!(
                    "<cylinder><radius>{}</radius><length>{}</length></cylinder>",
                    n(*radius),
                    n(*length)
                ),
                Geometry::Sphere { radius } => {
                    format!("<sphere><radius>{}</radius></sphere>", n(*radius))
                }
                Geometry::Capsule { radius, length } => format!(
                    "<capsule><radius>{}</radius><length>{}</length></capsule>",
                    n(*radius),
                    n(*length)
                ),
                Geometry::Ellipsoid(r) => format!(
                    "<ellipsoid><radii>{} {} {}</radii></ellipsoid>",
                    n(r[0]),
                    n(r[1]),
                    n(r[2])
                ),
                Geometry::Plane { normal, size } => format!(
                    "<plane><normal>{} {} {}</normal><size>{} {}</size></plane>",
                    n(normal[0]),
                    n(normal[1]),
                    n(normal[2]),
                    n(size[0]),
                    n(size[1])
                ),
                Geometry::Mesh { uri, scale } => {
                    has_mesh = true;
                    let w = format!("visual '{}' of {what}", v.name);
                    let u = match self.mesh_path(uri, &w) {
                        Some(p) => relative(&sdf_dir, &p),
                        None => uri.clone(),
                    };
                    format!(
                        "<mesh><uri>{}</uri><scale>{} {} {}</scale></mesh>",
                        xml(&u),
                        n(scale[0]),
                        n(scale[1]),
                        n(scale[2])
                    )
                }
            };
            let _ = writeln!(s, "        <geometry>{g}</geometry>");
            if let Some(c) = v.color {
                let _ = writeln!(
                    s,
                    "        <material><diffuse>{} {} {} {}</diffuse></material>",
                    n(c[0]),
                    n(c[1]),
                    n(c[2]),
                    n(c[3])
                );
            }
            let _ = writeln!(s, "      </visual>");
        }
        let _ = writeln!(s, "    </link>");
        let _ = writeln!(s, "  </model>");
        let _ = writeln!(s, "</sdf>");
        if has_mesh {
            self.warnings.push(format!(
                "{what} has a mesh together with other visuals. Meshes in an SDF are not shown in the Web UI \
                 (docs/spec/container-model.md §3.2.1)"
            ));
        }
        self.files.push((path.clone(), s.into_bytes()));
        Some(ModelSpec {
            path,
            scale: None,
            origin: Transform::identity(),
            color: None,
        })
    }
}

pub(crate) fn build(
    robot: &Robot,
    kind: Kind,
    resolver: &Resolver,
    conf_dir: &Path,
    args: &ImportArgs,
    mut warnings: Vec<String>,
) -> Result<ImportOutput, String> {
    if let Some(ns) = &args.namespace {
        juiz_sdk::identifier::validate_namespace(ns).map_err(|e| format!("{e}"))?;
    }
    // 名前 (juiz で使えない文字を _ に、重なれば番号を足す)
    let mut names: HashMap<String, String> = HashMap::new();
    let mut used: Vec<String> = Vec::new();
    for l in &robot.links {
        let mut s = sanitize(&l.name);
        if s != l.name {
            warnings.push(format!(
                "link '{}' is renamed to '{s}' ('/', ':' and '|' can not be used)",
                l.name
            ));
        }
        let base = s.clone();
        let mut i = 1;
        while used.contains(&s) {
            s = format!("{base}_{i}");
            i += 1;
        }
        used.push(s.clone());
        names.insert(l.name.clone(), s);
    }
    let roots = robot.roots();
    let anchor: Option<String> = match &args.anchor {
        Some(a) => {
            juiz_sdk::identifier::validate_name(a).map_err(|e| format!("--anchor: {e}"))?;
            if used.contains(a) {
                return Err(format!("--anchor '{a}' is the same as a link name"));
            }
            Some(a.clone())
        }
        None if roots.len() > 1 => {
            let mut a = sanitize(&robot.name);
            while used.contains(&a) {
                a.push_str("_model");
            }
            warnings.push(format!(
                "there are {} root links ({}); a static_anchor '{a}' is added above them",
                roots.len(),
                roots.join(", ")
            ));
            Some(a)
        }
        None => None,
    };
    let models_dir = match &args.models_dir {
        Some(d) => conf_dir.join(d),
        None => conf_dir.join("models").join(sanitize(&robot.name)),
    };
    let models_dir = normalize(&models_dir);
    let mut b = Builder {
        resolver,
        models_dir,
        copy_meshes: args.copy_meshes,
        files: Vec::new(),
        copies: HashMap::new(),
        warnings,
    };
    let full = |name: &str| -> String {
        match &args.namespace {
            Some(ns) if !ns.is_empty() => format!("{ns}/{name}"),
            _ => name.to_owned(),
        }
    };

    let mut out = String::new();
    let source = Path::new(&args.file)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind_name = match kind {
        Kind::Urdf => "urdf",
        Kind::Sdf => "sdf",
    };
    let _ = writeln!(
        out,
        "# juiz import {kind_name} {source} (docs/spec/container-model.md §13.1)"
    );
    let _ = writeln!(out, "# robot: {}", robot.name.replace('\n', " "));
    let _ = writeln!(
        out,
        "# link -> Container ({FRAME_TYPE_NAME} / {STATIC_ANCHOR_TYPE_NAME}), joint -> Transform (value at joint position 0), visual -> model"
    );
    let _ = writeln!(out, "containers:");
    let ns_line = |out: &mut String| {
        if let Some(ns) = args.namespace.as_deref().filter(|s| !s.is_empty()) {
            let _ = writeln!(out, "    namespace: {}", q(ns));
        }
    };
    if let Some(a) = &anchor {
        let _ = writeln!(out, "  - type_name: {STATIC_ANCHOR_TYPE_NAME}");
        let _ = writeln!(out, "    name: {}", q(a));
        ns_line(&mut out);
    }
    for l in &robot.links {
        let name = &names[&l.name];
        let type_name = if anchor.is_none() && roots.contains(&l.name.as_str()) {
            STATIC_ANCHOR_TYPE_NAME
        } else {
            FRAME_TYPE_NAME
        };
        let _ = writeln!(out, "  - type_name: {type_name}");
        let _ = writeln!(out, "    name: {}", q(name));
        ns_line(&mut out);
        if let Some(m) = b.link_model(l, name) {
            let _ = writeln!(out, "    model:");
            let _ = writeln!(out, "      path: {}", q(&relative(conf_dir, &m.path)));
            if let Some(s) = m.scale {
                let _ = writeln!(out, "      scale: {}", n(s));
            }
            if !is_identity(&m.origin) {
                let _ = writeln!(out, "      origin:");
                write_transform(&mut out, "        ", &m.origin);
            }
            if let Some(c) = &m.color {
                let _ = writeln!(out, "      color: {}", q(c));
            }
        }
    }
    let _ = writeln!(out, "transforms:");
    if let Some(a) = &anchor {
        for (r, p) in &robot.root_poses {
            // --anchor のときは model の位置も掛ける。自動で足した Anchor は model の座標そのもの
            let t = if args.anchor.is_some() {
                robot.model_pose.compose(p)
            } else {
                *p
            };
            let _ = writeln!(out, "  - source: {}", q(&full(a)));
            let _ = writeln!(out, "    destination: {}", q(&full(&names[r])));
            if !is_identity(&t) {
                write_transform(&mut out, "    ", &t);
            }
        }
    }
    for j in &robot.joints {
        let mut c = format!("joint {}: {}", q(&j.name), j.kind);
        if let Some(a) = j.axis {
            let _ = write!(c, ", axis [{}, {}, {}]", n(a[0]), n(a[1]), n(a[2]));
        }
        if let Some((lo, up)) = j.limit {
            let _ = write!(c, ", limit [{}, {}]", n(lo), n(up));
        }
        let _ = writeln!(out, "  # {c}");
        let _ = writeln!(out, "  - source: {}", q(&full(&names[&j.parent])));
        let _ = writeln!(out, "    destination: {}", q(&full(&names[&j.child])));
        if !is_identity(&j.origin) {
            write_transform(&mut out, "    ", &j.origin);
        }
    }
    if robot.joints.is_empty() && anchor.is_none() {
        // 空の配列にする (YAML の null を避ける)
        out = out.replace("transforms:\n", "transforms: []\n");
    }
    let mut copies: Vec<(PathBuf, PathBuf)> = b.copies.into_iter().collect();
    copies.sort();
    Ok(ImportOutput {
        conf: out,
        files: b.files,
        copies,
        warnings: b.warnings,
    })
}
