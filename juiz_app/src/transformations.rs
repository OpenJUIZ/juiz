//! Transformations の CLI (issue #370、仕様 docs/spec/transformations.md §13.2)
//!
//! ```text
//! juiz container pose <id> --reference map                       # 読む
//! juiz container pose <id> --reference map --set '<Pose の JSON>' # 書く
//! juiz transform list [--long]
//! juiz transform info <id | source destination>
//! juiz transform create <source> <destination> [--translation x,y,z] [--rotation x,y,z,w]
//! juiz transform update <id | source destination> [--translation ...] [--rotation ...]
//! juiz transform delete <id | source destination>
//! juiz anchor list
//! juiz anchor update <name>
//! juiz anchor tree <name>
//! ```
//!
//! Container・Anchor は ID か名前 (根のネームスペースから解決する、`robot1/base0` など)。
//! Transform は ID (`transform://…`) か、始点と終点の 2 つの名前で指定する。
//! `juiz container list --long` は Anchor の種類と rate を行の下に出す。

use std::path::Path;

use clap::Subcommand;
use juiz_core::prelude::*;
use juiz_sdk::anyhow::anyhow;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::manifests::TransformManifest;

use crate::composite::server_proxy;
use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum TransformSubCommands {
    /// List the identifiers of Transforms
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(long = "long", help = "Show source, destination and the value")]
        long: bool,
    },

    /// Show a Transform
    #[clap(arg_required_else_help = true)]
    Info {
        #[arg(help = "ID of the Transform (transform://...), or the source and the destination", num_args = 1..=2)]
        target: Vec<String>,
    },

    /// Create a Transform from <source> (parent) to <destination> (child)
    #[clap(arg_required_else_help = true)]
    Create {
        #[arg(help = "Source (parent) Container: ID or name")]
        source: String,
        #[arg(help = "Destination (child) Container: ID or name")]
        destination: String,
        #[arg(
            long = "translation",
            help = "x,y,z [m] (default: 0,0,0)",
            allow_hyphen_values = true
        )]
        translation: Option<String>,
        #[arg(
            long = "rotation",
            help = "Quaternion x,y,z,w (default: 0,0,0,1)",
            allow_hyphen_values = true
        )]
        rotation: Option<String>,
    },

    /// Change the value of a Transform. Omitted values are kept
    #[clap(arg_required_else_help = true)]
    Update {
        #[arg(help = "ID of the Transform (transform://...), or the source and the destination", num_args = 1..=2)]
        target: Vec<String>,
        #[arg(long = "translation", help = "x,y,z [m]", allow_hyphen_values = true)]
        translation: Option<String>,
        #[arg(
            long = "rotation",
            help = "Quaternion x,y,z,w",
            allow_hyphen_values = true
        )]
        rotation: Option<String>,
    },

    /// Delete a Transform
    #[clap(arg_required_else_help = true, alias = "destroy")]
    Delete {
        #[arg(help = "ID of the Transform (transform://...), or the source and the destination", num_args = 1..=2)]
        target: Vec<String>,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum AnchorSubCommands {
    /// List the identifiers of Anchors
    #[clap(arg_required_else_help = false)]
    List {},

    /// Update the ContainerPoses in the tree of an Anchor now
    #[clap(arg_required_else_help = true)]
    Update {
        #[arg(help = "Anchor: ID or name")]
        anchor: String,
    },

    /// Show the tree of an Anchor with the ContainerPoses
    #[clap(arg_required_else_help = true)]
    Tree {
        #[arg(help = "Anchor: ID or name")]
        anchor: String,
        #[arg(long = "json", help = "Print the tree as JSON")]
        json: bool,
    },
}

fn run_with_proxy(
    manifest: Value,
    working_dir: &Path,
    args: &Args,
    f: impl FnOnce(&mut dyn BrokerProxy) -> JuizResult<()>,
) -> JuizResult<()> {
    let server = args.server.clone();
    let mut f = Some(f);
    System::new(manifest)?
        .set_working_dir(working_dir)
        .start_http_broker(args.start_http_broker)
        .setup()?
        .run_and_do_once(|system| {
            let proxy = server_proxy(system, &server)?;
            let mut proxy = juiz_write(&proxy)?;
            (f.take().unwrap())(&mut *proxy)
        })
}

fn print_error(r: JuizResult<()>) -> JuizResult<()> {
    if let Err(e) = r {
        println!("Error: {e:#}");
    }
    Ok(())
}

/// `juiz container pose`
pub(crate) fn on_container_pose(
    manifest: Value,
    working_dir: &Path,
    args: Args,
    identifier: String,
    reference: String,
    set: Option<String>,
) -> JuizResult<()> {
    print_error(run_with_proxy(manifest, working_dir, &args, |proxy| {
        let p = match set {
            None => proxy.container_pose(&identifier, &reference)?,
            Some(json) => {
                let pose = parse_pose(&json)?;
                proxy.container_set_pose(&identifier, &reference, &pose)?
            }
        };
        print_pose_stamped(&p);
        Ok(())
    }))
}

pub(crate) fn on_transform(
    manifest: Value,
    working_dir: &Path,
    subcommand: TransformSubCommands,
    args: Args,
) -> JuizResult<()> {
    print_error(run_with_proxy(
        manifest,
        working_dir,
        &args,
        |proxy| match subcommand {
            TransformSubCommands::List { long } => {
                let ids = proxy.transform_list()?;
                if !long {
                    println!("{}", serde_json::to_string(&ids)?);
                    return Ok(());
                }
                for id in ids.iter() {
                    let p = proxy.transform_profile_full(id)?;
                    println!("{}", transform_line(&p));
                }
                Ok(())
            }
            TransformSubCommands::Info { target } => {
                let id = find_transform(proxy, &target)?;
                let p = proxy.transform_profile_full(&id)?;
                println!("{}", serde_json::to_string_pretty(&p)?);
                Ok(())
            }
            TransformSubCommands::Create {
                source,
                destination,
                translation,
                rotation,
            } => {
                let mut m = TransformManifest::new(&source, &destination);
                if let Some(t) = translation {
                    m = m.translation(parse_vector3(&t)?);
                }
                if let Some(r) = rotation {
                    m = m.rotation(parse_quaternion(&r)?);
                }
                let p = proxy.transform_create(&m)?;
                println!("created: {}", transform_line(&p));
                Ok(())
            }
            TransformSubCommands::Update {
                target,
                translation,
                rotation,
            } => {
                let id = find_transform(proxy, &target)?;
                let translation = translation.map(|t| parse_vector3(&t)).transpose()?;
                let rotation = rotation.map(|r| parse_quaternion(&r)).transpose()?;
                let p = proxy.transform_update(&id, translation, rotation)?;
                println!("updated: {}", transform_line(&p));
                Ok(())
            }
            TransformSubCommands::Delete { target } => {
                let id = find_transform(proxy, &target)?;
                let p = proxy.transform_destroy(&id)?;
                println!("deleted: {}", p["identifier"].as_str().unwrap_or_default());
                Ok(())
            }
        },
    ))
}

pub(crate) fn on_anchor(
    manifest: Value,
    working_dir: &Path,
    subcommand: AnchorSubCommands,
    args: Args,
) -> JuizResult<()> {
    print_error(run_with_proxy(
        manifest,
        working_dir,
        &args,
        |proxy| match subcommand {
            AnchorSubCommands::List {} => {
                println!("{}", serde_json::to_string(&proxy.anchor_list()?)?);
                Ok(())
            }
            AnchorSubCommands::Update { anchor } => {
                let v = proxy.anchor_update(&anchor)?;
                println!("updated: {}", v["identifier"].as_str().unwrap_or_default());
                Ok(())
            }
            AnchorSubCommands::Tree { anchor, json } => {
                let tree = proxy.transformations_tree(&anchor)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&tree)?);
                } else {
                    for line in tree_lines(&tree) {
                        println!("{line}");
                    }
                }
                Ok(())
            }
        },
    ))
}

