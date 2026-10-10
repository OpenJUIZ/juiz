
use std::path::Path;

use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;
use juiz_core::log;


use clap::Subcommand;

use crate::Args;


#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ContProcSubCommands {
    /// get logs
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(short = 'f', default_value = "./juiz.conf", help = "Input system definition file path")]
        filepath: String,
    },

    /// get logs
    #[clap(arg_required_else_help = false)]
    Info {
        #[arg(help = "ID of process")]
        identifier: String
    },


    /// get logs
    #[clap(arg_required_else_help = false)]
    Call {
        #[arg(help = "ID of process")]
        identifier: String,


        #[arg(help = "Argument")]
        argument: String,

        #[arg(short = 'o', help = "Output Filename")]
        fileout: Option<String>,

        #[arg(short = 'p', long, help = "Print Output")]
        print: bool,
    },
}

pub(crate) fn on_container_process(manifest: Value, working_dir: &Path, subcommand: ContProcSubCommands, args: Args) -> JuizResult<()> {
    let server = args.server.clone();
    let _recursive = args.recursive;
    
    match subcommand {
        ContProcSubCommands::List { filepath} => {
            log::trace!("container-process list command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            let recursive = args.recursive;
            System::new(manifest2)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once( |system| { on_container_process_list(system, Some(server), recursive) }) 
        },
        ContProcSubCommands::Info { identifier } => {
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                on_container_process_info(system, identifier)
            }) 
        },
        ContProcSubCommands::Call { identifier, argument, fileout, print } => {
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                on_container_process_call(system, identifier, argument, fileout, print)
            }) 
        },
    }
}

fn on_container_process_list(system: &mut System, _server: Option<String>, recursive: bool) -> JuizResult<()> {
    log::trace!("on_container_process_list() called");
    let proc_manifests = system.container_process_list(recursive)?;
    //println!("proc_manifests: {proc_manifests:?}");
    let mut ids: Vec<String> = Vec::new();
    for v in proc_manifests.iter() {
        //ids.push(obj_get_str(v, "identifier")?.to_owned());
        ids.push(v.to_string());
    }
    //println!("process list");
    println!("{ids:?}");
    Ok(())
}

fn on_container_process_info(system: &mut System, id: String) -> JuizResult<()> {
    //println!("processes:");
    let pid = id.try_into()?;
    let p = system.core_broker().lock_mut()?.worker_mut().any_process_from_identifier(&pid, true);
    match p {
        Ok(ps) => println!("{:}", ps.lock()?.profile()?),
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}
fn on_container_process_call(system: &mut System, id: String, arg: String, _fileout: Option<String>, print: bool) -> JuizResult<()> {
    //println!("processes:");
    let p = system.core_broker().lock_mut()?.worker_mut().any_process_from_identifier(&id.try_into()?, true);
    match p {
        Ok(ps) => {
            let argv = load_str(arg.as_str())?;
            // println!("Value is {argv:?}");
            let value = ps.lock()?.call(argv.try_into()?)?;
            if print {
                println!("{value}");
            }
        },
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}