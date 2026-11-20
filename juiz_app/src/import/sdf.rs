//! 複数 link・joint の SDF を読む (container-model.md §13.1)

use std::collections::HashMap;

use juiz_sdk::geometry::{Quaternion, Transform, Vector3};

use super::robot::{child, children, numbers, Geometry, Joint, Link, Robot, Visual};

/// `<pose>` (無ければ単位) と relative_to
fn pose(
    node: roxmltree::Node,
    what: &str,
    warnings: &mut Vec<String>,
) -> (Transform, Option<String>) {
    let Some(p) = child(node, "pose") else {
        return (Transform::identity(), None);
    };
    let relative_to = p
        .attribute("relative_to")
        .filter(|r| !r.is_empty())
        .map(|r| r.to_owned());
    let v = match numbers(p.text().unwrap_or_default(), 6) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!(
                "<pose> of {what}: {e} (x y z roll pitch yaw). It is ignored"
            ));
            return (Transform::identity(), relative_to);
        }
    };
    let k = if p.attribute("degrees") == Some("true") {
        std::f64::consts::PI / 180.0
    } else {
        1.0
    };
    (
        Transform::new(
            Vector3::new(v[0], v[1], v[2]),
            Quaternion::from_rpy(v[3] * k, v[4] * k, v[5] * k),
        ),
        relative_to,
    )
}

fn text_numbers(
    node: roxmltree::Node,
    key: &str,
    n: usize,
    what: &str,
) -> Result<Vec<f64>, String> {
    let c = child(node, key).ok_or_else(|| format!("{what} has no <{key}>"))?;
    numbers(c.text().unwrap_or_default(), n).map_err(|e| format!("<{key}> of {what}: {e}"))
}

fn geometry(g: roxmltree::Node, what: &str) -> Result<Geometry, String> {
    let shape = g
        .children()
        .find(|n| n.is_element())
        .ok_or_else(|| format!("<geometry> of {what} is empty"))?;
    let w = format!("<{}> of {what}", shape.tag_name().name());
    let one = |key: &str| -> Result<f64, String> { Ok(text_numbers(shape, key, 1, &w)?[0]) };
    Ok(match shape.tag_name().name() {
        "box" => {
            let v = text_numbers(shape, "size", 3, &w)?;
            Geometry::Box([v[0], v[1], v[2]])
        }
        "cylinder" => Geometry::Cylinder {
            radius: one("radius")?,
            length: one("length")?,
        },
        "sphere" => Geometry::Sphere {
            radius: one("radius")?,
        },
        "capsule" => Geometry::Capsule {
            radius: one("radius")?,
            length: one("length")?,
        },
        "ellipsoid" => {
            let v = text_numbers(shape, "radii", 3, &w)?;
            Geometry::Ellipsoid([v[0], v[1], v[2]])
        }
        "plane" => {
            let normal = match child(shape, "normal") {
                Some(_) => {
                    let v = text_numbers(shape, "normal", 3, &w)?;
                    [v[0], v[1], v[2]]
                }
                None => [0.0, 0.0, 1.0],
            };
            let v = text_numbers(shape, "size", 2, &w)?;
            Geometry::Plane {
                normal,
                size: [v[0], v[1]],
            }
        }
        "mesh" => {
            let uri = child(shape, "uri")
                .and_then(|u| u.text())
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .ok_or_else(|| format!("{w} has no <uri>"))?;
            let scale = match child(shape, "scale") {
                Some(_) => {
                    let v = text_numbers(shape, "scale", 3, &w)?;
                    [v[0], v[1], v[2]]
                }
                None => [1.0; 3],
            };
            Geometry::Mesh { uri, scale }
        }
        other => return Err(format!("unknown geometry <{other}> of {what}")),
    })
}

fn color(visual: roxmltree::Node, what: &str, warnings: &mut Vec<String>) -> Option<[f64; 4]> {
    let m = child(visual, "material")?;
    for key in ["diffuse", "ambient"] {
        if let Some(c) = child(m, key) {
            let t = c.text().unwrap_or_default();
            let n = t.split_whitespace().count();
            match numbers(t, n) {
                Ok(v) if n == 3 || n == 4 => {
                    return Some([v[0], v[1], v[2], if n == 4 { v[3] } else { 1.0 }])
                }
                _ => warnings.push(format!(
                    "<{key}> of {what} must be 3 or 4 numbers, but '{}'",
                    t.trim()
                )),
            }
        }
    }
    None
}

