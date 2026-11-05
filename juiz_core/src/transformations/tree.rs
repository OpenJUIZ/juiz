//! Anchor ごとの木 (docs/spec/transformations.md §5.2・§5.3)

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use juiz_sdk::anyhow::anyhow;
use juiz_sdk::geometry::Transform;

use crate::prelude::*;

/// 登録所が持つ Transform の値
#[derive(Debug, Clone)]
pub(super) struct TransformEntry {
    /// 始点 (親) の Container の ID
    pub source: String,
    /// 終点 (子) の Container の ID
    pub destination: String,
    pub value: Transform,
    pub stamp: juiz_sdk::geometry::Time,
    /// 版番号。登録所の中で一意 (値を変えるたびに新しい番号になる)
    pub version: u64,
    /// 取り込みで登録所が置いた、写しの Anchor B' → 写しの始点 P' の辺 (§19.3)。
    /// 値は取り込んだ pose_B(P)。Transform の一覧には出さず、書き換えられない
    pub imported: bool,
    /// System をまたぐ Transform (始点が写しの始点 P') の取り込みの設定 (§19.4)
    pub remote: Option<RemoteParams>,
    /// 端点のほかに `set_transform` で値を書いてよい Container の ID (§7.5、#410)
    pub writers: Vec<String>,
}

/// System をまたぐ Transform の取り込みの設定 (§19.4)
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RemoteParams {
    /// 周期 [Hz]
    pub rate: f64,
    /// 写しを消すまでの時間 [秒]
    pub timeout: f64,
}

/// Anchor A を根とする木。reach(A) と木での親・深さ
#[derive(Debug, Clone, Default)]
pub(super) struct Tree {
    /// 幅優先の順 (先頭が根)
    pub order: Vec<String>,
    /// Container の ID → (木での親の ID, 親 → 子の Transform の ID)。根は持たない
    pub parent: HashMap<String, (String, String)>,
    /// Container の ID → 根からの深さ (根は 0)
    pub depth: HashMap<String, usize>,
    /// 木に入っている Transform の ID
    pub transforms: HashSet<String>,
}

impl Tree {
    pub fn contains(&self, node: &str) -> bool {
        self.depth.contains_key(node)
    }

    /// `node` の子 (幅優先の順)
    pub fn children<'a>(&'a self, node: &'a str) -> impl Iterator<Item = &'a String> + 'a {
        self.order
            .iter()
            .filter(move |c| self.parent.get(*c).is_some_and(|(p, _)| p == node))
    }

    /// `descendant` が `ancestor` の子孫か (同じものは含まない)
    pub fn is_descendant(&self, descendant: &str, ancestor: &str) -> bool {
        let mut cur = descendant;
        while let Some((p, _)) = self.parent.get(cur) {
            if p == ancestor {
                return true;
            }
            cur = p;
        }
        false
    }
}

/// 始点 → [(Transform の ID, 終点)] (ID 順)
pub(super) type OutEdges<'a> = BTreeMap<&'a str, Vec<(&'a str, &'a str)>>;

pub(super) fn out_edges(transforms: &BTreeMap<String, TransformEntry>) -> OutEdges<'_> {
    let mut out: OutEdges = BTreeMap::new();
    for (tid, t) in transforms.iter() {
        out.entry(t.source.as_str())
            .or_default()
            .push((tid.as_str(), t.destination.as_str()));
    }
    out
}

/// Anchor `anchor` から届く範囲を幅優先でたどって木を作る。木でなければ `TransformTreeError` (§5.2)
pub(super) fn compute_tree(anchor: &str, out: &OutEdges) -> JuizResult<Tree> {
    let mut tree = Tree::default();
    tree.order.push(anchor.to_owned());
    tree.depth.insert(anchor.to_owned(), 0);
    let mut queue = VecDeque::from([anchor.to_owned()]);
    while let Some(node) = queue.pop_front() {
        let depth = tree.depth[&node];
        let Some(edges) = out.get(node.as_str()) else {
            continue;
        };
        for (tid, dst) in edges.iter() {
            if *dst == anchor {
                return Err(anyhow!(JuizError::TransformTreeError {
                    anchor: anchor.to_owned(),
                    container: anchor.to_owned(),
                    message: format!("Transform ({tid}) makes a cycle back to the anchor."),
                }));
            }
            if tree.contains(dst) {
                return Err(anyhow!(JuizError::TransformTreeError {
                    anchor: anchor.to_owned(),
                    container: (*dst).to_owned(),
                    message: format!(
                        "the Container is reached by two paths (from '{}' and from '{}').",
                        tree.parent
                            .get(*dst)
                            .map(|(p, _)| p.as_str())
                            .unwrap_or("?"),
                        node
                    ),
                }));
            }
            tree.order.push((*dst).to_owned());
            tree.depth.insert((*dst).to_owned(), depth + 1);
            tree.parent
                .insert((*dst).to_owned(), (node.clone(), (*tid).to_owned()));
            tree.transforms.insert((*tid).to_owned());
            queue.push_back((*dst).to_owned());
        }
    }
    Ok(tree)
}
