use juiz_core::anyhow::anyhow;
use juiz_core::log;
use juiz_core::prelude::juiz_lock;
use juiz_core::utils::yaml_conf_load;
use std::path::Path;

use clap::Subcommand;
use juiz_core::prelude::*;

use crate::namespace::{format_list, ListEntry};
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

        #[arg(
            long = "namespace",
            help = "List only objects in this namespace and below (ex., robot1)"
        )]
        namespace: Option<String>,

        #[arg(long = "long", help = "Show namespace, name and type columns")]
        long: bool,
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

    #[clap(arg_required_else_help = false)]
    Stop {
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
        EcSubCommands::List {
            filepath,
            namespace,
            long,
        } => {
            log::trace!("ec list command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            let recursive = args.recursive;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_ec_list(system, recursive, namespace, long))
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
                .run_and_do_once(|system| on_ec_start(system, server, identifier))
        }

        EcSubCommands::Stop {
            identifier,
            filepath,
        } => {
            log::trace!("ec stop command is selected.");
            let manifest2 = yaml_conf_load(filepath.clone())?;
            let server = args.server;
            System::new(manifest2)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_ec_stop(system, server, identifier))
        }
    }
}

fn on_ec_list(
    system: &mut System,
    recursive: bool,
    namespace: Option<String>,
    long: bool,
) -> JuizResult<()> {
    log::info!("on_ec_list() called");
    let ec_ids: Vec<Value> = system.ec_list(recursive)?;
    let mut entries: Vec<ListEntry> = Vec::new();
    for v in ec_ids.iter() {
        let id = v.as_str().ok_or_else(|| {
            anyhow!(JuizError::InvalidValueError {
                message: format!("execution context identifier must be a string, but {v}"),
            })
        })?;
        entries.push(id.try_into()?);
    }
    println!("{}", format_list(entries, namespace.as_deref(), long)?);
    Ok(())
}

fn on_ec_start(system: &mut System, server: String, id: String) -> JuizResult<()> {
    if let Err(e) = ec_start_or_stop(system, &server, &id, true) {
        println!("Error: {e:?}");
    }
    Ok(())
}

fn on_ec_stop(system: &mut System, server: String, id: String) -> JuizResult<()> {
    if let Err(e) = ec_start_or_stop(system, &server, &id, false) {
        println!("Error: {e:?}");
    }
    Ok(())
}

/// 識別子で指定した EC を start (start = true) または stop (start = false) する。
///
/// http://<host>:<port>/... の識別子はブローカープロキシ経由で起動中の juiz の EC を指す (issue #50)。
/// core://core/... の識別子はまずコマンド側のローカルシステムから探し、
/// 見つからなければ -s で指定したサーバー上の同じ EC として扱う。
fn ec_start_or_stop(system: &mut System, server: &str, id: &str, start: bool) -> JuizResult<Value> {
    let ec = {
        let mut core_broker = system.core_broker().lock_mut()?;
        let worker = core_broker.worker_mut();
        match worker.ec_proxy_from_identifier(&id.to_owned()) {
            Ok(ec) => ec,
            Err(e) => match server_identifier(server, id) {
                Some(remote_id) => worker.ec_proxy_from_identifier(&remote_id)?,
                None => return Err(e),
            },
        }
    };
    let mut ec = juiz_lock(&ec)?;
    if start {
        ec.start()
    } else {
        ec.stop()
    }
}

/// core://core/<path> を <server>/<path> に書き換える。core://core/ 以外の識別子では None。
fn server_identifier(server: &str, id: &str) -> Option<String> {
    let path = id.strip_prefix("core://core/")?;
    Some(format!("{}/{path}", server.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::server_identifier;

    #[test]
    fn server_identifier_rewrites_core_identifier() {
        assert_eq!(
            server_identifier(
                "http://127.0.0.1:8000",
                "core://core/execution_context/ec0::main_loop_ec"
            ),
            Some("http://127.0.0.1:8000/execution_context/ec0::main_loop_ec".to_owned())
        );
        assert_eq!(
            server_identifier(
                "http://127.0.0.1:8000/",
                "core://core/execution_context/ec0::main_loop_ec"
            ),
            Some("http://127.0.0.1:8000/execution_context/ec0::main_loop_ec".to_owned())
        );
    }

    #[test]
    fn server_identifier_keeps_other_identifiers() {
        assert_eq!(
            server_identifier(
                "http://127.0.0.1:8000",
                "http://127.0.0.1:8000/execution_context/ec0::main_loop_ec"
            ),
            None
        );
    }
}
