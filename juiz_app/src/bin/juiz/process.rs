
use std::path::Path;
use juiz_core::utils::yaml_conf_load;
use juiz_core::log;

use juiz_core::prelude::*;

// #[cfg(feature="opencv4")]
// use juiz_core::opencv::{imgcodecs::imwrite, core::{Mat, Vector}};
use clap::Subcommand;

use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ProcSubCommands {

    /// get logs
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(short = 'a', help = "Any process includes")]
        any_process: bool,
        
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


pub(crate) fn on_process(manifest: Value, working_dir: &Path, subcommand: ProcSubCommands, args: Args) -> JuizResult<()> {
    match on_process_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}")
    };
    Ok(())
}
pub(crate) fn on_process_inner(manifest: Value, working_dir: &Path, subcommand: ProcSubCommands, args: Args) -> JuizResult<()> {

    let server = args.server.clone();
    let recursive = args.recursive;
    match subcommand {
        ProcSubCommands::List { any_process, filepath} => {
            log::trace!("process list command is selected. args={args:?}");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                if any_process {
                    on_any_process_list(system, Some(server), recursive)
                } else {
                    on_process_list(system, Some(server), recursive)
                } 
            }) 
        },
        ProcSubCommands::Info { identifier } => {
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                on_process_info(system, identifier)
            }) 
        },
        ProcSubCommands::Call { identifier, argument , fileout, print} => {
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                on_process_call(system, identifier, argument, fileout, print)
            }) 
        } 
    }
}

fn on_process_list(system: &mut System, _server: Option<String>, recursive: bool) -> JuizResult<()> {
    log::info!("on_process_list() called");
    let proc_manifests = system.process_list(recursive)?;
    let mut ids: Vec<String> = Vec::new();
    for v in proc_manifests.iter() {
        ids.push(v.to_string());
    }
    println!("{ids:?}");
    Ok(())
}

fn on_any_process_list(system: &mut System, _server: Option<String>, recursive: bool) -> JuizResult<()> {
    let proc_manifests = system.any_process_list(recursive)?;
    let mut ids: Vec<String> = Vec::new();
    for v in proc_manifests.iter() {
        ids.push(v.to_string());
    }
    println!("{ids:?}");
    Ok(())
}

fn on_process_info(system: &mut System, id: String) -> JuizResult<()> {
    //println!("processes:");
    let pid = id.try_into()?;
    let p = system.core_broker().lock_mut()?.worker_mut().any_process_from_identifier(&pid, true);
    match p {
        Ok(ps) => println!("{:}", ps.lock()?.profile()?),
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
        } );
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


fn on_process_call(system: &mut System, id: String, arg: String, _fileout: Option<String>, print: bool) -> JuizResult<()> {
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