fn invalid(message: String) -> juiz_sdk::anyhow::Error {
    anyhow!(JuizError::InvalidArgumentError { message })
}

fn parse_numbers(s: &str, n: usize, what: &str) -> JuizResult<Vec<f64>> {
    let values = s
        .split(',')
        .map(|t| t.trim().parse::<f64>())
        .collect::<Result<Vec<f64>, _>>()
        .map_err(|e| invalid(format!("{what} '{s}' is invalid: {e}")))?;
    if values.len() != n {
        return Err(invalid(format!(
            "{what} needs {n} numbers separated by ',', but '{s}'"
        )));
    }
    Ok(values)
}

/// `x,y,z`
pub(crate) fn parse_vector3(s: &str) -> JuizResult<Vector3> {
    let v = parse_numbers(s, 3, "translation")?;
    Ok(Vector3::new(v[0], v[1], v[2]))
}

/// `x,y,z,w`
pub(crate) fn parse_quaternion(s: &str) -> JuizResult<Quaternion> {
    let v = parse_numbers(s, 4, "rotation")?;
    Ok(Quaternion::new(v[0], v[1], v[2], v[3]))
}

/// `juiz_std.geometry.Pose` の JSON (`{"position": {...}, "orientation": {...}}`)
pub(crate) fn parse_pose(json: &str) -> JuizResult<Pose> {
    let v: Value = serde_json::from_str(json)
        .map_err(|e| invalid(format!("--set must be a JSON of Pose: {e} ({json})")))?;
    serde_json::from_value(v.clone()).map_err(|e| {
        invalid(format!(
            "--set must be a JSON of Pose ({{\"position\": {{\"x\", \"y\", \"z\"}}, \"orientation\": {{\"x\", \"y\", \"z\", \"w\"}}}}): {e} ({v})"
        ))
    })
}

