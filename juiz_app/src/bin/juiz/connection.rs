//
// juiz connect http://localhost:8001/rust_talker0 http://localhost:8000/rust_listener0/arg1 --type push 


use std::path::Path;
use clap::Subcommand;

use juiz_core::log;
use juiz_core::prelude::*;

use crate::Args;

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum ConnectionSubCommands {

    #[clap(arg_required_else_help = false)]
    Create {
        #[arg(long = "type", short = 't', help = "Type of connection. Default is PUSH", default_value="push")]
        connection_type: String,

        #[arg(help = "ID of source process")]
        source: String,

        #[arg(help = "Inlet name of destination process")]
        arg_name: String,

        #[arg(help = "ID of destination process")]
        destination: String
    },

    #[clap(arg_required_else_help = false)]
    Delete {
        
        #[arg(help = "ID of Conection")]
        connection_id: String
    },

    /// get logs
    #[clap(arg_required_else_help = false)]
    List {
    }
}

pub(crate) fn on_connection(manifest: Value, working_dir: &Path, subcommand: ConnectionSubCommands, args: Args) -> JuizResult<()> {
    match on_connection_inner(manifest, working_dir, subcommand, args) {
        Ok(_) => return Ok(()),
        Err(e) => println!("Error: {e:?}")
    };
    Ok(())
}

pub(crate) fn on_connection_inner(manifest: Value, working_dir: &Path, subcommand: ConnectionSubCommands, args: Args) -> JuizResult<()> {
    let server = args.server.clone();
    let recursive = true;// args.recursive;
    match subcommand {
        ConnectionSubCommands::Create { connection_type, source, arg_name, destination} => {
            log::trace!("connection connect command is selected. args={args:?}");
            //let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                    on_connection_create(system, source, arg_name, destination, connection_type)
                }
            ) 
        },
        #[allow(unused)]
        ConnectionSubCommands::Delete { connection_id} => {
            log::trace!("connection connect command is selected. args={args:?}");
            //let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                .add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                // if any_process {
                //     //on_any_process_list(system, Some(server), recursive)
                // } else {
                //     //on_process_list(system, Some(server), recursive)
                // } 
                    Ok(())
                }
            ) 
        },
        ConnectionSubCommands::List {} => {
            log::trace!("connection list command is selected. args={args:?}");
            // let manifest2 = yaml_conf_load(filepath.clone())?;
            System::new(manifest)?
                .set_working_dir(working_dir)
                .start_http_broker(args.start_http_broker)
                .setup()?
                //.add_systemproxy_by_id(Some(server.clone()))?
                .run_and_do_once( |system| { 
                    on_connection_list(system, server, recursive)
                }
            ) 
        }
    }
}


fn on_connection_create(system: &mut System, source_id: String, arg_name: String, destination_id: String, connection_type: String) -> JuizResult<()> {
    log::info!("Connecting {source_id} -({arg_name}:{connection_type:})-> {destination_id}");
    let manifest = ConnectionManifest::new(
        connection_type.as_str().try_into()?,
        source_id.try_into()?,
        arg_name,
        destination_id.try_into()?,
        // None,
    );
    system.core_broker().lock_mut()?.connection_create(&manifest)?;
    Ok(())
}

fn on_connection_list(system: &mut System, server: String, _recursive: bool) -> JuizResult<()> {
    log::info!("Connection list");
    //let con_list = system.core_broker().lock()?.connection_list(recursive)?;
    let id_struct = IdentifierStruct::new_broker_id(server.clone())?;
    let proxy = system.create_broker_proxy(&serde_json::json!({
        "identifier": server,
        "type_name": id_struct.broker_type_name,
        "name": id_struct.broker_name,
    }))?;
    let con_list = proxy.lock().unwrap().connection_list(true)?;

    println!("[{}]", con_list.iter().map(|id| { id.to_string() }).collect::<Vec<String>>().join(","));
    Ok(())
}
