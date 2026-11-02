//
// juiz connect http://localhost:8001/rust_talker0 http://localhost:8000/rust_listener0/arg1 --type push

use clap::Subcommand;
use std::path::Path;

use juiz_core::log;
use juiz_core::prelude::*;
use juiz_sdk::connection_identifier::ConnectionIdentifier;

use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ConnectionSubCommands {
    #[clap(arg_required_else_help = false)]
    Create {
        #[arg(
            long = "type",
            short = 't',
            help = "Type of connection. Default is PUSH",
            default_value = "push"
        )]
        connection_type: String,

        #[arg(help = "ID of source process")]
        source: String,

        #[arg(help = "Inlet name of destination process")]
        arg_name: String,

        #[arg(help = "ID of destination process")]
        destination: String,
    },

    /// Delete a connection by an ID from `connection list`, or by <src> <arg> <dst> as in create
    #[clap(arg_required_else_help = true)]
    Delete {
        #[arg(
            num_args = 1..=3,
            help = "ID of connection (connection://<src>|<arg>|<dst>), or <src> <arg> <dst> as in create"
        )]
        connection: Vec<String>,
    },

    /// List connections. IDs are printed as a JSON array and can be passed to delete
    #[clap(arg_required_else_help = false)]
    List {
        #[arg(
            long = "long",
            help = "Show source, arg and destination of each connection"
        )]
        long: bool,
    },
}

pub(crate) fn on_connection(
    manifest: Value,
    working_dir: &Path,
    subcommand: ConnectionSubCommands,
    args: Args,
) -> JuizResult<()> {
    match on_connection_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}"),
    };
    Ok(())
}

pub(crate) fn on_connection_inner(
    manifest: Value,
    working_dir: &Path,
    subcommand: ConnectionSubCommands,
    args: Args,
) -> JuizResult<()> {
    let server = args.server.clone();
    let recursive = true; // args.recursive;
    match subcommand {
        ConnectionSubCommands::Create {
            connection_type,
            source,
            arg_name,
            destination,
        } => {
            log::trace!("connection connect command is selected. args={args:?}");
            //let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| {
                    on_connection_create(system, source, arg_name, destination, connection_type)
                })
        }
        ConnectionSubCommands::Delete { connection } => {
            log::trace!("connection delete command is selected. args={args:?}");
            let id = connection_identifier_from_args(connection)?;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .run_and_do_once(|system| on_connection_delete(system, server, id))
        }
        ConnectionSubCommands::List { long } => {
            log::trace!("connection list command is selected. args={args:?}");
            // let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                //.add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once(|system| on_connection_list(system, server, recursive, long))
        }
    }
}

fn on_connection_create(
    system: &mut System,
    source_id: String,
    arg_name: String,
    destination_id: String,
    connection_type: String,
) -> JuizResult<()> {
    log::info!("Connecting {source_id} -({arg_name}:{connection_type:})-> {destination_id}");
    // push / pull 以外は Unknown になって、そのまま繋がってしまっていた
    let typ: ConnectionType = connection_type.as_str().into();
    if typ == ConnectionType::Unknown {
        return Err(juiz_core::anyhow::Error::from(
            JuizError::ConnectionTypeError {
                type_string: connection_type,
            },
        ));
    }
    let manifest = ConnectionManifest::new(
        typ,
        source_id.try_into()?,
        arg_name,
        destination_id.try_into()?,
        // None,
    );
    system
        .core_broker()
        .lock_mut()?
        .connection_create(&manifest)?;
    Ok(())
}

/// delete の引数を接続の ID にする。1 個なら connection://<src>|<arg>|<dst>、3 個なら <src> <arg> <dst>
fn connection_identifier_from_args(args: Vec<String>) -> JuizResult<ConnectionIdentifier> {
    match <[String; 3]>::try_from(args) {
        Ok([source, arg_name, destination]) => Ok(ConnectionIdentifier::new(
            source.try_into()?,
            arg_name.as_str(),
            destination.try_into()?,
        )),
        Err(args) if args.len() == 1 => args[0].clone().try_into(),
        Err(args) => Err(juiz_core::anyhow::Error::from(
            JuizError::InvalidConnectionIdentifierError {
                identifier: args.join(" "),
            },
        )),
    }
}

/// -s のサーバーのブローカーの proxy
fn server_broker_proxy(
    system: &mut System,
    server: String,
) -> JuizResult<std::sync::Arc<std::sync::RwLock<dyn BrokerProxy>>> {
    let id_struct = IdentifierStruct::new_broker_id(server.clone())?;
    system.create_broker_proxy(&serde_json::json!({
        "identifier": server,
        "type_name": id_struct.broker_type_name,
        "name": id_struct.broker_name,
    }))
}

fn on_connection_delete(
    system: &mut System,
    server: String,
    id: ConnectionIdentifier,
) -> JuizResult<()> {
    log::info!("Connection delete {id}");
    let proxy = server_broker_proxy(system, server)?;
    juiz_write(&proxy)?.connection_destroy(&id)?;
    Ok(())
}

fn on_connection_list(
    system: &mut System,
    server: String,
    _recursive: bool,
    long: bool,
) -> JuizResult<()> {
    log::info!("Connection list");
    let proxy = server_broker_proxy(system, server)?;
    let con_list = juiz_borrow(&proxy)?.connection_list(true)?;
    // 同じ接続は接続元と接続先の両方から返ってくるので 1 つにまとめる
    let mut ids: Vec<ConnectionIdentifier> = Vec::new();
    for id in con_list.into_iter() {
        if !ids.iter().any(|i| i.is_same_connection(&id)) {
            ids.push(id);
        }
    }
    if long {
        for id in ids.iter() {
            println!(
                "{}  -({})->  {}",
                id.source_identifier, id.arg_name, id.destination_identifier
            );
        }
    } else {
        let strs: Vec<String> = ids.into_iter().map(|id| id.into()).collect();
        println!("{}", serde_json::to_string(&strs)?);
    }
    Ok(())
}
