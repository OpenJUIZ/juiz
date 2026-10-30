//! juiz コマンドの本体。
//!
//! `juiz` コマンド(`src/bin/juiz/main.rs`)はここの [`main_with_args`] を呼ぶだけになっている。
//! Python から起動する `juiz`(pip で入る版、issue #88)も同じ関数を使い、
//! そのときは feature `python-entry` の [`python::main_with_args`] から GIL を解放して呼ぶ。

mod composite;
mod composition;
mod connection;
mod container;
mod container_process;
mod default_juiz_conf;
mod execution_context;
mod namespace;
mod package;
mod process;
mod setup;
mod types;

#[cfg(feature = "python-entry")]
pub mod python;

use std::path::PathBuf;
use std::time::Duration;

use composite::{on_composite, CompositeSubCommands};
use connection::{on_connection, ConnectionSubCommands};
use container::{on_container, ContSubCommands};
use container_process::{on_container_process, ContProcSubCommands};
use execution_context::{on_execution_context, EcSubCommands};

use crate::process::{on_process, ProcSubCommands};
use crate::setup::{on_setup, SetupSubCommands};
use crate::types::{on_type, TypeSubCommands};
use juiz_core::prelude::*;
use juiz_core::utils::yaml_conf_load;
use juiz_core::{env_logger, log};
use std::ffi::OsString;

use clap::{Parser, Subcommand};

