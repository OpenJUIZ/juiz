//! Composite の中身を CLI で表示する (issue #170)
//!
//! `juiz process list --long` などで、Composite Process の式の木と CompositeContainer のスロットを
//! 一覧の行の下に出す。中身は profile の `composition` と `slots`(仕様 docs/spec/composition.md §8.2)。

use std::collections::BTreeMap;

use juiz_core::prelude::*;
use juiz_sdk::manifests::{CompositeArgument, CompositeNode, Composition};
use juiz_sdk::process_identifier::ProcessIdentifier;

/// Composite の式の木を、1 行に 1 ノードの木の形にする。
///
/// ```text
/// filesystem_imwrite [fs: filesystem]
/// ├─ filename = $path
/// └─ img: cv_cvt_color
///    ├─ code = "BGR2GRAY"
///    └─ img: video_capture_readandget [camera: video_capture]
/// ```
///
/// 引数の値は、子の式なら `<引数名>: <子の型>`、定数なら `<引数名> = <値>`、
/// 公開した引数なら `<引数名> = $<公開名>` と書く。スロットを使うノードには `[スロット名: Container の型]` を付ける。
pub(crate) fn format_composition(composition: &Composition) -> Vec<String> {
    let mut lines = vec![node_label(&composition.body, &composition.containers)];
    push_args(&composition.body, &composition.containers, "", &mut lines);
    lines
}

fn node_label(node: &CompositeNode, containers: &BTreeMap<String, String>) -> String {
    match &node.container {
        Some(slot) => match containers.get(slot) {
            Some(type_name) => format!("{} [{slot}: {type_name}]", node.process),
            None => format!("{} [{slot}]", node.process),
        },
        None => node.process.clone(),
    }
}

fn push_args(
    node: &CompositeNode,
    containers: &BTreeMap<String, String>,
    indent: &str,
    lines: &mut Vec<String>,
) {
    let n = node.args.len();
    for (i, (name, arg)) in node.args.iter().enumerate() {
        let last = i + 1 == n;
        let branch = if last { "└─ " } else { "├─ " };
        match arg {
            CompositeArgument::Node(child) => {
                lines.push(format!(
                    "{indent}{branch}{name}: {}",
                    node_label(child, containers)
                ));
                let child_indent = format!("{indent}{}", if last { "   " } else { "│  " });
                push_args(child, containers, &child_indent, lines);
            }
            CompositeArgument::Const(v) => lines.push(format!("{indent}{branch}{name} = {v}")),
            CompositeArgument::Expose(e) => {
                lines.push(format!("{indent}{branch}{name} = ${}", e.name))
            }
        }
    }
}

/// CompositeContainer のスロットを `<スロット名> -> <参照先の Container の ID>` の行にする
pub(crate) fn format_slots(slots: &BTreeMap<String, String>) -> Vec<String> {
    slots
        .iter()
        .map(|(slot, id)| format!("{slot} -> {id}"))
        .collect()
}

/// profile に `composition` があれば木の行を返す。Composite でなければ空
pub(crate) fn composition_lines(profile: &ProcessProfile) -> Vec<String> {
    profile
        .composition
        .as_ref()
        .map(format_composition)
        .unwrap_or_default()
}

/// profile に `slots` があればスロットの行を返す。CompositeContainer でなければ空
pub(crate) fn slots_lines(profile: &ContainerProfile) -> Vec<String> {
    profile.slots.as_ref().map(format_slots).unwrap_or_default()
}

/// Process (ContainerProcess を含む) の Composite の木。profile が取れなければ、その理由を 1 行で返す
pub(crate) fn process_details(system: &System, id: &ProcessIdentifier) -> Vec<String> {
    let profile = system
        .core_broker()
        .lock_mut()
        .and_then(|mut b| b.worker_mut().any_process_from_identifier(id, true))
        .and_then(|p| p.lock()?.profile());
    details_or_error(profile.map(|p| composition_lines(&p)))
}

/// Container の CompositeContainer のスロット。profile が取れなければ、その理由を 1 行で返す
pub(crate) fn container_details(system: &System, id: &ContainerIdentifier) -> Vec<String> {
    let profile = system
        .core_broker()
        .lock_mut()
        .and_then(|mut b| b.worker_mut().container_from_identifier(id))
        .and_then(|c| c.lock()?.profile());
    details_or_error(profile.map(|p| slots_lines(&p)))
}

fn details_or_error(lines: JuizResult<Vec<String>>) -> Vec<String> {
    lines.unwrap_or_else(|e| vec![format!("(profile is not available: {e})")])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composition() -> Composition {
        serde_json::from_value(jvalue!({
            "containers": { "camera": "video_capture", "fs": "filesystem" },
            "body": {
                "process": "filesystem_imwrite",
                "container": "fs",
                "args": {
                    "img": {
                        "process": "cv_cvt_color",
                        "args": {
                            "img": { "process": "video_capture_readandget", "container": "camera", "args": {} },
                            "code": { "const": "BGR2GRAY" }
                        }
                    },
                    "filename": { "expose": "path" }
                }
            }
        }))
        .unwrap()
    }

    #[test]
    fn format_composition_draws_tree() {
        assert_eq!(
            format_composition(&composition()),
            vec![
                "filesystem_imwrite [fs: filesystem]",
                "├─ filename = $path",
                "└─ img: cv_cvt_color",
                "   ├─ code = \"BGR2GRAY\"",
                "   └─ img: video_capture_readandget [camera: video_capture]",
            ]
        );
    }

    #[test]
    fn format_composition_keeps_vertical_line_for_inner_nodes() {
        let c: Composition = serde_json::from_value(jvalue!({
            "body": {
                "process": "add",
                "args": {
                    "a": { "process": "inc", "args": { "x": { "process": "zero" } } },
                    "b": { "expose": { "name": "b", "default": 1 } }
                }
            }
        }))
        .unwrap();
        assert_eq!(
            format_composition(&c),
            vec!["add", "├─ a: inc", "│  └─ x: zero", "└─ b = $b"]
        );
    }

    #[test]
    fn lines_from_profile() {
        let mut profile = ProcessProfile::new("p0", "inc_twice");
        assert!(composition_lines(&profile).is_empty());
        profile.composition = Some(composition());
        assert_eq!(composition_lines(&profile).len(), 5);

        let mut profile = ContainerProfile::new("composite_container", "cc0");
        assert!(slots_lines(&profile).is_empty());
        profile.slots = Some(BTreeMap::from([(
            "fs".to_owned(),
            "core://core/container/fs0::filesystem".to_owned(),
        )]));
        assert_eq!(
            slots_lines(&profile),
            vec!["fs -> core://core/container/fs0::filesystem"]
        );
    }
}
