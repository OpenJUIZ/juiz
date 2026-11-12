use juiz_core::log;
use juiz_core::utils::yaml_conf_load;
use std::path::Path;

use juiz_core::prelude::*;

// #[cfg(feature="opencv4")]
// use juiz_core::opencv::{imgcodecs::imwrite, core::{Mat, Vector}};
use clap::Subcommand;

use crate::composition::{process_details, process_lines};
use crate::namespace::{format_list, ListEntry};
use crate::Args;
use juiz_sdk::process_identifier::ProcessIdentifier;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ProcSubCommands {
    /// List processes
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(short = 'a', help = "Any process includes")]
        any_process: bool,

        #[arg(
            long = "namespace",
            help = "List only objects in this namespace and below (ex., robot1)"
        )]
        namespace: Option<String>,

        #[arg(long = "long", help = "Show namespace, name and type columns")]
        long: bool,

        #[arg(
            short = 'f',
            default_value = "./juiz.conf",
            help = "Input system definition file path"
        )]
        filepath: String,
    },

    /// Show the profile of a process
    #[clap(arg_required_else_help = false)]
    Info {
        #[arg(help = "ID of process")]
        identifier: String,
    },

    /// Call a process with arguments
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

    /// Call a process with some of the arguments. The others are taken from p_apply / push (or the defaults)
    #[clap(arg_required_else_help = false)]
    CallPartial {
        #[arg(help = "ID of process")]
        identifier: String,

        #[arg(
            default_value = "{}",
            help = "Arguments to pass (the others are not changed)"
        )]
        argument: String,

        #[arg(short = 'p', long, help = "Print Output")]
        print: bool,
    },
}

pub(crate) fn on_process(
    manifest: Value,
    working_dir: &Path,
    subcommand: ProcSubCommands,
    args: Args,
) -> JuizResult<()> {
    match on_process_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}"),
    };
    Ok(())
}
pub(crate) fn on_process_inner(
    manifest: Value,
    working_dir: &Path,
    subcommand: ProcSubCommands,
    args: Args,
) -> JuizResult<()> {
    let server = args.server.clone();
    let recursive = args.recursive;
    match subcommand {
        ProcSubCommands::List {
            any_process,
            filepath,
            namespace,
            long,
        } => {
            log::trace!("process list command is selected. args={args:?}");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| {
                    let ids = if any_process {
                        system.any_process_list(recursive)?
                    } else {
                        system.process_list(recursive)?
                    };
                    let entries = ids.iter().map(ListEntry::from).collect();
                    let details = |e: &ListEntry| composite_details(system, e);
                    println!(
                        "{}",
                        format_list(entries, namespace.as_deref(), long, &details)?
                    );
                    Ok(())
                })
        }
        ProcSubCommands::Info { identifier } => System::new(manifest)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once(|system| on_process_info(system, identifier)),
        ProcSubCommands::Call {
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
                on_process_call(system, identifier, argument, fileout, print)
            }),
        ProcSubCommands::CallPartial {
            identifier,
            argument,
            print,
        } => System::new(manifest)?
            .set_working_dir(working_dir)
            .start_http_broker(args.start_http_broker)
            .setup()?
            .add_systemproxy_by_id(Some(server.clone()))?
            .run_and_do_once(|system| {
                crate::process::on_any_process_call_partial(system, identifier, argument, print)
            }),
    }
}

/// 一覧の行の下に出す Composite の式の木 (issue #170)
pub(crate) fn composite_details(system: &System, e: &ListEntry) -> Vec<String> {
    match ProcessIdentifier::try_from(e.identifier.clone()) {
        Ok(id) => process_details(system, &id),
        Err(_) => Vec::new(),
    }
}

fn on_process_info(system: &mut System, id: String) -> JuizResult<()> {
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
            // readonly の印 (issue #377) と Composite の中身 (issue #170)
            for line in process_lines(&profile) {
                println!("  {line}");
            }
        }
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}

// #[cfg(feature="opencv4")]
// fn do_with_capsule_ptr(value: CapsulePtr) -> JuizResult<()> {
//     if value.is_value()? {
//         println!("{:?}", value);
//     } else if value.is_mat()? {
//         let _ = value.lock_as_mat(|mat: &Mat| {

//             let params: Vector<i32> = Vector::new();
//             match fileout {
//                 Some(filepath) => {
//                     match imwrite(filepath.as_str(), mat, &params) {
//                         Ok(_) => {
//                             //println!("ok");
//                         },
//                         Err(e) => {
//                             println!("error: {e:?}");
//                         }
//                     }
//                 },
//                 None => {
//                     println!("{:?}", mat);
//                 }
//             }
//         } );
//     }
//     Ok(())
// }

// #[cfg(not(feature="opencv4"))]
#[allow(unused)]
fn do_with_capsule_ptr(value: CapsulePtr) -> JuizResult<()> {
    if value.is_value()? {
        println!("{:?}", value);
    } else if value.is_image()? {
        let _ = value.lock_as_image(|image| {
            todo!()

            // let params: Vector<i32> = Vector::new();
            // match fileout {
            //     Some(filepath) => {
            //         match imwrite(filepath.as_str(), mat, &params) {
            //             Ok(_) => {
            //                 //println!("ok");
            //             },
            //             Err(e) => {
            //                 println!("error: {e:?}");
            //             }
            //         }
            //     },
            //     None => {
            //         println!("{:?}", mat);
            //     }
            // }
        });
    }
    Ok(())
}

// #[cfg(feature = "opencv4")]
// fn do_with_capsule_ptr(value: CapsulePtr) -> JuizResult<()> {
//     if value.is_value()? {
//         println!("{:?}", value);
//     } else if value.is_mat()? {
//         let _ = value.lock_as_mat(|mat| {

//             let params: Vector<i32> = Vector::new();
//             match fileout {
//                 Some(filepath) => {
//                     match imwrite(filepath.as_str(), mat, &params) {
//                         Ok(_) => {
//                             //println!("ok");
//                         },
//                         Err(e) => {
//                             println!("error: {e:?}");
//                         }
//                     }
//                 },
//                 None => {
//                     println!("{:?}", mat);
//                 }
//             }
//         } );
//     }
//     Ok(())
// }

fn on_process_call(
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

/// 一部の引数だけで呼び出す (issue #83)。省いた引数は p_apply・push 済みの値か既定値で補われる
pub(crate) fn on_any_process_call_partial(
    system: &mut System,
    id: String,
    arg: String,
    print: bool,
) -> JuizResult<()> {
    let p = system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .any_process_from_identifier(&id.try_into()?, true);
    match p {
        Ok(ps) => {
            let argv = load_str(arg.as_str())?;
            let value = ps.lock()?.call_partial(argv.try_into()?)?;
            if print {
                println!("{value}");
            }
        }
        Err(e) => println!("Error: {e:?}"),
    }
    Ok(())
}
