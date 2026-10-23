use std::path::Path;

use juiz_core::log;
use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;

use clap::Subcommand;

use crate::namespace::{format_list, ListEntry};
use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ContSubCommands {
    /// get logs
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(
            short = 'f',
            default_value = "./juiz.conf",
            help = "Input system definition file path"
        )]
        filepath: String,

        #[arg(
            long = "namespace",
            help = "List only objects in this namespace and below (ex., robot1)"
        )]
        namespace: Option<String>,

        #[arg(long = "long", help = "Show namespace, name and type columns")]
        long: bool,
    },

    /// get logs
    #[clap(arg_required_else_help = false)]
    Info {
        #[arg(help = "ID of container")]
        identifier: String,
    },
}

pub(crate) fn on_container(
    manifest: Value,
    working_dir: &Path,
    subcommand: ContSubCommands,
    args: Args,
) -> JuizResult<()> {
    match on_container_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}"),
    };
    Ok(())
}

pub(crate) fn on_container_inner(
    manifest: Value,
    working_dir: &Path,
    subcommand: ContSubCommands,
    args: Args,
) -> JuizResult<()> {
    let recursive = args.recursive;
    match subcommand {
        ContSubCommands::List {
            filepath,
            namespace,
            long,
        } => {
            log::trace!("container list command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_container_list(system, recursive, namespace, long))
        }
        ContSubCommands::Info { identifier } => {
            let server = args.server;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_container_info(system, identifier))
        }
    }
}

fn on_container_list(
    system: &mut System,
    recursive: bool,
    namespace: Option<String>,
    long: bool,
) -> JuizResult<()> {
    log::trace!("on_container_list() called");
    let ids = system.container_list(recursive)?;
    log::debug!("system.container_list() returns '{ids:?}'");
    let entries = ids.iter().map(ListEntry::from).collect();
    println!("{}", format_list(entries, namespace.as_deref(), long)?);
    Ok(())
}

fn on_container_info(system: &mut System, id: String) -> JuizResult<()> {
    let p = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .container_from_identifier(&id.try_into()?);
    match p {
        Ok(ps) => println!("{:}", ps.lock()?.profile()?),
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}
