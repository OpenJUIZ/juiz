use std::io::prelude::*;
use std::{
    cell::RefCell,
    io::BufReader,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::prelude::*;
use interprocess::local_socket::{prelude::*, GenericFilePath, Stream};

use crate::brokers::messenger_broker_proxy_factory::create_messenger_broker_proxy_factory;

//use super::ipc_broker::ProxySideSenderReceiverPair;

use crate::brokers::{
    BrokerProxyFactory, MessengerBrokerProxyCore, MessengerBrokerProxyCoreFactory,
};

// pub type IPCBrokerProxy = MessengerBrokerProxy;

#[allow(unused)]
pub struct IPCBrokerProxyCore {
    name: String,
    buf_reader: RefCell<BufReader<Stream>>,
    buf_size: usize,
    //sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>>,
}

impl IPCBrokerProxyCore {
    //pub fn new(name: &str, buf_reader: BufReader<Stream>, buf_size: usize) -> IPCBrokerProxyCore {
    //    IPCBrokerProxyCore{name: name.to_owned(), buf_reader: RefCell::new(buf_reader), buf_size}
    // }
}

pub struct IPCBrokerProxyCoreFactory {
    //sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>>,
}

impl IPCBrokerProxyCoreFactory {
    pub fn new() -> JuizResult<Box<dyn MessengerBrokerProxyCoreFactory>> {
        Ok(Box::new(IPCBrokerProxyCoreFactory {}))
    }
}

impl MessengerBrokerProxyCoreFactory for IPCBrokerProxyCoreFactory {
    fn create_core(&self, object_name: &str) -> JuizResult<Box<dyn MessengerBrokerProxyCore>> {
        /* let name = if GenericNamespaced::is_supported() {
            object_name.to_ns_name::<GenericNamespaced>()?
        } else {
            object_name.to_fs_name::<GenericFilePath>()?
        }; */
        let name = format!("/tmp/{:}", object_name).to_fs_name::<GenericFilePath>()?;
        let buf_size: usize = 4096;
        let conn = Stream::connect(name)?;
        Ok(Box::new(IPCBrokerProxyCore {
            name: object_name.to_owned(),
            buf_reader: RefCell::new(BufReader::new(conn)),
            buf_size,
        }))
    }
}

impl MessengerBrokerProxyCore for IPCBrokerProxyCore {
    fn send_and_receive(&self, value: CapsuleMap, _timeout: Duration) -> JuizResult<CapsulePtr> {
        log::trace!("IPCBrokerProxyCore::send_and_receive(value={value:?}) called");
        let mut buffer = String::with_capacity(self.buf_size);
        let v: Value = value.into();
        let strv = v.to_string() + "\n";
        log::trace!(" - payload = {strv:}");
        self.buf_reader
            .borrow_mut()
            .get_mut()
            .write_all(strv.as_bytes())?;

        self.buf_reader.borrow_mut().read_line(&mut buffer)?;
        Ok(juiz_sdk::serde_json::from_str::<Value>(buffer.as_str())?.into())
        //Ok(Value::from(buffer).try_into()?)
    }
}

pub fn create_ipc_broker_proxy_factory() -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    log::trace!("create_ipc_broker_factory called");
    create_messenger_broker_proxy_factory(
        "IPCBrokerProxyFactory",
        "ipc",
        IPCBrokerProxyCoreFactory::new()?,
    )
}