/// Simple program to greet a person
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[clap(
    name = "juiz",
    author = "Yuki Suga",
    version = env!("CARGO_PKG_VERSION"),
    about = "JUIZ command-line tool"
)]
struct Args {
    /// Name of the person to greet
    #[arg(
        short = 'd',
        help = "Daemonize JUIZ server. This option automatically enables http server (-b option). If you want to supress, use -q option."
    )]
    daemonize: bool,

    #[arg(
        short = 'w',
        default_value = "true",
        help = "Recursively walk subsystems."
    )]
    recursive: bool,

    #[arg(
        short = 'r',
        help = "Ratio of periodical execution. If this option is set, created object will periodically executed under the ratio you set [Hz]"
    )]
    ratio: Option<f64>,

    #[arg(
        short = 'q',
        default_value = "false",
        help = "Stop HTTP Broker. Default(false). This option is used with -d option only. If you use this with -b option, http server will start."
    )]
    stop_http_broker: bool,

    #[arg(short = 'b', help = "Start HTTP Broker. Default(false)")]
    start_http_broker: bool,

    #[arg(
        short = 'f',
        default_value = "./juiz.conf",
        help = "Input system definition file path"
    )]
    filepath: String,

    #[arg(
        long = "ipc",
        help = "Start an IPC broker with this name (ex., cam0) for processes on the same machine. Clients connect with -s ipc://<name>."
    )]
    ipc: Option<String>,

    #[arg(
        long = "namespace",
        help = "Default namespace of the system. Overrides 'namespace' in the system definition file (ex., robot1). Use '/' for the root."
    )]
    namespace: Option<String>,

    #[arg(
        short = 's',
        long = "server",
        default_value = "http://127.0.0.1:8000",
        help = "Host of server (ex., http://localhost:8000)"
    )]
    server: String,

    #[arg(
        short = 'C',
        long = "cwd",
        default_value = ".",
        help = "Set current working directory."
    )]
    cwd: String,

    #[arg(long = "process", help = "ProcessModule loader mode.")]
    process: Option<String>,

    #[arg(long = "container", help = "ContainerModule loader mode.")]
    container: Option<String>,

    #[arg(
        long = "container_process",
        help = "ContainerProcessModule loader mode."
    )]
    container_process: Option<String>,

    #[arg(long = "component", help = "ComponentModule loader mode.")]
    component: Option<String>,

    #[arg(short = 'l', help = "Module Language", default_value = "rust")]
    module_language: String,

    #[arg(
        short = '1',
        long = "c1",
        help = "Create one instance for evely manually loaded module",
        default_value = "false"
    )]
    module_create: bool,

    #[arg(
        short = 'c',
        long = "create",
        value_delimiter = ',',
        help = "Pass typeName to create one instance for evely manually loaded module"
    )]
    create_instance: Vec<String>,

    #[arg(
        short = 'e',
        help = "Execute manually loaded module once",
        default_value = "false"
    )]
    module_execute: bool,

    #[arg(
        long = "print",
        help = "print output of execute. Use -e option with this.",
        default_value = "false"
    )]
    module_execute_print: bool,

    #[arg(
        short = 'm',
        help = "Print manifest of loaded module",
        default_value = "false"
    )]
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
        subcommand: SetupSubCommands,
    },

    // Process tools
    #[clap(arg_required_else_help = false)]
    Process {
        #[clap(subcommand)]
        subcommand: ProcSubCommands,
    },

    // Container tools
    #[clap(arg_required_else_help = false)]
    Container {
        #[clap(subcommand)]
        subcommand: ContSubCommands,
    },

    // Container tools
    #[clap(arg_required_else_help = false)]
    ContainerProcess {
        #[clap(subcommand)]
        subcommand: ContProcSubCommands,
    },

    // Execution Context tools
    #[clap(arg_required_else_help = false)]
    Ec {
        #[clap(subcommand)]
        subcommand: EcSubCommands,
    },

    // Connection tools
    #[clap(arg_required_else_help = false)]
    Connection {
        #[clap(subcommand)]
        subcommand: ConnectionSubCommands,
    },

    // Composite tools (issue #171)
    #[clap(arg_required_else_help = false)]
    Composite {
        #[clap(subcommand)]
        subcommand: CompositeSubCommands,
    },

    // Type tools (issue #197)
    #[clap(arg_required_else_help = false)]
    Type {
        #[clap(subcommand)]
        subcommand: TypeSubCommands,
    },

    /// Generate schemas and code from juiz IDL (.jidl) files (same as juiz-idl, issue #191)
    Idl {
        #[command(flatten)]
        args: juiz_idl::cli::IdlArgs,
    },

    /// Pack a directory with juizapp.yaml into a .juizapp package (docs/spec/app-package.md §6)
    Pack {
        /// Directory that has juizapp.yaml
        dir: String,

        /// Output file or directory (default: <name>-<version>.juizapp in the current directory)
        #[arg(short = 'o', long = "output")]
        output: Option<String>,

        /// Put only the plugins of this target (ex., x86_64-unknown-linux-gnu). Can be repeated.
        #[arg(long = "target")]
        target: Vec<String>,
    },

    /// Check, extract and run a .juizapp package or an installed package (docs/spec/app-package.md §7)
    Run {
        /// .juizapp file, or the name of an installed package (name or name@version)
        package: String,

        /// conf in the package to start with (default: 'entry' in juizapp.yaml)
        #[arg(long = "entry")]
        entry: Option<String>,

        #[arg(short = 'b', help = "Start HTTP Broker. By default, option.http_broker in the conf decides.")]
        start_http_broker: bool,

        #[arg(long = "ipc", help = "Start an IPC broker with this name (same as juiz --ipc).")]
        ipc: Option<String>,

        #[arg(long = "namespace", help = "Default namespace of the system (same as juiz --namespace).")]
        namespace: Option<String>,

        /// Where to connect a dependency: <name>=<URL> (ex., robot_base=ipc://robot_base2). Can be repeated.
        #[arg(long = "depend")]
        depend: Vec<String>,
    },

    /// Install a .juizapp package (docs/spec/app-package.md §8)
    Install {
        /// .juizapp file
        package: String,
    },

    /// Remove an installed package: name (all versions) or name@version
    Uninstall {
        package: String,
    },

    /// List installed packages
    Packages {},
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
    };
    let print = args.module_execute_print;
    let module_manifest_print = args.module_manifest_print;
    let create_instance = args.create_instance;
    if let Some(process_path) = args.process {
        let pm: ProcessManifest = serde_json::from_value(
            system
                .core_broker()
                .lock_mut()?
                .system_load_process(language, process_path)?,
        )?;
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
        let cm: ContainerManifest = serde_json::from_value(
            system
                .core_broker()
                .lock_mut()?
                .system_load_container(language.clone(), container_path)?,
        )?;
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
            let pm: ProcessManifest = serde_json::from_value(
                system
                    .core_broker()
                    .lock_mut()?
                    .system_load_container_process(language, container_process_path)?,
            )?;
            if module_manifest_print {
                println!("{pm:?}");
            }
            if create_every {
                let process_proxy =
                    create_container_process_by_cid_and_pm(system, &container_id.unwrap(), &pm)?;
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
                            let process_proxy =
                                create_container_process_by_cid_and_pm(system, &cid, &pm)?;
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
        let compm: ComponentManifest = system
            .core_broker()
            .lock_mut()?
            .system_load_component(language, component_path)?
            .try_into()?;
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
        let pm: ProcessManifest = serde_json::from_value(
            system
                .core_broker()
                .lock_mut()?
                .system_load_process(language, process_path)?,
        )?;
        if module_manifest_print {
            println!("{pm:?}");
        }
        if create {
            let type_name = pm.type_name;
            let prof = system.core_broker().lock_mut()?.process_create(
                &ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()),
            )?;
            if execute {
                let identifier = prof.identifier();
                //let identifier = obj_get_str(&prof, "identifier")?;
                let process_proxy = system
                    .core_broker()
                    .lock()?
                    .worker()
                    .process_from_identifier(&identifier, true)?;
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
    } else if let Some(container_path) = args.container {
        let cm: ContainerManifest = serde_json::from_value(
            system
                .core_broker()
                .lock_mut()?
                .system_load_container(language.clone(), container_path)?,
        )?;
        let mut container_id = None;
        if module_manifest_print {
            println!("{cm:?}");
        }
        if create {
            let type_name = cm.type_name.clone();
            let name = format!("{}0", type_name);
            //let  mut cp = CapsuleMap::new();
            //cp.insert("name".to_owned(), jvalue!(name).into());
            //cp.insert("type_name".to_owned(), jvalue!(type_name).into());
            let cm2 = cm.clone().name(name.as_str());
            let cont_prof = system
                .core_broker()
                .lock_mut()?
                .container_create(&cm2, CapsuleMap::new())?;
            //container_id = Some(obj_get_str(&cont_prof, "identifier")?.to_owned());
            container_id = Some(cont_prof.identifier());
        }
        if let Some(container_process_path) = args.container_process {
            let pm: ProcessManifest = serde_json::from_value(
                system
                    .core_broker()
                    .lock_mut()?
                    .system_load_container_process(language, container_process_path)?,
            )?;
            if module_manifest_print {
                println!("{pm:?}");
            }
            if create {
                let type_name = pm.type_name;
                let prof = system.core_broker().lock_mut()?.container_process_create(
                    &container_id.unwrap(),
                    &ProcessManifest::new(type_name.as_str())
                        .name(format!("{}0", type_name).as_str()),
                )?;
                if execute {
                    // let identifier = obj_get_str(&prof, "identifier")?;
                    let identifier = prof.identifier();
                    let process_proxy = system
                        .core_broker()
                        .lock()?
                        .worker()
                        .any_process_from_identifier(&identifier, true)?;
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
        let compm: ComponentManifest = system
            .core_broker()
            .lock_mut()?
            .system_load_component(language, component_path)?
            .try_into()?;
        if module_manifest_print {
            println!("{compm:}");
        }
        // 作るのは最初の 1 回だけ。-r のときは作ったものを周期で実行する
        // (毎周期作ると 2 周目に同じ名前の二重登録でエラーになっていた)
        let mut procs: Vec<ProcessPtr> = Vec::new();
        if create {
            for pm in compm.processes.iter() {
                procs.push(create_process_by_pm(system, pm)?);
            }
            for cm in compm.containers.iter() {
                let cont = create_container_by_cm(system, cm)?;
                let cid = cont.identifier().clone();
                for pm in cm.processes.iter() {
                    procs.push(create_container_process_by_cid_and_pm(system, &cid, pm)?);
                }
            }
        }
        loop {
            if execute {
                for p in procs.iter() {
                    let v = p.lock_mut()?.execute()?;
                    if print {
                        println!("{v:?}");
                    }
                }
            }
            match ratio {
                Some(ratio_hz) => std::thread::sleep(Duration::from_secs_f64(1.0 / ratio_hz)),
                None => break,
            }
        }
    }
    Ok(())
}

fn create_process_by_pm(system: &mut System, pm: &ProcessManifest) -> JuizResult<ProcessPtr> {
    let type_name = pm.type_name.clone();
    let prof = system.core_broker().lock_mut()?.process_create(
        &ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()),
    )?;
    //let pid = Some(obj_get_str(&prof, "identifier")?.to_owned());
    let pid = Some(prof.identifier());
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .process_from_identifier(&pid.unwrap(), true)
}

fn create_container_by_cm(system: &mut System, cm: &ContainerManifest) -> JuizResult<ContainerPtr> {
    //let type_name = cm.type_name.clone();
    let name = format!("{}0", cm.type_name);
    let cm2 = cm.clone().name(name.as_str());
    let cp = CapsuleMap::new();
    // cp.insert("name".to_owned(), jvalue!(name).into());
    // cp.insert("type_name".to_owned(), jvalue!(type_name).into());

    let cont_prof = system
        .core_broker()
        .lock_mut()?
        .container_create(&cm2, cp)?;
    // let container_id = Some(obj_get_str(&cont_prof, "identifier")?.to_owned());
    let container_id = Some(cont_prof.identifier());
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .container_from_identifier(&container_id.unwrap())
}

fn create_container_process_by_cid_and_pm(
    system: &mut System,
    cid: &ContainerIdentifier,
    pm: &ProcessManifest,
) -> JuizResult<ProcessPtr> {
    let type_name = pm.type_name.clone();
    let prof = system.core_broker().lock_mut()?.container_process_create(
        &cid,
        &ProcessManifest::new(type_name.as_str()).name(format!("{}0", type_name).as_str()),
    )?;
    // let pid = Some(obj_get_str(&prof, "identifier")?.to_owned());
    let pid = prof.identifier();
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .any_process_from_identifier(&pid, true)
}

/// コマンドライン引数(先頭はプログラム名)を受け取って juiz を実行し、終了コードを返す。
///
/// 引数の誤りや `--help` は clap のメッセージを表示して、それに応じた終了コードを返す。
/// プロセスは終了させないので、Python から呼んだときも呼び出し元に戻る。
pub fn main_with_args<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let _ = env_logger::try_init();
    #[cfg(feature = "qmp-broker")]
    qmp_broker::register();
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(e) => {
            let _ = e.print();
            return e.exit_code();
        }
    };
    // juiz idl は系を作らないので、設定ファイルを読まずに juiz-idl と同じように動かす
    if let Some(SubCommands::Idl { args }) = &args.subcommand {
        return match juiz_idl::cli::run(args) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        };
    }
    // juiz pack / juiz run も系の conf (-f) を読まない
    match package_command(&args) {
        Some(Ok(_)) => return 0,
        Some(Err(e)) => {
            eprintln!("error: {e:#}");
            return 1;
        }
        None => {}
    }
    match run(args) {
        Ok(_) => 0,
        Err(e) => {
            println!("Error:{:?}", e);
            1
        }
    }
}

