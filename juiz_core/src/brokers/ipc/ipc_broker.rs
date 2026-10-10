use std::{sync::{Arc, Mutex}, thread::sleep, time::Duration};

use juiz_sdk::anyhow;

use super::super::core_broker::CoreBrokerPtr;
use crate::{brokers::broker_ptr::BrokerPtr, prelude::*};
use crate::brokers::{broker_factory_impl::create_broker_factory_impl, BrokerFactory, CRUDBrokerHolder};
use crate::brokers::CRUDBroker;
use interprocess::local_socket::{prelude::*, traits::Stream, GenericFilePath, GenericNamespaced, ListenerOptions};
use std::io::{self, prelude::*, BufReader};


async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    /* 
    match on_start_inner(broker_manifest, crud_broker).await {
        Err(e) => {
            log::error!("IPCBroker::on_start_inner() error: {:?}", e);
        },
        Ok(_) => {

        }
    }
    */
    match on_start_inner_tokio(broker_manifest, crud_broker).await {
        Err(e) => {
            log::error!("IPCBroker::on_start_inner_tokio() error: {:?}", e);
        },
        Ok(_) => {

        }
    }
}

fn extract_method_name(args: & CapsuleMap) -> JuizResult<&String> {
    let err = |name: &str | anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError{ name: name.to_owned() });
    Ok(args.get_param("method_name").ok_or_else( || err("method_name") )?)
}


fn extract_class_name<'a>(args: &'a CapsuleMap) -> JuizResult<String> {
    // method_name, class_name, function_name, params
    let err = |name: &str | anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError{ name: name.to_owned() });
    let class_name = args.get_param("class_name").ok_or_else( || err("class_name") )?;
    Ok(class_name.to_owned())
}


fn extract_function_name<'a>(args: &'a CapsuleMap) -> JuizResult<&'a String> {
    let err = |name: &str | anyhow::Error::from(JuizError::CapsuleDoesNotIncludeParamError{ name: name.to_owned() });
    let function_name = args.get_param("function_name").ok_or_else( || err("function_name") )?;
    Ok(function_name)
}

fn handle_function(crud_broker: Arc<Mutex<CRUDBroker>>, args: CapsuleMap) -> JuizResult<CapsulePtr> {
    log::info!("MessengerBroker::handle_function() called");
    log::trace!(" - args: CapsuleMap = {args:?}");
    let class_name = extract_class_name(&args)?;
    let function_name = extract_function_name(&args)?.to_owned();
    //log::trace!("MessengerBroker::handle_function() with method_name = {method_name:}");
    let response = match  extract_method_name(&args)?.as_str() {
        "CREATE" => juiz_lock(&crud_broker)?.create_class(class_name.as_str(),  function_name.as_str(), args),
        "READ" =>  juiz_lock(&crud_broker)?.read_class(class_name.as_str(), function_name.as_str(), args),
        "UPDATE" =>  juiz_lock(&crud_broker)?.update_class(class_name.as_str(), function_name.as_str(), args),
        "DELETE" => juiz_lock(&crud_broker)?.delete_class(class_name.as_str(), function_name.as_str(), args),
        _ => {
            Err(anyhow::Error::from(JuizError::CRUDBRokerCanNotFindMethodError{method_name: "".to_owned()}))
        }
    };
    log::trace!(" - returns: {response:?}");
    response
}


fn handle_buffer_function(crud_broker: Arc<Mutex<CRUDBroker>>, conn: &mut BufReader<LocalSocketStream>, buffer: &String) -> JuizResult<()> {
    let value: Value = handle_buffer(crud_broker, buffer)?;
    println!("vout{:?}, vstr{:}", value, (value.to_string() + "\n"));
    match conn.get_mut().write_all((value.to_string() + "\n").as_bytes()) {
        Err(e) => {
            log::error!("Error({e:?}) in IPCBroker::routine()");
            log::trace!("IPCBrokerCore::receive_and_send() exit");
            Err(anyhow::Error::from(JuizError::BrokerSendError{}))
        },
        Ok(()) => {
            log::trace!("IPCBrokerCore collectly sent data.");
            log::trace!("IPCBrokerCore::receive_and_send() exit");
            Ok(())
        }
    }
}

