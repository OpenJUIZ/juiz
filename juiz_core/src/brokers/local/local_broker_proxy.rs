use std::{
    ops::Deref,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::brokers::messenger_broker_proxy_factory::create_messenger_broker_proxy_factory;
use crate::prelude::*;
use juiz_sdk::anyhow;

use super::local_broker::ProxySideSenderReceiverPair;

use crate::brokers::{
    BrokerProxyFactory, MessengerBrokerProxyCore, MessengerBrokerProxyCoreFactory,
};

//pub type LocalBrokerProxy = MessengerBrokerProxy;
pub struct LocalBrokerProxyCore {
    sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>>,
}

pub struct LocalBrokerProxyCoreFactory {
    sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>>,
}

impl LocalBrokerProxyCoreFactory {
    pub fn new(
        sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>>,
    ) -> JuizResult<Box<dyn MessengerBrokerProxyCoreFactory>> {
        Ok(Box::new(LocalBrokerProxyCoreFactory { sender_receiver }))
    }
}

impl MessengerBrokerProxyCoreFactory for LocalBrokerProxyCoreFactory {
    fn create_core(&self, _object_name: &str) -> JuizResult<Box<dyn MessengerBrokerProxyCore>> {
        Ok(Box::new(LocalBrokerProxyCore {
            sender_receiver: self.sender_receiver.clone(),
        }))
    }
}

impl MessengerBrokerProxyCore for LocalBrokerProxyCore {
    fn send_and_receive(&self, value: CapsuleMap, timeout: Duration) -> JuizResult<CapsulePtr> {
        let us = timeout.as_micros();
        log::trace!("LocaBrokerProxyCore::send_and_receive(timeout_us={us}) called");
        let sndr_recvr = self.sender_receiver.lock().map_err(|_e| {
            return anyhow::Error::from(JuizError::BrokerSendCanNotLockSenderError {});
        })?;
        let ProxySideSenderReceiverPair(sndr, recvr) = sndr_recvr.deref();
        let _ = sndr
            .send(value)
            .map_err(|_e| return anyhow::Error::from(JuizError::LocalBrokerProxySendError {}))?;
        let result = recvr.recv_timeout(timeout).map_err(|e| {
            log::error!("LocalBrokerProxyCore::send_and_receive() failed. Error is {e:}");
            return anyhow::Error::from(JuizError::LocalBrokerProxyReceiveTimeoutError {
                error: e,
            });
        });
        log::trace!("LocaBrokerProxyCore::send_and_receive() exit");
        result
    }
}

impl LocalBrokerProxyCore {
    // pub fn new(sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>> ) -> LocalBrokerProxyCore {
    //     LocalBrokerProxyCore{sender_receiver}
    // }
}

pub fn create_local_broker_proxy_factory(
    sender_receiver: Arc<Mutex<ProxySideSenderReceiverPair>>,
) -> JuizResult<Arc<Mutex<dyn BrokerProxyFactory>>> {
    log::trace!("create_local_broker_factory called");
    create_messenger_broker_proxy_factory(
        "LocalBrokerProxyFactory",
        "local",
        LocalBrokerProxyCoreFactory::new(sender_receiver)?,
    )
}