/// `juiz pack`・`juiz run`・`juiz install` などパッケージのコマンドを実行する。どれでもなければ None
fn package_command(args: &Args) -> Option<JuizResult<()>> {
    match args.subcommand.as_ref()? {
        SubCommands::Pack { dir, output, target } => Some(
            package::pack::pack(&package::pack::PackOptions {
                dir: PathBuf::from(dir),
                output: output.as_ref().map(PathBuf::from),
                targets: target.clone(),
            })
            .map(|p| {
                let m = &p.manifest;
                println!(
                    "Packed {} {} ({}; {} files) into {}",
                    m.name,
                    m.version,
                    m.targets.as_deref().unwrap_or_default().join(", "),
                    m.files.as_ref().map(|f| f.len()).unwrap_or(0),
                    p.path.display()
                );
            }),
        ),
        SubCommands::Run {
            package,
            entry,
            start_http_broker,
            ipc,
            namespace,
            depend,
        } => Some(package::run::run_package(&package::run::RunOptions {
            package: package.clone(),
            entry: entry.clone(),
            start_http_broker: *start_http_broker || args.start_http_broker,
            ipc: ipc.clone().or_else(|| args.ipc.clone()),
            namespace: namespace.clone().or_else(|| args.namespace.clone()),
            depends: depend.clone(),
        })),
        SubCommands::Install { package } => Some((|| {
            use package::install::{install, InstallOutcome};
            let data = package::install::data_dir()?;
            match install(std::path::Path::new(package), &data, &package::run::Host::current())? {
                InstallOutcome::Installed(i) => println!(
                    "Installed {} {} into {}",
                    i.manifest.name,
                    i.manifest.version,
                    i.dir.display()
                ),
                InstallOutcome::AlreadyInstalled(i) => println!(
                    "{} {} is already installed in {}",
                    i.manifest.name,
                    i.manifest.version,
                    i.dir.display()
                ),
            }
            Ok(())
        })()),
        SubCommands::Uninstall { package } => Some((|| {
            let data = package::install::data_dir()?;
            for i in package::install::uninstall(package, &data)? {
                println!("Uninstalled {} {}", i.manifest.name, i.manifest.version);
            }
            Ok(())
        })()),
        SubCommands::Packages {} => Some((|| {
            let data = package::install::data_dir()?;
            let all = package::install::list(&data)?;
            if all.is_empty() {
                println!(
                    "No packages are installed in {}",
                    data.join("packages").display()
                );
            } else {
                print!("{}", package::install::format_table(&all));
            }
            Ok(())
        })()),
        _ => None,
    }
}

