



use crate::prelude::*;

use core::fmt::Debug;
use std::clone::Clone;

use juiz_sdk::{connection_identifier::ConnectionIdentifier, connections::{Connection, ConnectionManifest, ConnectionProfile}};

#[derive(Debug, Clone)]
pub struct SourceConnectionImpl {
    profile: ConnectionProfile,
    source_process: ProcessPtr,
}

impl SourceConnectionImpl {

    pub fn new_from_manifest(connection_manifest: ConnectionManifest, source_process: ProcessPtr) -> Self {
        log::trace!("SourceConnectionImpl::new_from_manifest({connection_manifest}) called");
        SourceConnectionImpl{
            profile: connection_manifest.into(),
            source_process}
    }
    
}

impl Connection for SourceConnectionImpl {

    fn identifier(&self) -> ConnectionIdentifier {
        self.profile.clone().into() 
    }

    fn connection_type(&self) -> ConnectionType {
        self.profile.connection_type.clone()
    }
    
    fn profile(&self) -> ConnectionProfile {
        self.profile.clone()
    }
}

impl SourceConnection for SourceConnectionImpl {

    fn is_source_updated(&self) -> JuizResult<bool> {
        self.source_process.lock()?.is_updated()
    }

    fn invoke_source(&mut self) -> JuizResult<CapsulePtr> {
        self.source_process.lock()?.invoke()
    }
 
    fn pull(&self) -> JuizResult<CapsulePtr> {
        log::trace!("SourceConnectionImpl({}).pull() called", self.identifier());
        self.source_process.lock()?.invoke()
    }
}
