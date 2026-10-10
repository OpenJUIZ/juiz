

pub mod connection_builder {

    use crate::prelude::*;
    
    use juiz_sdk::{anyhow::{anyhow, Context}, connections::{ConnectionManifest, ConnectionProfile}};

    ///
    pub fn create_connection(system: &System, manifest: &ConnectionManifest) -> JuizResult<ConnectionProfile> {
        
        log::trace!("connection_builder::create_connection(manifest={:?}) called", manifest);
        let src = system.core_broker().lock()?.worker().any_process_from_identifier(&manifest.source_process_id, true)?;
        let dst = system.core_broker().lock()?.worker().any_process_from_identifier(&manifest.destination_process_id, true)?;
        //let arg_name =  manifest.arg_name.clone();
        connect(
            src, dst, 
            manifest
        ).context("connection_builder::connect()")
    }
    
    pub fn connect(src: ProcessPtr, dst: ProcessPtr, connection_manifest: &ConnectionManifest) -> JuizResult<ConnectionProfile> {
        log::trace!("connection_builder::connect({connection_manifest})が呼ばれました。");
        let mut manif_for_source = connection_manifest.clone();
        manif_for_source.source_process_id.broker_type_name = "core".to_owned();
        manif_for_source.source_process_id.broker_name = "core".to_owned();
        log::debug!("【connect】出力側接点宣言変更。ブローカを'core'に変更。");
        let _src_manifest = match src.lock_mut()?.try_connect_to(dst.clone(), &manif_for_source) {
            Ok(manif) => {
                log::debug!("【connect】出力側接点接続完了");
                Ok(manif)
            }
            Err(e) => {
                log::error!("【connect】Process(src).try_connect_to() failed. Error({e})");
                Err(anyhow!(e))
            }
        }?;

        let mut manif_for_dest = connection_manifest.clone();
        manif_for_dest.destination_process_id.broker_type_name = "core".to_owned();
        manif_for_dest.destination_process_id.broker_name = "core".to_owned();
        manif_for_dest.source_process_id = connection_manifest.source_process_id.clone();
        log::debug!("【connect】入力側接点宣言変更。ブローカを'core'に変更。");
        match dst.lock_mut()?.notify_connected_from(src, &manif_for_dest) {
            Ok(result) => {
                log::trace!("destination_connection, connected!");
                Ok(result.into())
            }
            Err(e) => {
                log::error!("Process(dist).notify_connected_from() failed. Error({e})");
                Err(anyhow!(e))
            }
        }
    }

}