/// --ipc <name> で、conf の brokers に IPC ブローカーを足す (docs/spec/ipc-broker.md §4.2)
fn add_ipc_broker(mut manifest: Value, ipc: Option<&str>) -> JuizResult<Value> {
    let Some(name) = ipc else {
        return Ok(manifest);
    };
    let obj = manifest
        .as_object_mut()
        .ok_or_else(|| juiz_core::anyhow::anyhow!("the system definition file must be a map"))?;
    let brokers = obj.entry("brokers").or_insert_with(|| jvalue!([]));
    if brokers.is_null() {
        *brokers = jvalue!([]);
    }
    brokers
        .as_array_mut()
        .ok_or_else(|| juiz_core::anyhow::anyhow!("'brokers' in the system definition file must be a list"))?
        .push(jvalue!({ "type_name": "ipc", "name": name }));
    Ok(manifest)
}

fn run(args: Args) -> JuizResult<()> {
    log::trace!("【呼出】juiz_app::run called");
    let mut manifest = namespace::override_namespace(
        yaml_conf_load(args.filepath.clone())?,
        args.namespace.as_deref(),
    )?;
    // profile の package は juiz run でパッケージから起動したときだけ載せる (docs/spec/app-package.md §10.1)
    if let Some(obj) = manifest.as_object_mut() {
        obj.remove("package");
    }
    let manifest = add_ipc_broker(manifest, args.ipc.as_deref())?;
    let flag_start = if args.daemonize {
        true
    } else {
        args.start_http_broker
    };
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
                .run_and_do(|system| do_task(system, args));
        } else {
            return System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(flag_start)
                .setup()?
                //.add_subsystem_by_id(Some(server))?
                .run_and_do_once(|system| do_task_once(system, args));
        }
    }
    // サブコマンドがある場合
    let command = args.clone().subcommand.unwrap();
    match command {
        SubCommands::Setup { subcommand } => on_setup(manifest, subcommand, args),
        SubCommands::Process { subcommand } => on_process(manifest, working_dir, subcommand, args),
        SubCommands::Container { subcommand } => {
            on_container(manifest, working_dir, subcommand, args)
        }
        SubCommands::ContainerProcess { subcommand } => {
            on_container_process(manifest, working_dir, subcommand, args)
        }
        SubCommands::Ec { subcommand } => {
            on_execution_context(manifest, working_dir, subcommand, args)
        }
        SubCommands::Connection { subcommand } => {
            on_connection(manifest, working_dir, subcommand, args)
        }
        SubCommands::Composite { subcommand } => {
            on_composite(manifest, working_dir, subcommand, args)
        }
        SubCommands::Type { subcommand } => on_type(manifest, working_dir, subcommand, args),
        SubCommands::Idl { .. }
        | SubCommands::Pack { .. }
        | SubCommands::Run { .. }
        | SubCommands::Install { .. }
        | SubCommands::Uninstall { .. }
        | SubCommands::Packages { .. } => {
            unreachable!()
        }
        /* _ => {
            return Ok(())
        } */
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_ipc_broker_appends_to_brokers() {
        // --ipc が無ければそのまま
        let m = jvalue!({ "name": "a" });
        assert_eq!(add_ipc_broker(m.clone(), None).unwrap(), m);
        // brokers が無い / null / ある
        for m in [
            jvalue!({ "name": "a" }),
            jvalue!({ "name": "a", "brokers": null }),
        ] {
            let m = add_ipc_broker(m, Some("cam0")).unwrap();
            assert_eq!(m["brokers"], jvalue!([{ "type_name": "ipc", "name": "cam0" }]));
        }
        let m = add_ipc_broker(
            jvalue!({ "brokers": [{ "type_name": "qmp", "name": "q" }] }),
            Some("cam0"),
        )
        .unwrap();
        assert_eq!(m["brokers"].as_array().unwrap().len(), 2);
        assert_eq!(m["brokers"][1]["type_name"], "ipc");
    }

    #[test]
    fn add_ipc_broker_rejects_invalid_conf() {
        assert!(add_ipc_broker(jvalue!([]), Some("cam0")).is_err());
        assert!(add_ipc_broker(jvalue!({ "brokers": "x" }), Some("cam0")).is_err());
    }
}
