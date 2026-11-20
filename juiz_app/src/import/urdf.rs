//! URDF を読む (container-model.md §13.1)

use std::collections::HashMap;

use juiz_sdk::geometry::Transform;

use super::robot::{child, children, numbers, rpy, Geometry, Joint, Link, Robot, Visual};

fn vec3(
    node: roxmltree::Node,
    key: &str,
    default: [f64; 3],
    what: &str,
    warnings: &mut Vec<String>,
) -> [f64; 3] {
    match node.attribute(key) {
        None => default,
        Some(t) => match numbers(t, 3) {
            Ok(v) => [v[0], v[1], v[2]],
            Err(e) => {
                warnings.push(format!("{key} of {what}: {e}. It is ignored"));
                default
            }
        },
    }
}

/// `<origin xyz rpy>` (無ければ単位)
fn origin(node: roxmltree::Node, what: &str, warnings: &mut Vec<String>) -> Transform {
    match child(node, "origin") {
        None => Transform::identity(),
        Some(o) => rpy(
            vec3(o, "xyz", [0.0; 3], what, warnings),
            vec3(o, "rpy", [0.0; 3], what, warnings),
        ),
    }
}

fn number(node: roxmltree::Node, key: &str, what: &str) -> Result<f64, String> {
    let t = node
        .attribute(key)
        .ok_or_else(|| format!("{what} has no '{key}'"))?;
    t.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("'{key}' of {what} is not a number: '{t}'"))
}

/// `<color rgba>` を持つ `<material>` (直下か、名前で robot の直下のもの)
fn color(
    material: Option<roxmltree::Node>,
    named: &HashMap<String, [f64; 4]>,
    what: &str,
    warnings: &mut Vec<String>,
) -> Option<[f64; 4]> {
    let m = material?;
    if let Some(c) = child(m, "color") {
        return match c.attribute("rgba").map(|t| numbers(t, 4)) {
            Some(Ok(v)) => Some([v[0], v[1], v[2], v[3]]),
            Some(Err(e)) => {
                warnings.push(format!("<color rgba> of {what}: {e}. It is ignored"));
                None
            }
            None => None,
        };
    }
    let name = m.attribute("name")?;
    match named.get(name) {
        Some(c) => Some(*c),
        None => {
            // テクスチャだけの material もあるので警告だけ
            if child(m, "texture").is_none() {
                warnings.push(format!("material '{name}' of {what} has no color"));
            }
            None
        }
    }
}

fn geometry(g: roxmltree::Node, what: &str) -> Result<Geometry, String> {
    let shape = g
        .children()
        .find(|n| n.is_element())
        .ok_or_else(|| format!("<geometry> of {what} is empty"))?;
    let w = format!("<{}> of {what}", shape.tag_name().name());
    Ok(match shape.tag_name().name() {
        "box" => {
            let v = numbers(shape.attribute("size").unwrap_or_default(), 3)
                .map_err(|e| format!("size of {w}: {e}"))?;
            Geometry::Box([v[0], v[1], v[2]])
        }
        "cylinder" => Geometry::Cylinder {
            radius: number(shape, "radius", &w)?,
            length: number(shape, "length", &w)?,
        },
        "sphere" => Geometry::Sphere {
            radius: number(shape, "radius", &w)?,
        },
        // ROS 2 の URDF にある (urdfdom 3)
        "capsule" => Geometry::Capsule {
            radius: number(shape, "radius", &w)?,
            length: number(shape, "length", &w)?,
        },
        "mesh" => {
            let uri = shape
                .attribute("filename")
                .ok_or_else(|| format!("{w} has no 'filename'"))?;
            let scale = match shape.attribute("scale") {
                None => [1.0; 3],
                Some(t) => {
                    let v = numbers(t, 3).map_err(|e| format!("scale of {w}: {e}"))?;
                    [v[0], v[1], v[2]]
                }
            };
            Geometry::Mesh {
                uri: uri.to_owned(),
                scale,
            }
        }
        other => return Err(format!("unknown geometry <{other}> of {what}")),
    })
}

pub(crate) fn parse(text: &str, warnings: &mut Vec<String>) -> Result<Robot, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not an XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "robot" {
        return Err(format!(
            "the top element of a URDF must be <robot>, but <{}>",
            root.tag_name().name()
        ));
    }
    let name = root.attribute("name").unwrap_or("robot").to_owned();
    let mut named = HashMap::new();
    for m in children(root, "material") {
        if let (Some(n), Some(c)) = (m.attribute("name"), child(m, "color")) {
            if let Some(Ok(v)) = c.attribute("rgba").map(|t| numbers(t, 4)) {
                named.insert(n.to_owned(), [v[0], v[1], v[2], v[3]]);
            }
        }
    }
    if root
        .descendants()
        .any(|n| n.is_element() && n.tag_name().name() == "xacro:macro")
        || text.contains("xmlns:xacro")
    {
        warnings.push(
            "this looks like a xacro file. Run xacro first (xacro robot.urdf.xacro > robot.urdf)"
                .to_owned(),
        );
    }
    let mut links = Vec::new();
    for l in children(root, "link") {
        let lname = l
            .attribute("name")
            .ok_or("a <link> has no name")?
            .to_owned();
        let mut visuals = Vec::new();
        for (i, v) in children(l, "visual").enumerate() {
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
            visuals.push(Visual {
                origin: origin(v, &what, warnings),
                color: color(child(v, "material"), &named, &what, warnings),
                name: vname,
                geometry,
            });
        }
        links.push(Link {
            name: lname,
            visuals,
        });
    }
    let mut joints = Vec::new();
    for j in children(root, "joint") {
        let jname = j
            .attribute("name")
            .ok_or("a <joint> has no name")?
            .to_owned();
        let what = format!("joint '{jname}'");
        let link_of = |key: &str| -> Result<String, String> {
            child(j, key)
                .and_then(|n| n.attribute("link"))
                .map(|s| s.to_owned())
                .ok_or_else(|| format!("{what} has no <{key} link>"))
        };
        let kind = j.attribute("type").unwrap_or("fixed").to_owned();
        let axis = if !matches!(kind.as_str(), "revolute" | "continuous" | "prismatic") {
            None
        } else {
            Some(match child(j, "axis") {
                None => [1.0, 0.0, 0.0],
                Some(a) => vec3(a, "xyz", [1.0, 0.0, 0.0], &what, warnings),
            })
        };
        let limit =
            child(j, "limit").and_then(|l| match (l.attribute("lower"), l.attribute("upper")) {
                (None, None) => None,
                (lo, up) => Some((
                    lo.and_then(|t| t.trim().parse().ok()).unwrap_or(0.0),
                    up.and_then(|t| t.trim().parse().ok()).unwrap_or(0.0),
                )),
            });
        if child(j, "mimic").is_some() {
            warnings.push(format!("<mimic> of {what} is not imported"));
        }
        joints.push(Joint {
            parent: link_of("parent")?,
            child: link_of("child")?,
            origin: origin(j, &what, warnings),
            name: jname,
            kind,
            axis,
            limit,
        });
    }
    let mut robot = Robot {
        name,
        links,
        joints,
        root_poses: Vec::new(),
        model_pose: Transform::identity(),
    };
    robot.check()?;
    robot.root_poses = robot
        .roots()
        .into_iter()
        .map(|r| (r.to_owned(), Transform::identity()))
        .collect();
    Ok(robot)
}
