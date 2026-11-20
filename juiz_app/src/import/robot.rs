//! URDF と SDF に共通の中間の形

use juiz_sdk::geometry::{Quaternion, Transform, Vector3};

/// 基本形とメッシュ (container-model.md §3.2 の部分集合と同じ)
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Geometry {
    Box([f64; 3]),
    Cylinder {
        radius: f64,
        length: f64,
    },
    Sphere {
        radius: f64,
    },
    Capsule {
        radius: f64,
        length: f64,
    },
    Ellipsoid([f64; 3]),
    Plane {
        normal: [f64; 3],
        size: [f64; 2],
    },
    /// `uri` は元の書き方 (package:// など)、`scale` は軸ごとの倍率
    Mesh {
        uri: String,
        scale: [f64; 3],
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Visual {
    pub name: String,
    /// link から見た位置姿勢
    pub origin: Transform,
    pub geometry: Geometry,
    /// RGBA (0〜1)
    pub color: Option<[f64; 4]>,
}

#[derive(Debug, Clone)]
pub(crate) struct Link {
    pub name: String,
    pub visuals: Vec<Visual>,
}

#[derive(Debug, Clone)]
pub(crate) struct Joint {
    pub name: String,
    /// fixed・revolute・continuous・prismatic・floating・planar・ball など
    pub kind: String,
    pub parent: String,
    pub child: String,
    /// 関節の値が 0 のときの parent → child の変換
    pub origin: Transform,
    pub axis: Option<[f64; 3]>,
    pub limit: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub(crate) struct Robot {
    pub name: String,
    pub links: Vec<Link>,
    pub joints: Vec<Joint>,
    /// 根の link (どの joint の child でもないもの) の、根の座標 (URDF は単位、SDF は model の座標) から見た位置。
    /// `--anchor` か根が複数あるときの Anchor → 根の Transform に使う
    pub root_poses: Vec<(String, Transform)>,
    /// model の `<pose>` (SDF だけ。`--anchor` のときに Anchor → 根に掛ける)
    pub model_pose: Transform,
}

pub(crate) fn rpy(xyz: [f64; 3], rpy: [f64; 3]) -> Transform {
    Transform::new(
        Vector3::new(xyz[0], xyz[1], xyz[2]),
        Quaternion::from_rpy(rpy[0], rpy[1], rpy[2]),
    )
}

/// 空白区切りの数 `n` 個。読めなければ理由
pub(crate) fn numbers(text: &str, n: usize) -> Result<Vec<f64>, String> {
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

pub(crate) fn child<'a, 'i>(
    node: roxmltree::Node<'a, 'i>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

pub(crate) fn children<'a, 'i>(
    node: roxmltree::Node<'a, 'i>,
    name: &'static str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}

impl Robot {
    /// どの joint の child でもない link
    pub fn roots(&self) -> Vec<&str> {
        self.links
            .iter()
            .map(|l| l.name.as_str())
            .filter(|n| !self.joints.iter().any(|j| j.child == *n))
            .collect()
    }

    /// 木の形を確かめる: joint の端点の link があり、child は 1 度だけ、循環が無い
    pub fn check(&self) -> Result<(), String> {
        if self.links.is_empty() {
            return Err(format!("'{}' has no link", self.name));
        }
        let has = |n: &str| self.links.iter().any(|l| l.name == n);
        for (i, l) in self.links.iter().enumerate() {
            if self.links[..i].iter().any(|o| o.name == l.name) {
                return Err(format!("link '{}' is defined twice", l.name));
            }
        }
        for j in &self.joints {
            for end in [&j.parent, &j.child] {
                if !has(end) {
                    return Err(format!(
                        "joint '{}' refers to an unknown link '{end}'",
                        j.name
                    ));
                }
            }
            if self.joints.iter().filter(|o| o.child == j.child).count() > 1 {
                return Err(format!(
                    "link '{}' is the child of several joints (not a tree)",
                    j.child
                ));
            }
        }
        if self.roots().is_empty() {
            return Err("the joints make a cycle (no root link)".to_owned());
        }
        // 根から届かない link があれば循環
        let mut reached: Vec<&str> = self.roots();
        let mut i = 0;
        while i < reached.len() {
            let p = reached[i];
            for j in self.joints.iter().filter(|j| j.parent == p) {
                if !reached.contains(&j.child.as_str()) {
                    reached.push(&j.child);
                }
            }
            i += 1;
        }
        if reached.len() != self.links.len() {
            return Err("the joints make a cycle".to_owned());
        }
        Ok(())
    }
}
