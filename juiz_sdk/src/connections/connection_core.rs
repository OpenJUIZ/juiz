use crate::prelude::*;
use super::connection_manifest::ConnectionManifest;


#[derive(Clone)]
pub struct ConnectionCore {
    // core: ObjectCore, 
    //manifest: Value,
    //identifier: Identifier,
    manifest: ConnectionManifest,
    // connection_type: ConnectionType,
    //source_process_identifier: Identifier, 
    //destination_process_identifier: Identifier,
    // arg_name: String,
}
// fn manifest_to_connection_id(manifest: &ConnectionManifest) -> Identifier {
//     if manifest.identifier.is_some() {
//         manifest.identifier.as_ref().unwrap().clone()
//     } else {
//         connection_identifier_new(&manifest.source_process_id, &manifest.destination_process_id, manifest.arg_name.as_str())
//     }
// }


// impl Clone for ConnectionCore {
//     fn clone(&self) -> Self {
//         Self { 
//             //core: self.core.clone(),
//             identifier: self.identifier,
//             manifest: self.manifest.clone(), 
//              // connection_type: self.connection_type.clone(), 
//              // source_process_identifier: self.source_process_identifier.clone(), 
//              // destination_process_identifier: self.destination_process_identifier.clone(), 
//              /// arg_name: self.arg_name.clone() 
//         }
//     }
// }

impl ConnectionCore { 

    pub fn new(connection_impl_class_name: &'static str, 
            //source_process_identifier: Identifier, 
            //destination_process_identifier: Identifier, 
            //arg_name: String, 
            //connection_manifest: &Value
            connection_manifest: ConnectionManifest
        ) -> Self {
        //log::trace!("ConnectionCore::new() called");
        //let manif = check_connection_manifest(connection_manifest.clone())?;
        //let connection_type: ConnectionType = obj_get_str(&manif, "type")?.into();
        //let connection_id = manifest_to_connection_id(&connection_manifest);
        // 
        log::trace!("ConnectionCore::new(manif={:})", connection_manifest);
        Self {
            //identifier: connection_id.clone(),
            //core: ObjectCore::new(connection_id.clone(), JuizObjectClass::Connection(connection_impl_class_name), "Connection", connection_id.as_str(), "core", "core"),
            // source_process_identifier,
            // destination_process_identifier,
            manifest: connection_manifest, //manif,
            //arg_name,
            //connection_type
        }
    }

    // pub fn object_core(&self) -> &ObjectCore {
    //     &self.core
    // }

    pub fn destination_identifier(&self) -> &Identifier {
        // &self.destination_process_identifier
        &self.manifest.destination_process_id
    }

    pub fn source_identifier(&self) -> &Identifier {
        //&self.source_process_identifier
        &self.manifest.source_process_id
    }

    pub fn arg_name(&self) -> &String {
        //&self.arg_name
        &self.manifest.arg_name
    }

    pub fn connection_type(&self) -> ConnectionType {
        //self.connection_type.clone()
        self.manifest.connection_type.clone()
    }   

    pub fn profile_full(&self) -> JuizResult<Value> {
        Ok(jvalue!({
            "connection_type": self.connection_type().to_string(),
            "arg_name": self.arg_name().to_owned(),
            "destination_identifier": self.destination_identifier().to_owned(),
            "source_process_identifier": self.source_identifier().to_owned(),
        }).into())
    }
}


