//! `juiz composite` サブコマンド: 実行中の juiz に Composite を定義する (issue #171)
//!
//! ```text
//! juiz composite create inc_twice.yaml      # Composite の Factory / CompositeContainer を作る
//! juiz composite list [--long]              # 登録済みの Composite の型名 (--long は式の木も)
//! juiz composite info inc_twice             # Factory の manifest と式の木
//! juiz composite destroy inc_twice          # Factory の登録を外す
//! ```
//!
//! 定義のファイルは conf と同じ形の YAML / JSON。Composite 1 件 (`type_name` と `body`) か、
//! `composites:` と `composite_containers:` を持つもの (conf の一部をそのまま渡せる)。
//! CompositeContainer の削除は `juiz container destroy <ID>`。

use std::path::Path;

use clap::Subcommand;
use juiz_core::log;
use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;
use juiz_sdk::{anyhow::anyhow, manifests::CompositeManifest};

use crate::composition::format_composition;
use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum CompositeSubCommands {
    /// Define Composites (and CompositeContainers) from a YAML / JSON file
    #[clap(arg_required_else_help = true)]
    Create {
        #[arg(help = "Definition file (a Composite, or 'composites:' / 'composite_containers:')")]
        file: String,
    },

    /// List the type names of Composites
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(long = "long", help = "Show the kind and the expression tree")]
        long: bool,
    },

    /// Show the manifest of a Composite
    #[clap(arg_required_else_help = true)]
    Info {
        #[arg(help = "Type name of the Composite")]
        type_name: String,
    },

    /// Destroy (unregister) a Composite
    #[clap(arg_required_else_help = true)]
    Destroy {
        #[arg(help = "Type name of the Composite")]
        type_name: String,
    },
}

pub(crate) fn on_composite(
    manifest: Value,
    working_dir: &Path,
    subcommand: CompositeSubCommands,
    args: Args,
) -> JuizResult<()> {
    let server = args.server.clone();
    System::new(manifest)?
        .set_working_dir(working_dir)
        .start_http_broker(args.start_http_broker)
        .setup()?
        .run_and_do_once(|system| {
            let proxy = server_proxy(system, &server)?;
            let mut proxy = juiz_lock(&proxy)?;
            match subcommand {
                CompositeSubCommands::Create { file } => on_composite_create(&mut *proxy, &file),
                CompositeSubCommands::List { long } => on_composite_list(&*proxy, long),
                CompositeSubCommands::Info { type_name } => {
                    let m = proxy.composite_profile_full(&type_name)?;
                    println!("{}", serde_json::to_string_pretty(&m)?);
                    print_composite(&m);
                    Ok(())
                }
                CompositeSubCommands::Destroy { type_name } => {
                    let m = proxy.composite_destroy(&type_name)?;
                    println!("destroyed: {}", m.type_name);
                    Ok(())
                }
            }
        })
}

/// `-s` で指定したサーバーへの BrokerProxy
pub(crate) fn server_proxy(
    system: &mut System,
    server: &str,
) -> JuizResult<std::sync::Arc<std::sync::Mutex<dyn BrokerProxy>>> {
    let id_struct = IdentifierStruct::new_broker_id(server.to_owned())?;
    system.create_broker_proxy(&jvalue!({
        "identifier": server,
        "type_name": id_struct.broker_type_name,
        "name": id_struct.broker_name,
    }))
}

/// 定義のファイルを読み、Composite の定義と CompositeContainer の定義に分ける
pub(crate) fn load_definitions(file: &str) -> JuizResult<(Vec<CompositeManifest>, Vec<Value>)> {
    if !Path::new(file).is_file() {
        return Err(anyhow!(JuizError::InvalidArgumentError {
            message: format!("definition file '{file}' is not found"),
        }));
    }
    let v = yaml_conf_load(file.to_owned())?;
    parse_definitions(&v).map_err(|e| e.context(format!("reading '{file}'")))
}