fn handle_buffer(crud_broker: Arc<Mutex<CRUDBroker>>, buffer: &String) -> JuizResult<Value> {
    let value: CapsuleMap = juiz_sdk::serde_json::from_str::<Value>(buffer.as_str())?.try_into()?;
    let result = handle_function(crud_broker.clone(), value)?;
    capsule_to_value(result)
}

async fn handle_conn(crud_broker: &Arc<Mutex<CRUDBroker>>, mut conn: interprocess::local_socket::tokio::Stream) -> io::Result<()> {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt},
        try_join,
    };
    let mut recver = tokio::io::BufReader::new(&conn);
    //let mut sender = conn;

    // Allocate a sizeable buffer for receiving. This size should be big enough and easy to
    // find for the allocator.
    let mut buffer = String::with_capacity(128);
    let recv = recver.read_line(&mut buffer);
    try_join!(recv)?;
    let value = handle_buffer(crud_broker.clone(), &mut buffer).or_else(|_e| { Err(io::Error::new(io::ErrorKind::Other, "handle_buffer failed.")) })?;
    //let value: CapsuleMap = serde_json::from_str::<Value>(buffer.as_str())?.try_into()?;
    //let result = handle_function(crud_broker.clone(), value)?;
    //let value: Value =  capsule_to_value(result)?;
    println!("vout{:?}, vstr{:}", value, (value.to_string() + "\n"));
    let vstr = (value.to_string() + "\n").to_owned();
    let send = conn.write_all(vstr.as_bytes());
    // Run both operations concurrently.
    try_join!(send)?;
    Ok(())
}

async fn on_start_inner_tokio(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> JuizResult<()> {
    use interprocess::local_socket::tokio::prelude::*;
    log::trace!("ipc_broker::on_start_tokio() called");
    let mut namespace = obj_get_str(&broker_manifest, "namespace").or::<&str>(Ok("juiz2.sock") ).unwrap().to_owned();
    log::info!("ipc_broker::on_start(namespace={namespace}, {broker_manifest:?})) called");
//    let buf_size = obj_get_i64(&broker_manifest, "buffer_size").or::<JuizError>(Ok(4096))? as usize;
    let name = if GenericNamespaced::is_supported() {
        //namespace.to_ns_name::<GenericNamespaced>()? 
        namespace = format!("/tmp/{:}", namespace);
        namespace.as_str().to_fs_name::<GenericFilePath>()?
    } else {
        namespace = format!("/tmp/{:}", namespace);
        namespace.as_str().to_fs_name::<GenericFilePath>()?
    };
    log::trace!("IPBrokerCore (namespace={:?})", name);
    let opts = ListenerOptions::new().name(name);

    let listener = match opts.create_tokio() {
		Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
			// When a program that uses a file-type socket name terminates its socket server
			// without deleting the file, a "corpse socket" remains, which can neither be
			// connected to nor reused by a new listener. Normally, Interprocess takes care of
			// this on affected platforms by deleting the socket file when the listener is
			// dropped. (This is vulnerable to all sorts of races and thus can be disabled.)
			//
			// There are multiple ways this error can be handled, if it occurs, but when the
			// listener only comes from Interprocess, it can be assumed that its previous instance
			// either has crashed or simply hasn't exited yet. In this example, we leave cleanup
			// up to the user, but in a real application, you usually don't want to do that.
			
			return Err(e.into());
		}
		x => x?,
	};

    loop {
        let conn = match listener.accept().await {
			Ok(c) => c,
			Err(e) => {
				eprintln!("There was an error with an incoming connection: {e}");
				continue;
			}
		};
        let crud = crud_broker.clone();

        // Spawn new parallel asynchronous tasks onto the Tokio runtime and hand the connection
		// over to them so that multiple clients could be processed simultaneously in a
		// lightweight fashion.
		tokio::spawn(async move {
			// The outer match processes errors that happen when we're connecting to something.
			// The inner if-let processes errors that happen during the connection.
			if let Err(e) = handle_conn(&crud, conn).await {
				eprintln!("Error while handling connection: {e}");
			}
		});
    }
    //Ok(())
}

