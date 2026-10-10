//! juizローダーアプリ


mod process;
mod execution_context;
mod setup;
mod default_juiz_conf;
mod container;
mod container_process;
mod connection;

use std::path::PathBuf;
use std::time::Duration;

use connection::{ConnectionSubCommands, on_connection};
use execution_context::{on_execution_context, EcSubCommands};
use container::{on_container, ContSubCommands};
use container_process::{on_container_process, ContProcSubCommands};

use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;
use juiz_core::{ env_logger, log};
use crate::process::{on_process, ProcSubCommands};
use crate::setup::{on_setup, SetupSubCommands};

use clap::{Parser, Subcommand};


/// Simple program to greet a person
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[clap(
    name = "juiz",
    author = "Yuki Suga",
    version = "v1.0.0",
    about = "JUIZ command-line tool"
)]
struct Args {
    /// Name of the person to greet
    #[arg(short = 'd', help = "Daemonize JUIZ server. This option automatically enables http server (-b option). If you want to supress, use -q option.")]
    daemonize: bool,

    #[arg(short = 'w', default_value="true", help = "Recursively walk subsystems.")]
    recursive: bool,


    #[arg(short = 'r', help = "Ratio of periodical execution. If this option is set, created object will periodically executed under the ratio you set [Hz]")]
    ratio: Option<f64>,

    #[arg(short = 'q', default_value="false", help = "Stop HTTP Broker. Default(false). This option is used with -d option only. If you use this with -b option, http server will start.")]
    stop_http_broker: bool,

    #[arg(short = 'b', help = "Start HTTP Broker. Default(false)")]
    start_http_broker: bool,

    #[arg(short = 'f', default_value = "./juiz.conf", help = "Input system definition file path")]
    filepath: String,

    #[arg(short = 's', long = "server", default_value = "http://127.0.0.1:8000", help = "Host of server (ex., http://localhost:8000)")]
    server: String,

    #[arg(short = 'C', long = "cwd", default_value = ".", help = "Set current working directory.")]
    cwd: String,

    #[arg(long = "process", help = "ProcessModule loader mode.")]
    process: Option<String>,

    #[arg(long = "container", help = "ContainerModule loader mode.")]
    container: Option<String>,

    #[arg(long = "container_process", help = "ContainerProcessModule loader mode.")]
    container_process: Option<String>,

    #[arg(long = "component", help = "ComponentModule loader mode.")]
    component: Option<String>,

    #[arg(short = 'l', help = "Module Language", default_value="rust")]
    module_language: String,

    #[arg(short = '1', long = "c1", help = "Create one instance for evely manually loaded module", default_value="false")]
    module_create: bool,

    #[arg(short = 'c', long = "create", value_delimiter=',', help = "Pass typeName to create one instance for evely manually loaded module")]
    create_instance: Vec<String>,


    #[arg(short = 'e', help = "Execute manually loaded module once", default_value="false")]
    module_execute: bool,

    #[arg(long = "print", help = "print output of execute. Use -e option with this.", default_value="false")]
    module_execute_print: bool,

    #[arg(short = 'm', help = "Print manifest of loaded module", default_value="false")]
    module_manifest_print: bool,

    #[clap(subcommand)]
    subcommand: Option<SubCommands>,
}


#[derive(Debug, Subcommand, Clone)]
enum SubCommands {
    // Setup tools
    #[clap(arg_required_else_help = false)]
    Setup {
        #[clap(subcommand)]
        subcommand: SetupSubCommands
    },

    // Process tools
    #[clap(arg_required_else_help = false)]
    Process {
        #[clap(subcommand)]
        subcommand: ProcSubCommands
    },

    // Container tools
    #[clap(arg_required_else_help = false)]
    Container {
        #[clap(subcommand)]
        subcommand: ContSubCommands
    },

    // Container tools
    #[clap(arg_required_else_help = false)]
    ContainerProcess {
        #[clap(subcommand)]
        subcommand: ContProcSubCommands
    },

    // Execution Context tools
    #[clap(arg_required_else_help = false)]
    Ec {
        #[clap(subcommand)]
        subcommand: EcSubCommands
    },

    // Connection tools
    #[clap(arg_required_else_help = false)]
    Connection {
        #[clap(subcommand)]
        subcommand: ConnectionSubCommands
    },
}


