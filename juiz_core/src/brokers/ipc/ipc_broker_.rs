use anyhow::Context;
use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions, Stream,
};
use std::io::{self, prelude::*, BufReader};
use std::{
    cell::RefCell,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use crate::brokers::{
    BrokerFactory, MessengerBroker, MessengerBrokerCore, MessengerBrokerCoreFactory,
};
use crate::{
    brokers::{
        create_broker_factory_impl, create_messenger_broker_factory, BrokerProxy, CRUDBrokerHolder,
    },
    jvalue,
    processes::capsule::{Capsule, CapsuleMap},
    utils::juiz_lock,
    value::{obj_get_i64, obj_get_str},
    CapsulePtr, CoreBroker, JuizError, JuizResult, Value,
};

pub struct ByteSenderReceiverPair(pub mpsc::Sender<Vec<u8>>, pub mpsc::Receiver<Vec<u8>>);
pub struct BrokerSideSenderReceiverPair(
    pub mpsc::Sender<CapsulePtr>,
    pub mpsc::Receiver<CapsuleMap>,
);
pub struct ProxySideSenderReceiverPair(
    pub mpsc::Sender<CapsuleMap>,
    pub mpsc::Receiver<CapsulePtr>,
);
pub type LocalBroker = MessengerBroker;

#[allow(dead_code)]
pub struct IPCBrokerCore {
    //sender_receiver: Arc<Mutex<BrokerSideSenderReceiverPair>>,
    //byte_sender_receiver: Arc<Mutex<ByteSenderReceiverPair>>,
    //stream: Stream,
    buf_reader: RefCell<BufReader<Stream>>,
    buf_size: usize,
}

impl IPCBrokerCore {
    pub fn new(stream: Stream, buf_size: usize) -> JuizResult<Arc<Mutex<dyn MessengerBrokerCore>>> {
        log::trace!("IPCBrokerCore::new(buf_size={buf_size}) called");
        Ok(Arc::new(Mutex::new(IPCBrokerCore {
            //stream: stream,
            buf_reader: RefCell::new(BufReader::new(stream)),
            buf_size,
        })))
    }
}

impl MessengerBrokerCore for IPCBrokerCore {
    fn receive_and_send(
        &self,
        _timeout: Duration,
        func: Arc<Mutex<dyn Fn(CapsuleMap) -> JuizResult<CapsulePtr>>>,
    ) -> JuizResult<Capsule> {
        let mut buffer = String::with_capacity(self.buf_size);
        self.buf_reader.borrow_mut().read_line(&mut buffer)?;
        let value: CapsuleMap = Value::from(buffer).try_into()?;

        log::trace!("IPCBrokerCore::receive_and_send() received some data.");
        let ret_value = match (juiz_lock(&func)?)(value) {
            Ok(v) => v,
            Err(e) => {
                log::error!("User function call in MessengerBrokerCore::receive_and_send() failed. Error is {}", e.to_string());
                return Err(e);
            }
        };

        log::trace!("IPCBrokerCore now sending back data.");
        ret_value.lock_as_value(|v| {
            match self
                .buf_reader
                .borrow_mut()
                .get_mut()
                .write_all((v.to_string() + "\n").as_bytes())
            {
                Err(e) => {
                    log::error!("Error({e:?}) in IPCBroker::routine()");
                    log::trace!("IPCBrokerCore::receive_and_send() exit");
                    Err(anyhow::Error::from(JuizError::BrokerSendError {}))
                }
                Ok(()) => {
                    log::trace!("IPCBrokerCore collectly sent data.");
                    log::trace!("IPCBrokerCore::receive_and_send() exit");
                    Ok(jvalue!({}).into())
                }
            }
        })?
    }
}

pub struct IPCBrokerCoreFactory {
    //sender_receiver: Arc<Mutex<BrokerSideSenderReceiverPair>>,
    //byte_sender_receiver: Arc<Mutex<ByteSenderReceiverPair>>,
}

impl IPCBrokerCoreFactory {
    pub fn new() -> Box<Self> {
        //sender_receiver: Arc<Mutex<BrokerSideSenderReceiverPair>>, byte_sender_receiver: Arc<Mutex<ByteSenderReceiverPair>>) -> Box<LocalBrokerCoreFactory> {
        Box::new(Self {})
    }
}

impl MessengerBrokerCoreFactory for IPCBrokerCoreFactory {
    fn create(&self, manifest: &Value) -> JuizResult<Arc<Mutex<dyn MessengerBrokerCore>>> {
        log::trace!("IPCBrokerCoreFactory::create(manifest={manifest}) called");
        let mut namespace = obj_get_str(manifest, "namespace")
            .or::<JuizError>(Ok("example.sock"))?
            .to_owned();
        let buf_size = obj_get_i64(manifest, "buffer_size").or::<JuizError>(Ok(4096))? as usize;
        // Pick a name.
        let name = if GenericNamespaced::is_supported() {
            namespace.to_ns_name::<GenericNamespaced>()?
        } else {
            namespace = format!("/tmp/{:}", namespace);
            namespace.as_str().to_fs_name::<GenericFilePath>()?
        };
        log::trace!("IPBrokerCore (namespace={:?})", name);
        let opts = ListenerOptions::new().name(name);
        // ...then create it.
        let listener = match opts.create_sync() {
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
                eprintln!(
                    "Error: could not start server because the socket file is occupied. Please check if
                    {namespace} is in use by another process and try again."
                );
                return Err(e);
            }
            x => x,
        }?;

        // Define a function that checks for errors in incoming connections. We'll use this to filter
        // through connections that fail on initialization for one reason or another.
        fn handle_error(conn: io::Result<Stream>) -> Option<Stream> {
            match conn {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("Incoming connection failed: {e}");
                    None
                }
            }
        }

        //let conn = Stream::connect(name).with_context(|| format!("Stream::connect() in IPCBrokerCoreFactory::create()"))?;

        let mut buffer = String::with_capacity(buf_size);
        for conn in listener.incoming().filter_map(handle_error) {
            // Wrap the connection into a buffered receiver right away
            // so that we could receive a single line from it.
            let mut conn = BufReader::new(conn);
            println!("Incoming connection!");

            // Since our client example sends first, the server should receive a line and only then
            // send a response. Otherwise, because receiving from and sending to a connection cannot
            // be simultaneous without threads or async, we can deadlock the two processes by having
            // both sides wait for the send buffer to be emptied by the other.
            conn.read_line(&mut buffer)?;

            // Now that the receive has come through and the client is waiting on the server's send, do
            // it. (`.get_mut()` is to get the sender, `BufReader` doesn't implement a pass-through
            // `Write`.)
            conn.get_mut().write_all(b"Hello from server!\n")?;

            // Print out the result, getting the newline for free!
            print!("Client answered: {buffer}");

            // Clear the buffer so that the next iteration will display new data instead of messages
            // stacking on top of one another.
            buffer.clear();
        }

        IPCBrokerCore::new(conn, buf_size)
    }
}

fn broker_factory(
    core_broker: Arc<Mutex<CoreBroker>>,
) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    // env_logger::init();
    create_messenger_broker_factory(
        "IPCBrokerProxyFactory",
        "ipc",
        core_broker,
        IPCBrokerCoreFactory::new(),
    )
}

pub fn create_ipc_broker_factory(
    core_broker: Arc<Mutex<CoreBroker>>,
) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    broker_factory(core_broker)
}