#[allow(unused)]
async fn on_start_inner(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> JuizResult<()> {
    log::trace!("ipc_broker::on_start() called");
    let mut namespace = obj_get_str(&broker_manifest, "namespace").or::<&str>(Ok("juiz2.sock") ).unwrap().to_owned();
    log::info!("ipc_broker::on_start(namespace={namespace}, {broker_manifest:?})) called");
    let buf_size = obj_get_i64(&broker_manifest, "buffer_size").or::<JuizError>(Ok(4096))? as usize;
    // Pick a name.
    let name = if GenericNamespaced::is_supported() {
        //namespace.to_ns_name::<GenericNamespaced>()? 
        namespace = format!("/tmp/{:}", namespace);
        namespace.as_str().to_fs_name::<GenericFilePath>()?
    } else {
        namespace = format!("/tmp/{:}", namespace);
        namespace.as_str().to_fs_name::<GenericFilePath>()?
    };
    log::trace!("IPBrokerCore (namespace={:?})", name);
    let opts = ListenerOptions::new().name(name);

    // ...then create it.
    let listener = match opts.create_sync() {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {

            log::error!("ipc broker: Error. Address In Use.");
            // When a program that uses a file-type socket name terminates its socket server
            // without deleting the file, a "corpse socket" remains, which can neither be
            // connected to nor reused by a new listener. Normally, Interprocess takes care of
            // this on affected platforms by deleting the socket file when the listener is
            // dropped. (This is vulnerable to all sorts of races and thus can be disabled.)
            //
            // There are multiple ways this error can be handled, if it occurs, but when the
            // listener only comes from Interprocess, it can be assumed that its previous instance
            // either has crashed or simply hasn't exited yet. In this example, we leave cleanup
            // up to the user, but in a real application, you usually don't want to do that.
            //eprintln!(
            //    "Error: could not start server because the socket file is occupied. Please check if
            //    {namespace} is in use by another process and try again."
            //);
            return Err(anyhow::Error::from(e));
        }
        x => x,
    }?;

    // Define a function that checks for errors in incoming connections. We'll use this to filter
    // through connections that fail on initialization for one reason or another.
    fn handle_error(conn: io::Result<LocalSocketStream>) -> Option<LocalSocketStream> {
        log::warn!("IPC Broker handle_error() called");
        match conn {
            Ok(c) => {
                
                Some(c)
            },
            Err(e) => {
                eprintln!("Incoming connection failed: {e}");
                None
            }
        }
    }

    //let conn = Stream::connect(name).with_context(|| format!("Stream::connect() in IPCBrokerCoreFactory::create()"))?;

    for conn in listener.incoming().filter_map(handle_error) {
        // Wrap the connection into a buffered receiver right away
        // so that we could receive a single line from it.
        log::info!("incomming connection detected");

        conn.set_nonblocking(true)?;
        let mut conn = BufReader::new(conn);
        //let mut endflag = 
        loop {
            let mut buffer = String::with_capacity(buf_size);
            
            match conn.read_line(&mut buffer) {
                Err(e) => {
                    log::error!("Error. {:?}", e)
                }
                Ok (size) => {
                    if size == 0 {
                        log::info!(" - result_size: {:?}", size);
                        break;
                    }
                    handle_buffer_function(crud_broker.clone(), &mut conn, &buffer)?;
                }
            };
            sleep(Duration::from_millis(10));
        }
    }

    Ok(())
}

pub fn create_ipc_broker_factory(core_broker: CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    //env_logger::init();

    fn create_broker_function(core_broker: CoreBrokerPtr, manifest: Value) -> JuizResult<BrokerPtr> {
        Ok(BrokerPtr::new(CRUDBrokerHolder::new("IPCBroker", "ipc", core_broker, &on_start, manifest.clone())?))
    }

    let manifest = jvalue!({
        "type_name": "ipc"
    });
    create_broker_factory_impl(core_broker, manifest, create_broker_function)
}

