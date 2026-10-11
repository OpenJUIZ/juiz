use juiz_core::log;
use juiz_core::prelude::juiz_lock;
use juiz_core::utils::yaml_conf_load;
use std::path::Path;

use clap::Subcommand;
use juiz_core::prelude::*;

use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum EcSubCommands {
    /// get logs
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(
            short = 'f',
            default_value = "./juiz.conf",
            help = "Input system definition file path"
        )]
        filepath: String,
    },

    #[clap(arg_required_else_help = false)]
    Start {
        #[arg(help = "ID of Execution context")]
        identifier: String,

        #[arg(
            short = 'f',
            default_value = "./juiz.conf",
            help = "Input system definition file path"
        )]
        filepath: String,
    },
}

pub(crate) fn on_execution_context(
    manifest: Value,
    working_dir: &Path,
    subcommand: EcSubCommands,
    args: Args,
) -> JuizResult<()> {
    match on_ec_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}"),
    };
    Ok(())
}

pub(crate) fn on_ec_inner(
    _manifest: Value,
    working_dir: &Path,
    subcommand: EcSubCommands,
    args: Args,
) -> JuizResult<()> {
    match subcommand {
        EcSubCommands::List { filepath } => {
            log::trace!("ec list command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            let recursive = args.recursive;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_ec_list(system, Some(server), recursive))
        }

        EcSubCommands::Start {
            identifier,
            filepath,
        } => {
            log::trace!("ec start command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_ec_start(system, Some(server), identifier))
        }
    }
}

fn on_ec_list(system: &mut System, _server: Option<String>, recursive: bool) -> JuizResult<()> {
    log::info!("on_ec_list() called");
    let ec_manifests: Vec<Value> = system.ec_list(recursive)?;
    let mut ids: Vec<String> = Vec::new();
    for v in ec_manifests.iter() {
        ids.push(v.as_str().unwrap().to_owned());
    }
    println!("{ids:?}");
    Ok(())
}

fn on_ec_start(system: &mut System, _server: Option<String>, id: String) -> JuizResult<()> {
    //println!("processes:");
    let e = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .ec_from_id(&id);
    match e {
        Ok(ec) => {
            juiz_lock(&ec)?.start()?;
        }
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}
