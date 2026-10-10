
use std::path::Path;


use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;
use juiz_core::log;


use clap::Subcommand;

use crate::Args;


#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ContSubCommands {
    /// get logs
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(short = 'f', default_value = "./juiz.conf", help = "Input system definition file path")]
        filepath: String,
    },

    /// get logs
    #[clap(arg_required_else_help = false)]
    Info {
        #[arg(help = "ID of container")]
        identifier: String
    },
}

pub(crate) fn on_container(manifest: Value, working_dir: &Path, subcommand: ContSubCommands, args: Args) -> JuizResult<()> {
    match on_container_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}")
    };
    Ok(())
}

pub(crate) fn on_container_inner(manifest: Value, working_dir: &Path, subcommand: ContSubCommands, args: Args ) -> JuizResult<()> {
    let recursive = args.recursive;
    match subcommand {
        ContSubCommands::List { filepath } => {
            log::trace!("container list command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            System::new(manifest2)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once( |system| { on_container_list(system, Some(server), recursive) }) 
        },
        ContSubCommands::Info { identifier } => {
            let server = args.server;
            System::new(manifest)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once( |system| { 
                on_container_info(system, identifier)
            }) 
        } 
    }
}

fn on_container_list(system: &mut System, _server: Option<String>, recursive: bool) -> JuizResult<()> {
    log::trace!("on_container_list() called");
    let proc_manifests = system.container_list(recursive)?;
    log::debug!("system.container_list() returns '{proc_manifests:?}'");
    let mut ids: Vec<String> = Vec::new();

    for v in proc_manifests.iter() {
        ids.push(v.to_string());
    }
    println!("{ids:?}");
    Ok(())
}

fn on_container_info(system: &mut System, id: String) -> JuizResult<()> {
    let p = system.core_broker().lock_mut()?.worker_mut().container_from_identifier(&id.try_into()?);
    match p {
        Ok(ps) => println!("{:}",  ps.lock()?.profile()?),
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}