use std::path::Path;

use juiz_core::log;
use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;

use clap::Subcommand;

use crate::composition::composition_lines;
use crate::namespace::{format_list, ListEntry};
use crate::process::composite_details;
use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ContProcSubCommands {
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
        #[arg(help = "ID of process")]
        identifier: String,
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

pub(crate) fn on_container_process(
    manifest: Value,
    working_dir: &Path,
    subcommand: ContProcSubCommands,
    args: Args,
) -> JuizResult<()> {
    let server = args.server.clone();
    let _recursive = args.recursive;

    match subcommand {
        ContProcSubCommands::List {
            filepath,
            namespace,
            long,
        } => {
            log::trace!("container-process list command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            let recursive = args.recursive;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| {
                    on_container_process_list(system, recursive, namespace, long)
                })
        }
        ContProcSubCommands::Info { identifier } => System::new(manifest)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once(|system| on_container_process_info(system, identifier)),
        ContProcSubCommands::Call {
            identifier,
            argument,
            fileout,
            print,
        } => System::new(manifest)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once(|system| {
                on_container_process_call(system, identifier, argument, fileout, print)
            }),
    }
}

fn on_container_process_list(
    system: &mut System,
    recursive: bool,
    namespace: Option<String>,
    long: bool,
) -> JuizResult<()> {
    log::trace!("on_container_process_list() called");
    let ids = system.container_process_list(recursive)?;
    let entries = ids.iter().map(ListEntry::from).collect();
    let details = |e: &ListEntry| composite_details(system, e);
    println!(
        "{}",
        format_list(entries, namespace.as_deref(), long, &details)?
    );
    Ok(())
}

fn on_container_process_info(system: &mut System, id: String) -> JuizResult<()> {
    //println!("processes:");
    let pid = id.try_into()?;
    let p = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .any_process_from_identifier(&pid, true);
    match p {
        Ok(ps) => {
            let profile = ps.lock()?.profile()?;
            println!("{profile}");
            // Composite の中身 (issue #170)
            for line in composition_lines(&profile) {
                println!("  {line}");
            }
        }
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}
fn on_container_process_call(
    system: &mut System,
    id: String,
    arg: String,
    _fileout: Option<String>,
    print: bool,
) -> JuizResult<()> {
    //println!("processes:");
    let p = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .any_process_from_identifier(&id.try_into()?, true);
    match p {
        Ok(ps) => {
            let argv = load_str(arg.as_str())?;
            // println!("Value is {argv:?}");
            let value = ps.lock()?.call(argv.try_into()?)?;
            if print {
                println!("{value}");
            }
        }
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}
