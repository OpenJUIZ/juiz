


use juiz_sdk::{connection_identifier::ConnectionIdentifier, connections::{ConnectionManifest, ConnectionProfile, ConnectionType}};
use crate::prelude::*;


use core::fmt::Debug;
use std::clone::Clone;

use juiz_sdk::connections::{DestinationConnection, Connection};

#[derive(Clone, Debug)]
pub struct DestinationConnectionImpl{
    profile: ConnectionProfile,
    destination_process: ProcessPtr
}

impl DestinationConnectionImpl {

    pub fn new_from_manifest(connection_manifest: ConnectionManifest,  destination_process: ProcessPtr) -> Self {
        DestinationConnectionImpl{
            profile: connection_manifest.into(),
            destination_process}
    }
}

impl Connection for DestinationConnectionImpl {

    fn identifier(&self) -> ConnectionIdentifier {
        self.profile.clone().into() //identifier.clone()
    }

    fn connection_type(&self) -> ConnectionType {
        self.profile.connection_type.clone()
    }
    
    fn profile(&self) -> ConnectionProfile {
        self.profile.clone()        
    }
}

impl DestinationConnection for DestinationConnectionImpl {

    fn execute_destination(&self) -> JuizResult<CapsulePtr> {
        log::trace!("DestinationConnectionImpl::execute_destination() called");
        self.destination_process.lock()?.execute()
    }

    fn push(&self, value: CapsulePtr) -> JuizResult<CapsulePtr> {
        log::trace!("DestinationConnectionImpl::push() called");
        if self.connection_type() == ConnectionType::Push {
            self.destination_process.lock()?.push_by(self.profile.arg_name.as_str(), value)
        } else {
            Ok(value)
        }
    }
}