fn fmt_vector3(v: &Value) -> String {
    let n = |k: &str| v[k].as_f64().unwrap_or(0.0);
    format!("({}, {}, {})", n("x"), n("y"), n("z"))
}

fn fmt_quaternion(v: &Value) -> String {
    let n = |k: &str| v[k].as_f64().unwrap_or(0.0);
    format!("({}, {}, {}, {})", n("x"), n("y"), n("z"), n("w"))
}

fn print_pose_stamped(p: &PoseStamped) {
    let v = serde_json::to_value(p).unwrap_or_default();
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

/// `<始点> -> <終点>  translation (x, y, z)  rotation (x, y, z, w)`
fn transform_line(p: &Value) -> String {
    format!(
        "{} -> {}  translation {}  rotation {}",
        p["source"].as_str().unwrap_or_default(),
        p["destination"].as_str().unwrap_or_default(),
        fmt_vector3(&p["transform"]["translation"]),
        fmt_quaternion(&p["transform"]["rotation"]),
    )
}

/// ID の名前の部分 (`<ns>/<name>`)。ID でなければそのまま
fn name_of(id: &str) -> String {
    match IdentifierStruct::try_from(id.to_owned()) {
        Ok(s) if s.namespace.is_empty() => s.object_name,
        Ok(s) => format!("{}/{}", s.namespace, s.object_name),
        Err(_) => id.to_owned(),
    }
}

/// Container の指定 (ID か名前) が `id` を指すか
fn matches_container(spec: &str, id: &str) -> bool {
    if spec.contains("://") {
        return spec == id;
    }
    let spec = spec.strip_prefix('/').unwrap_or(spec);
    let (path, type_name) = match spec.split_once("::") {
        Some((p, t)) => (p, Some(t)),
        None => (spec, None),
    };
    match IdentifierStruct::try_from(id.to_owned()) {
        Ok(s) => name_of(id) == path && type_name.map_or(true, |t| t == s.type_name),
        Err(_) => false,
    }
}

/// Transform の ID。`transform://…` ならそのまま、始点と終点の 2 つなら一覧から探す
fn find_transform(proxy: &dyn BrokerProxy, target: &[String]) -> JuizResult<String> {
    match target {
        [id] if id.starts_with("transform://") => Ok(id.clone()),
        [source, destination] => {
            let mut found = Vec::new();
            for id in proxy.transform_list()? {
                let p = proxy.transform_profile_full(&id)?;
                if matches_container(source, p["source"].as_str().unwrap_or_default())
                    && matches_container(destination, p["destination"].as_str().unwrap_or_default())
                {
                    found.push(id);
                }
            }
            match found.len() {
                1 => Ok(found.pop().unwrap()),
                0 => Err(anyhow!(JuizError::TransformNotFoundError {
                    identifier: format!("{source} -> {destination}")
                })),
                _ => Err(invalid(format!(
                    "Transform '{source} -> {destination}' is ambiguous ({found:?}). Use the identifier instead."
                ))),
            }
        }
        _ => Err(invalid(format!(
            "give the identifier of the Transform (transform://...) or the source and the destination, but {target:?}"
        ))),
    }
}

/// Anchor の木を字下げで描く。各行は名前と、Anchor から見た位置 (x, y, z) と向き (x, y, z, w)
pub(crate) fn tree_lines(tree: &Value) -> Vec<String> {
    fn walk(node: &Value, prefix: &str, last: bool, root: bool, out: &mut Vec<String>) {
        let id = node["identifier"].as_str().unwrap_or_default();
        let pose = &node["pose"];
        let label = format!(
            "{}  position {}  orientation {}",
            name_of(id),
            fmt_vector3(&pose["position"]),
            fmt_quaternion(&pose["orientation"]),
        );
        let (line, child_prefix) = if root {
            (label, String::new())
        } else {
            let branch = if last { "└─ " } else { "├─ " };
            let cont = if last { "   " } else { "│  " };
            (
                format!("{prefix}{branch}{label}"),
                format!("{prefix}{cont}"),
            )
        };
        out.push(line);
        let children = node["children"].as_array().cloned().unwrap_or_default();
        let n = children.len();
        for (i, c) in children.iter().enumerate() {
            walk(c, &child_prefix, i + 1 == n, false, out);
        }
    }
    let mut out = Vec::new();
    walk(tree, "", true, true, &mut out);
    out
}

/// `container list --long` で Anchor の行の下に出す `anchor: periodic (10 Hz)` / `anchor: static`
pub(crate) fn anchor_lines(profile: &ContainerProfile) -> Vec<String> {
    let Some(a) = profile.anchor.as_ref() else {
        return Vec::new();
    };
    let kind = a["kind"].as_str().unwrap_or_default();
    match a["rate"].as_f64() {
        Some(rate) => vec![format!("anchor: {kind} ({rate} Hz)")],
        None => vec![format!("anchor: {kind}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_values() {
        assert_eq!(
            parse_vector3("1, 0,-0.5").unwrap(),
            Vector3::new(1.0, 0.0, -0.5)
        );
        assert_eq!(
            parse_quaternion("0,0,0.7071,0.7071").unwrap(),
            Quaternion::new(0.0, 0.0, 0.7071, 0.7071)
        );
        assert!(parse_vector3("1,2").is_err());
        assert!(parse_quaternion("a,b,c,d").is_err());
        let p = parse_pose(r#"{"position": {"x": 1, "y": 2, "z": 3}, "orientation": {"x": 0, "y": 0, "z": 0, "w": 1}}"#).unwrap();
        assert_eq!(p.position, Vector3::new(1.0, 2.0, 3.0));
        assert!(parse_pose("{").is_err());
    }

    #[test]
    fn container_spec_matches_identifier() {
        let id = "core://core/container/robot1/base0::part";
        assert!(matches_container("robot1/base0", id));
        assert!(matches_container("/robot1/base0", id));
        assert!(matches_container("robot1/base0::part", id));
        assert!(!matches_container("robot1/base0::other", id));
        assert!(!matches_container("base0", id));
        assert!(matches_container(id, id));
    }

    #[test]
    fn draws_tree() {
        let tree = jvalue!({
            "identifier": "core://core/container/map::static_anchor",
            "pose": {"position": {"x": 0.0, "y": 0.0, "z": 0.0}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}},
            "children": [
                {"identifier": "core://core/container/base0::part",
                 "pose": {"position": {"x": 1.0, "y": 0.0, "z": 0.0}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}},
                 "children": [
                    {"identifier": "core://core/container/camera0::part",
                     "pose": {"position": {"x": 1.0, "y": 0.1, "z": 0.3}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}},
                     "children": []}
                 ]},
                {"identifier": "core://core/container/robot1/arm::part",
                 "pose": {"position": {"x": 0.0, "y": 1.0, "z": 0.0}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}},
                 "children": []}
            ]
        });
        assert_eq!(
            tree_lines(&tree),
            vec![
                "map  position (0, 0, 0)  orientation (0, 0, 0, 1)",
                "├─ base0  position (1, 0, 0)  orientation (0, 0, 0, 1)",
                "│  └─ camera0  position (1, 0.1, 0.3)  orientation (0, 0, 0, 1)",
                "└─ robot1/arm  position (0, 1, 0)  orientation (0, 0, 0, 1)",
            ]
        );
    }
}