/// 取り込む `<model>`: `<sdf>` の直下か、`<world>` の中にちょうど 1 つ
fn find_model<'a, 'i>(root: roxmltree::Node<'a, 'i>) -> Result<roxmltree::Node<'a, 'i>, String> {
    if root.tag_name().name() != "sdf" {
        return Err(format!(
            "the top element of an SDF must be <sdf>, but <{}>",
            root.tag_name().name()
        ));
    }
    let direct: Vec<_> = children(root, "model").collect();
    let in_world: Vec<_> = children(root, "world")
        .flat_map(|w| children(w, "model"))
        .collect();
    let all: Vec<_> = direct.iter().chain(in_world.iter()).copied().collect();
    match all.len() {
        1 => Ok(all[0]),
        0 => Err("no <model> in the SDF (<include> of another file is not supported)".to_owned()),
        n => Err(format!(
            "the SDF has {n} models ({}). Import one model per file",
            all.iter()
                .map(|m| m.attribute("name").unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

pub(crate) fn parse(text: &str, warnings: &mut Vec<String>) -> Result<Robot, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not an XML: {e}"))?;
    let model = find_model(doc.root_element())?;
    let name = model.attribute("name").unwrap_or("model").to_owned();
    let (model_pose, _) = pose(model, &format!("model '{name}'"), warnings);
    for n in children(model, "model").chain(children(model, "include")) {
        warnings.push(format!(
            "nested <{}> '{}' is skipped",
            n.tag_name().name(),
            n.attribute("name").unwrap_or_default()
        ));
    }
    if child(model, "frame").is_some() {
        warnings.push("<frame> elements are not imported".to_owned());
    }
    // link の model の座標から見た位置 (relative_to は空・model・別の link を解く)
    let link_nodes: Vec<_> = children(model, "link").collect();
    let mut raw: HashMap<String, (Transform, Option<String>)> = HashMap::new();
    let mut order = Vec::new();
    for l in &link_nodes {
        let lname = l
            .attribute("name")
            .ok_or("a <link> has no name")?
            .to_owned();
        let p = pose(*l, &format!("link '{lname}'"), warnings);
        raw.insert(lname.clone(), p);
        order.push(lname);
    }
    let mut in_model: HashMap<String, Transform> = HashMap::new();
    fn resolve(
        n: &str,
        model: &str,
        raw: &HashMap<String, (Transform, Option<String>)>,
        done: &mut HashMap<String, Transform>,
        depth: usize,
        warnings: &mut Vec<String>,
    ) -> Transform {
        if let Some(t) = done.get(n) {
            return *t;
        }
        let (t, rel) = raw[n].clone();
        let t = match rel.as_deref() {
            None => t,
            Some(r) if r == model || r == "__model__" => t,
            Some(r) if raw.contains_key(r) && depth < 64 => {
                resolve(r, model, raw, done, depth + 1, warnings).compose(&t)
            }
            Some(r) => {
                warnings.push(format!(
                    "<pose relative_to=\"{r}\"> of link '{n}' is not supported. The model frame is used"
                ));
                t
            }
        };
        done.insert(n.to_owned(), t);
        t
    }
    for n in &order {
        resolve(n, &name, &raw, &mut in_model, 0, warnings);
    }
    let mut links = Vec::new();
    for l in &link_nodes {
        let lname = l.attribute("name").unwrap_or_default().to_owned();
        let mut visuals = Vec::new();
        for (i, v) in children(*l, "visual").enumerate() {
            let vname = v
                .attribute("name")
                .map(|s| s.to_owned())
                .unwrap_or_else(|| format!("visual_{i}"));
            let what = format!("visual '{vname}' of link '{lname}'");
            let Some(g) = child(v, "geometry") else {
                warnings.push(format!("{what} has no <geometry>. It is skipped"));
                continue;
            };
            let geometry = match geometry(g, &what) {
                Ok(g) => g,
                Err(e) => {
                    warnings.push(format!("{e}. It is skipped"));
                    continue;
                }
            };
            let (origin, rel) = pose(v, &what, warnings);
            if let Some(r) = rel.filter(|r| *r != lname) {
                warnings.push(format!(
                    "<pose relative_to=\"{r}\"> of {what} is not supported. The link frame is used"
                ));
            }
            visuals.push(Visual {
                color: color(v, &what, warnings),
                name: vname,
                origin,
                geometry,
            });
        }
        links.push(Link {
            name: lname,
            visuals,
        });
    }
    let mut joints = Vec::new();
    for j in children(model, "joint") {
        let jname = j
            .attribute("name")
            .ok_or("a <joint> has no name")?
            .to_owned();
        let what = format!("joint '{jname}'");
        let end = |key: &str| -> Result<String, String> {
            child(j, key)
                .and_then(|n| n.text())
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .ok_or_else(|| format!("{what} has no <{key}>"))
        };
        let parent = end("parent")?;
        let child_link = end("child")?;
        if parent == "world" {
            // world に付いた link は根として扱う (§13.1)
            continue;
        }
        let kind = j.attribute("type").unwrap_or("fixed").to_owned();
        let axis_node = child(j, "axis");
        let axis = axis_node.and_then(|a| child(a, "xyz")).and_then(|x| {
            numbers(x.text().unwrap_or_default(), 3)
                .ok()
                .map(|v| [v[0], v[1], v[2]])
        });
        // 軸を持つのは 1 自由度の関節 (と revolute2・universal の 1 本目) だけ
        let axis = match kind.as_str() {
            "revolute" | "continuous" | "prismatic" | "revolute2" | "universal" | "screw" => {
                axis.or(Some([0.0, 0.0, 1.0]))
            }
            _ => None,
        };
        let limit = axis_node.and_then(|a| child(a, "limit")).and_then(|l| {
            let get = |k: &str| {
                child(l, k)
                    .and_then(|n| n.text())
                    .and_then(|t| t.trim().parse::<f64>().ok())
            };
            match (get("lower"), get("upper")) {
                (None, None) => None,
                (lo, up) => Some((lo.unwrap_or(0.0), up.unwrap_or(0.0))),
            }
        });
        let (Some(p), Some(c)) = (in_model.get(&parent), in_model.get(&child_link)) else {
            return Err(format!(
                "{what} refers to an unknown link ('{parent}' or '{child_link}')"
            ));
        };
        joints.push(Joint {
            origin: p.inverse().compose(c),
            name: jname,
            kind,
            parent,
            child: child_link,
            axis,
            limit,
        });
    }
    let mut robot = Robot {
        name,
        links,
        joints,
        root_poses: Vec::new(),
        model_pose,
    };
    robot.check()?;
    robot.root_poses = robot
        .roots()
        .into_iter()
        .map(|r| (r.to_owned(), in_model[r]))
        .collect();
    Ok(robot)
}
