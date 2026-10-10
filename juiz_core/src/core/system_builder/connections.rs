

use juiz_sdk::anyhow;

use crate::{connections::connection_builder::connection_builder, prelude::*};

pub(super) fn setup_connections(system: &System, manifest: &Value) -> JuizResult<()> {
    log::trace!("【呼出】system_builder::setup_connections({manifest})");
    for c in get_array(manifest)?.iter() {
        log::debug!("【setup_connections】Value({c})より接続作成");
        //let p_type_name = obj_get_str(c, "type_name")?;
        let mut mut_manifest = c.clone();
        let connection_manifest: ConnectionManifest = serde_json::from_value(c.clone()).or_else(|_e| -> anyhow::Result<ConnectionManifest> {
            log::debug!("【setup_connections】接続マニフェストが不正。source/destination書式がある可能性有り。確認開始");
            let srcv = obj_get_obj(c, "source")?;
            let dstv = obj_get_obj(c, "destination")?;
            log::debug!("【setup_connections】source/destinationを確認。");
            let obj = mut_manifest.as_object_mut().unwrap();
            let src_type_name = srcv.get("type_name").ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("Connectionへの引数にsourceがありますが、sourceにtype_nameが含まれていません。") }))?.as_str().ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("sourceに含まれるtype_nameが文字列ではありません。")}))?.to_owned();
            let dst_type_name = dstv.get("type_name").ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("Connectionへの引数にdestinationがありますが、destinationにtype_nameが含まれていません。") }))?.as_str().ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("destinationに含まれるtype_nameが文字列ではありません。")}))?.to_owned();
            let src_name = srcv.get("name").ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("Connectionへの引数にsourceがありますが、sourceにnameが含まれていません。") }))?.as_str().ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("sourceに含まれるnameが文字列ではありません。")}))?.to_owned();
            let dst_name = dstv.get("name").ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("Connectionへの引数にdestinationがありますが、destinationにnameが含まれていません。") }))?.as_str().ok_or(anyhow::anyhow!(JuizError::ArgumentError { message: format!("destinationに含まれるnameが文字列ではありません。")}))?.to_owned();
            let spm = ProcessManifest::new(src_type_name.as_str())
                .name(src_name.as_str());
            let dpm = ProcessManifest::new(dst_type_name.as_str())
                .name(dst_name.as_str());
            obj.insert("source_process_id".to_owned(),serde_json::to_value(spm.identifier()?)?);
            obj.insert("destination_process_id".to_owned(),serde_json::to_value(dpm.identifier()?)?);
            log::debug!("【setup_connections】Value({mut_manifest})から接続マニフェストへの変換試行中");
            serde_json::from_value(mut_manifest).or_else(|e| {
                log::error!("【setup_connections】Valueから接続マニフェストへの変換失敗。エラー ({e:})");
                Err(anyhow::anyhow!(e))
            })
        })?;
        log::debug!("ConnectionManifest ({connection_manifest})を取得。");
        match connection_builder::create_connection(system, &connection_manifest) {
            Ok(c) => {
                log::info!("【setup_connections】【作成】接続");
                Ok(c)
            },
            Err(e) => {
                log::error!("【setup_connections】接続作成失敗");
                Err(e)
            }
        }?;
    } 
    Ok(())
}