fn parse_definitions(v: &Value) -> JuizResult<(Vec<CompositeManifest>, Vec<Value>)> {
    let invalid = |message: String| anyhow!(JuizError::InvalidArgumentError { message });
    let parse = |i: Option<usize>, c: &Value| {
        serde_json::from_value::<CompositeManifest>(c.clone()).map_err(|e| match i {
            Some(i) => invalid(format!("composites[{i}] is invalid: {e}")),
            None => invalid(format!("Composite definition is invalid: {e}")),
        })
    };
    if v.get("type_name").is_some() {
        return Ok((vec![parse(None, v)?], Vec::new()));
    }
    let array = |key: &str| -> JuizResult<Vec<Value>> {
        match v.get(key) {
            None => Ok(Vec::new()),
            Some(a) => a
                .as_array()
                .cloned()
                .ok_or_else(|| invalid(format!("'{key}' must be an array"))),
        }
    };
    let composites = array("composites")?
        .iter()
        .enumerate()
        .map(|(i, c)| parse(Some(i), c))
        .collect::<JuizResult<Vec<_>>>()?;
    let containers = array("composite_containers")?;
    if composites.is_empty() && containers.is_empty() {
        return Err(invalid(
            "no definition found. Write a Composite ('type_name' and 'body'), or 'composites:' / 'composite_containers:'".to_owned(),
        ));
    }
    Ok((composites, containers))
}

fn on_composite_create(proxy: &mut dyn BrokerProxy, file: &str) -> JuizResult<()> {
    log::trace!("on_composite_create({file}) called");
    let (composites, containers) = load_definitions(file)?;
    // conf と同じく、Composite を配列の順に登録してから CompositeContainer を作る
    for c in composites.iter() {
        let m = proxy
            .composite_create(c)
            .map_err(|e| e.context(format!("creating Composite '{}'", c.type_name)))?;
        println!("created: {} ({})", m.type_name, kind(&m));
        print_composite(&m);
    }
    for (i, c) in containers.iter().enumerate() {
        let p = proxy
            .composite_container_create(c)
            .map_err(|e| e.context(format!("creating composite_containers[{i}]")))?;
        let id: String = p.identifier().into();
        println!("created: {id}");
        for line in crate::composition::slots_lines(&p) {
            println!("  {line}");
        }
    }
    Ok(())
}

fn on_composite_list(proxy: &dyn BrokerProxy, long: bool) -> JuizResult<()> {
    let names = proxy.composite_list()?;
    if !long {
        println!("{}", serde_json::to_string(&names)?);
        return Ok(());
    }
    for name in names.iter() {
        match proxy.composite_profile_full(name) {
            Ok(m) => {
                println!("{name}  {}", kind(&m));
                print_composite(&m);
            }
            Err(e) => println!("{name}  (profile is not available: {e})"),
        }
    }
    Ok(())
}

/// インスタンスを作るときの種類。スロットを持つ Composite は ContainerProcess になる
fn kind(m: &ProcessManifest) -> &'static str {
    if m.container_type.is_some() {
        "container_process"
    } else {
        "process"
    }
}

fn print_composite(m: &ProcessManifest) {
    if let Some(c) = m.composition.as_ref() {
        for line in format_composition(c) {
            println!("  {line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composite(type_name: &str) -> Value {
        jvalue!({ "type_name": type_name, "body": { "process": "increment_process" } })
    }

    #[test]
    fn parse_single_composite() {
        let (c, cc) = parse_definitions(&composite("inc1")).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].type_name, "inc1");
        assert!(cc.is_empty());
    }

    #[test]
    fn parse_conf_form() {
        let v = jvalue!({
            "composites": [composite("a"), composite("b")],
            "composite_containers": [{ "name": "cc0", "slots": { "s": "c0" } }],
        });
        let (c, cc) = parse_definitions(&v).unwrap();
        assert_eq!(
            c.iter().map(|c| c.type_name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(cc.len(), 1);
    }

    #[test]
    fn parse_errors() {
        assert!(parse_definitions(&jvalue!({})).is_err());
        assert!(parse_definitions(&jvalue!({ "composites": {} })).is_err());
        let e = parse_definitions(&jvalue!({ "composites": [{ "type_name": "x" }] }))
            .unwrap_err()
            .to_string();
        assert!(e.contains("composites[0]"), "{e}");
    }

    #[test]
    fn load_missing_file() {
        let e = load_definitions("no_such_file.yaml")
            .unwrap_err()
            .to_string();
        assert!(e.contains("no_such_file.yaml"), "{e}");
    }
}
