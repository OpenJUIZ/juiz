pub mod connection_builder {

    use crate::prelude::*;

    use juiz_sdk::{
        anyhow::{anyhow, Context},
        connections::{ConnectionManifest, ConnectionProfile},
    };

    ///
    pub fn create_connection(
        system: &System,
        manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile> {
        log::trace!(
            "connection_builder::create_connection(manifest={:?}) called",
            manifest
        );
        let src = system
            .core_broker()
            .lock()?
            .worker()
            .any_process_from_identifier(&manifest.source_process_id, true)?;
        let dst = system
            .core_broker()
            .lock()?
            .worker()
            .any_process_from_identifier(&manifest.destination_process_id, true)?;
        //let arg_name =  manifest.arg_name.clone();
        connect(src, dst, manifest).context("connection_builder::connect()")
    }

    pub fn connect(
        src: ProcessPtr,
        dst: ProcessPtr,
        connection_manifest: &ConnectionManifest,
    ) -> JuizResult<ConnectionProfile> {
        log::trace!("connection_builder::connect({connection_manifest})が呼ばれました。");
        // 出力型と引数型を比べ、互換でなければつながない (仕様 docs/spec/datatype.md §8、issue #193)
        check_connection_types(&src, &dst, &connection_manifest.arg_name)?;
        let mut manif_for_source = connection_manifest.clone();
        manif_for_source.source_process_id.broker_type_name = "core".to_owned();
        manif_for_source.source_process_id.broker_name = "core".to_owned();
        log::debug!("【connect】出力側接点宣言変更。ブローカを'core'に変更。");
        let _src_manifest = match src
            .lock_mut()?
            .try_connect_to(dst.clone(), &manif_for_source)
        {
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

    /// source の戻り値の型 (writer) と destination の引数の型 (reader) を比べる (issue #193)。
    ///
    /// profile を取れないとき (リモートの Process が応答しないなど) は検査せず、今までどおり接続処理に任せる。
    pub fn check_connection_types(
        src: &ProcessPtr,
        dst: &ProcessPtr,
        arg_name: &str,
    ) -> JuizResult<()> {
        let profiles = src
            .lock()
            .and_then(|s| s.profile())
            .and_then(|s| Ok((s, dst.lock()?.profile()?)));
        let (src_profile, dst_profile) = match profiles {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "【connect】型検査のための profile を取れませんでした。検査を飛ばします ({e})"
                );
                return Ok(());
            }
        };
        crate::types::check_connectable(
            src_profile.output_schema.as_ref(),
            crate::types::argument_schema(&dst_profile, arg_name),
        )
        .map(|_| ())
        .or_else(|e| {
            log::error!(
                "【connect】{} から {}::{arg_name} へは型が合わないので接続しません ({e})",
                src.identifier(),
                dst.identifier()
            );
            Err(e)
        })
    }
}