fn do_task_once(system: &mut System, args: Args) -> JuizResult<()> {
    // println!("System started once");
    let language = args.module_language;
    let create_every = args.module_create;
    let ratio = args.ratio;
    let execute = if ratio.is_some() {
        true
    } else {
        args.module_execute
    };let print = args.module_execute_print;
    let module_manifest_print = args.module_manifest_print;
    let create_instance = args.create_instance;
    if let Some(process_path) = args.process {
        let pm: ProcessManifest = serde_json::from_value(system.core_broker().lock_mut()?.system_load_process(language, process_path)?)?;
        if module_manifest_print {
            println!("{pm:?}");
        }
        if create_every {
            let process_proxy = create_process_by_pm(system, &pm)?;
            if execute {
                let v = process_proxy.lock_mut()?.execute()?;
                if print {
                    println!("{v:?}");
                }
            }
        } else {
            for create_type_name in create_instance.iter() {
                if pm.type_name == create_type_name.as_str() {
                    let process_proxy = create_process_by_pm(system, &pm)?;
                    if execute {
                        let v = process_proxy.lock_mut()?.execute()?;
                        if print {
                            println!("{v:?}");
                        }
                    }
                }
            }
        }
    } else if let Some(container_path) = args.container {
        let cm: ContainerManifest = serde_json::from_value(system.core_broker().lock_mut()?.system_load_container(language.clone(), container_path)?)?;
        let mut container_id = None;
        if module_manifest_print {
            println!("{cm:?}");
        }
        if create_every {
            let container_ptr = create_container_by_cm(system, &cm)?;
            container_id = Some(container_ptr.identifier().clone());
        } else {
            for create_type_name in create_instance.iter() {
                if cm.type_name == create_type_name.as_str() {
                    let container_ptr = create_container_by_cm(system, &cm)?;
                    container_id = Some(container_ptr.identifier().clone());
                    break;
                }
            }
        }
        if let Some(container_process_path) = args.container_process {
            let pm: ProcessManifest = serde_json::from_value(system.core_broker().lock_mut()?.system_load_container_process(language, container_process_path)?)?;
            if module_manifest_print {
                println!("{pm:?}");
            }
            if create_every {
                let process_proxy = create_container_process_by_cid_and_pm(system, &container_id.unwrap(), &pm)?;
                if execute {
                    let v = process_proxy.lock_mut()?.execute()?;
                    if print {
                        println!("{v:?}");
                    }
                }
            } else {
                if let Some(cid) = container_id {
                    for create_type_name in create_instance.iter() {
                        if pm.type_name == create_type_name.as_str() {
                            let process_proxy = create_container_process_by_cid_and_pm(system, &cid, &pm)?;
                            if execute {
                                let v = process_proxy.lock_mut()?.execute()?;
                                if print {
                                    println!("{v:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    } else if let Some(component_path) = args.component {
        let compm: ComponentManifest = system.core_broker().lock_mut()?.system_load_component(language, component_path)?.try_into()?;
        if module_manifest_print {
            println!("{compm:}");
        }
        for pm in compm.processes.iter() {
            let proc = create_process_by_pm(system, pm)?;
            if execute {
                let v = proc.lock_mut()?.execute()?;
                if print {
                    println!("{v:?}");
                }
            }
        }
        for cm in compm.containers.iter() {
            let cont = create_container_by_cm(system, cm)?;
            let cid = cont.identifier().clone();
            for pm in cm.processes.iter() {
                let cproc = create_container_process_by_cid_and_pm(system, &cid, pm)?;
                if execute {
                    let v = cproc.lock_mut()?.execute()?;
                    if print {
                        println!("{v:?}");
                    }
                }
            }
        }
    }
    Ok(())
}



fn do_task(system: &mut System, args: Args) -> JuizResult<()> {
    // println!("System started");
    let language = args.module_language;
    let create = args.module_create;
    let ratio = args.ratio;
    let execute = if ratio.is_some() {
        true
    } else {
        args.module_execute
    };
    let print = args.module_execute_print;
    let module_manifest_print = args.module_manifest_print;
    if let Some(process_path) = args.process {
        let pm: ProcessManifest = serde_json::from_value(system.core_broker().lock_mut()?.system_load_process(language, process_path)?)?;
        if module_manifest_print {
            println!("{pm:?}");
        }
        if create {
            let type_name = pm.type_name;
            let prof = system.core_broker().lock_mut()?.process_create(&ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()))?;
            if execute {
                let identifier = prof.identifier();
                //let identifier = obj_get_str(&prof, "identifier")?;
                let process_proxy = system.core_broker().lock()?.worker().process_from_identifier(&identifier, true)?;
                if let Some(ratio_hz) = ratio {
                    let duration = Duration::from_secs_f64(1.0 / ratio_hz);
                    loop {
                        // let ratio_sec = (1.0 / ratio_hz).floor() as u64;
                        // let ratio_nanosec = ((1.0 / ratio_hz).fract() * 1000000000) as u64;

                        std::thread::sleep(duration);
                        let v = process_proxy.lock_mut()?.execute()?;
                        if print {
                            println!("{v:?}");
                        }
                    }
                } else {
                    let v = process_proxy.lock_mut()?.execute()?;
                    if print {
                        println!("{v:?}");
                    }
                }
            }
        }
    }  else if let Some(container_path) = args.container {
        let cm: ContainerManifest = serde_json::from_value(system.core_broker().lock_mut()?.system_load_container(language.clone(), container_path)?)?;
        let mut container_id = None;
        if module_manifest_print {
            println!("{cm:?}");
        }if create {
            let type_name = cm.type_name.clone();
            let name = format!("{}0", type_name);
            //let  mut cp = CapsuleMap::new();
            //cp.insert("name".to_owned(), jvalue!(name).into());
            //cp.insert("type_name".to_owned(), jvalue!(type_name).into());
            let cm2 = cm.clone().name(name.as_str());
            let cont_prof = system.core_broker().lock_mut()?.container_create(&cm2, CapsuleMap::new())?;
            //container_id = Some(obj_get_str(&cont_prof, "identifier")?.to_owned());
            container_id = Some(cont_prof.identifier());
        }
        if let Some(container_process_path) = args.container_process {
            let pm: ProcessManifest = serde_json::from_value(system.core_broker().lock_mut()?.system_load_container_process(language, container_process_path)?)?;
            if module_manifest_print {
                println!("{pm:?}");
            }
            if create {
                let type_name = pm.type_name;
                let prof = system.core_broker().lock_mut()?.container_process_create(&container_id.unwrap(), &ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()))?;
                if execute {
                    // let identifier = obj_get_str(&prof, "identifier")?;
                    let identifier = prof.identifier();
                    let process_proxy = system.core_broker().lock()?.worker().any_process_from_identifier(&identifier, true)?;
                    if let Some(ratio_hz) = ratio {
                        let duration = Duration::from_secs_f64(1.0 / ratio_hz);
                        loop {
                            std::thread::sleep(duration);
                            let v = process_proxy.lock_mut()?.execute()?;
                            if print {
                                println!("{v:?}");
                            }
                        }
                    } else {
                        let v = process_proxy.lock_mut()?.execute()?;
                        if print {
                            println!("{v:?}");
                        }
                    }
                }
            }
        }
    } else if let Some(component_path) = args.component {
        let compm: ComponentManifest = system.core_broker().lock_mut()?.system_load_component(language, component_path)?.try_into()?;
        if module_manifest_print {
            println!("{compm:}");
        }
        let mut procs: Vec<ProcessPtr> = Vec::new();
        let mut conts: Vec<ContainerPtr> = Vec::new();
        let mut cont_procs: Vec<ProcessPtr> = Vec::new();
        loop {
            for pm in compm.processes.iter() {
                if create {
                    let proc = create_process_by_pm(system, pm)?;
                    procs.push(proc.clone());
                    if execute {
                        let v = proc.lock_mut()?.execute()?;
                        if print {
                            println!("{v:?}");
                        }
                    }
                }
            }
            for cm in compm.containers.iter() {
                if create {
                    let cont = create_container_by_cm(system, cm)?;
                    conts.push(cont.clone());
                    let cid = cont.identifier().clone();
                    for pm in cm.processes.iter() {
                        let cproc = create_container_process_by_cid_and_pm(system, &cid, pm)?;
                        cont_procs.push(cproc.clone());
                        if execute {
                            let v = cproc.lock_mut()?.execute()?;
                            if print {
                                println!("{v:?}");
                            }
                        }
                    }
                }
            }
            if let Some(ratio_hz) = ratio {
                let duration = Duration::from_secs_f64(1.0 / ratio_hz);
                std::thread::sleep(duration);
                for p in procs.iter() {
                    let v = p.lock_mut()?.execute()?;
                    if print {
                        println!("{v:?}");
                    }
                }
                for cp in cont_procs.iter() {
                    let v = cp.lock_mut()?.execute()?;
                    if print {
                        println!("{v:?}");
                    }
                }
            } else {
                break;
            }
        }// loop
    }
    Ok(())
}

fn create_process_by_pm(system: &mut System, pm: &ProcessManifest) -> JuizResult<ProcessPtr> {
    let type_name = pm.type_name.clone();
    let prof = system.core_broker().lock_mut()?.process_create(&ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()))?;
    //let pid = Some(obj_get_str(&prof, "identifier")?.to_owned());
    let pid = Some(prof.identifier());
    system.core_broker().lock_mut()?.worker_mut().process_from_identifier(&pid.unwrap(), true)
}

fn create_container_by_cm(system: &mut System, cm: &ContainerManifest) -> JuizResult<ContainerPtr> {
    //let type_name = cm.type_name.clone();
    let name = format!("{}0", cm.type_name);
    let cm2 = cm.clone().name(name.as_str());
    let cp = CapsuleMap::new();
    // cp.insert("name".to_owned(), jvalue!(name).into());
    // cp.insert("type_name".to_owned(), jvalue!(type_name).into());

    let cont_prof = system.core_broker().lock_mut()?.container_create(&cm2, cp)?;
    // let container_id = Some(obj_get_str(&cont_prof, "identifier")?.to_owned());
    let container_id = Some(cont_prof.identifier());
    system.core_broker().lock_mut()?.worker_mut().container_from_identifier(&container_id.unwrap())
}

fn create_container_process_by_cid_and_pm(system: &mut System, cid: &ContainerIdentifier, pm: &ProcessManifest) -> JuizResult<ProcessPtr> {
    let type_name = pm.type_name.clone();
    let prof = system.core_broker().lock_mut()?.container_process_create(&cid, &ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()))?;
    // let pid = Some(obj_get_str(&prof, "identifier")?.to_owned());
    let pid = prof.identifier();
    system.core_broker().lock_mut()?.worker_mut().any_process_from_identifier(&pid, true)
}

fn main() -> () {
    let _ = env_logger::try_init();
    match do_once() {
        Ok(_) => (),
        Err(e) => println!("Error:{:?}", e)
    }
}

fn do_once() -> JuizResult<()>{
    log::trace!("【呼出】main::do_once called");
    let args = Args::parse();
    let manifest = yaml_conf_load(args.filepath.clone())?;
    let flag_start = if args.daemonize { true } else { args.start_http_broker };
    let manifest_filepath = PathBuf::from(args.filepath.as_str().to_string());
    let working_dir = manifest_filepath.parent().unwrap();
    let ratio = args.ratio;
    // サブコマンドが指定されていない場合は単純に起動。
    if args.subcommand.is_none() {
        //let daemonize = ratio.is_some() || args.daemonize;
        if args.daemonize || ratio.is_some() {
            return System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(flag_start)
                .setup()?
                //.add_subsystem_by_id(Some(server))?
                .run_and_do(|system| { 
                    do_task(system, args)
                });
        } else {
            return System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(flag_start)
                .setup()?
                //.add_subsystem_by_id(Some(server))?
                .run_and_do_once(|system| {
                    do_task_once(system, args)
                });
        }
    }
    // サブコマンドがある場合
    let command = args.clone().subcommand.unwrap();
    match command {
        SubCommands::Setup{ subcommand } => { 
            on_setup(manifest, subcommand, args)
        },
        SubCommands::Process { subcommand } => {
            on_process(manifest, working_dir, subcommand, args)
        },
        SubCommands::Container { subcommand } => {
            on_container(manifest, working_dir, subcommand, args)
        },
        SubCommands::ContainerProcess { subcommand } => {
            on_container_process(manifest, working_dir, subcommand, args)
        },
        SubCommands::Ec { subcommand } => {
            on_execution_context(manifest, working_dir, subcommand, args)
        },
        SubCommands::Connection { subcommand } => {
            on_connection(manifest, working_dir, subcommand, args)
        },
        /* _ => {
            return Ok(())
        } */
    }
}