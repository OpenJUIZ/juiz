use std::path::PathBuf;

use juiz_sdk::anyhow::{self, Context};

use crate::{
    core::system_builder::topics::{setup_publish_topic, setup_subscribe_topic},
    plugin::JuizObjectPlugin,
    prelude::*,
    processes::ProcessFactoryWrapper,
};

pub(super) fn setup_process_factories(
    system: &System,
    manifest: &Value,
    option: &Value,
) -> JuizResult<()> {
    log::trace!("【呼出】setup_process_factories({manifest:})");
    for (name, v) in get_hashmap(manifest)?.iter() {
        log::debug!("ロード中：ProcessFactory (name={:})", name);
        setup_process_factory(system, name, v, option)
            .with_context(|| format!("setup_process_factory(name='{name:}')"))?;
        log::info!("ロード完了：ProcessFactory (name={:})", name);
    }
    log::trace!("【終了】setup_process_factories()");
    Ok(())
}

/// ProcessFactoryをセットアップする。
/// name: ProcessFactoryの型名
/// v: manifest。languageタグがあれば、rust, pythonから分岐する。
fn setup_process_factory(
    system: &System,
    name: &String,
    v: &Value,
    option: &Value,
) -> JuizResult<ProcessFactoryPtr> {
    log::trace!("【呼出】setup_process_factory({name:}, {v:})");
    let manifest_entry_point = "manifest";
    let result = match v.as_object() {
        None => {
            log::error!("【失敗】引数のValueがobject型ではない。");
            Err(anyhow::Error::from(JuizError::InvalidSettingError {
                message:
                    "loading process_factories failed. Value is not object type. Invalid config."
                        .to_owned(),
            }))
        }
        Some(obj) => {
            let language = obj
                .get("language")
                .and_then(|v| v.as_str())
                .or(Some("rust"))
                .unwrap();
            let working_dir = system.get_working_dir();
            register_process_factory(
                &mut system.core_broker().lock_mut()?.worker_mut(),
                working_dir.clone(),
                JuizObjectPlugin::new(
                    language,
                    name,
                    v,
                    working_dir,
                    manifest_entry_point,
                    option,
                )?,
                "process_factory",
                None,
            )
        }
    };
    log::trace!("【終了】setup_process_factory()");
    result
}

pub(super) fn setup_processes(system: &System, manifest: &Value) -> JuizResult<()> {
    log::trace!("【呼出】setup_processes({manifest})");
    for process_manifest_value in get_array(manifest)?.iter() {
        // Topic 名は Process のネームスペースで解決しておく (issue #149)
        let process_manifest =
            ProcessManifest::try_from(process_manifest_value.clone())?.resolve_topic_names()?;
        log::debug!("【setup_processes】作成中 Process ({:?})", process_manifest);
        let new_process = system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .create_process_ref(&process_manifest)?;
        log::info!("【作成】Process ({:?})", process_manifest);
        p_apply_from_manifest(&new_process, process_manifest_value)?;

        // Topicをpublishするなら
        for pub_topic in process_manifest.publishes.iter() {
            setup_publish_topic(system, new_process.clone(), pub_topic.clone())?
        }
        for (arg_name, sub_topic) in process_manifest.subscribes.iter() {
            setup_subscribe_topic(system, new_process.clone(), arg_name, sub_topic.clone())?
        }
    }
    log::trace!("【終了】setup_processes()");
    Ok(())
}

/// 設定ファイルのプロセス定義に `p_apply` があれば、その値を引数に部分適用する (issue #61)
///
/// ```yaml
/// p_apply:
///   arg1: 10
/// ```
/// conf で与えた値は inlet の初期値として働き、後から push や p_apply が来れば上書きされる。
pub(super) fn p_apply_from_manifest(process: &ProcessPtr, manifest: &Value) -> JuizResult<()> {
    let Some(p_apply) = manifest.get("p_apply") else {
        return Ok(());
    };
    let map = p_apply.as_object().ok_or_else(|| {
        anyhow::Error::from(JuizError::InvalidSettingError {
            message: format!("p_apply must be object type. ({p_apply})"),
        })
    })?;
    for (arg_name, value) in map.iter() {
        process
            .lock_mut()?
            .p_apply(arg_name, value.clone().into())
            .with_context(|| format!("p_apply(arg_name='{arg_name}', value={value})"))?;
        log::debug!("【p_apply】{arg_name} = {value}");
    }
    Ok(())
}

pub(super) fn cleanup_processes(system: &mut System) -> JuizResult<()> {
    log::trace!("【呼出】cleanup_processes()");
    let r = system
        .core_broker()
        .lock_mut()
        .and_then(|mut cb| cb.worker_mut().store_mut().clear());
    log::trace!("【終了】cleanup_processes()");
    r
}

pub(crate) fn register_process_factory(
    core_worker: &mut CoreWorker,
    working_dir: Option<PathBuf>,
    plugin: JuizObjectPlugin,
    symbol_name: &str,
    type_name_opt: Option<&str>,
) -> JuizResult<ProcessFactoryPtr> {
    log::trace!("【呼出】register_process_factory()");
    let pf = plugin.load_process_factory(working_dir, symbol_name, type_name_opt)?;
    let type_name = pf
        .lock()
        .or_else(|e| {
            Err(JuizError::ObjectLockError {
                target: e.to_string(),
            })
        })?
        .type_name()
        .to_owned();
    let pfw = ProcessFactoryPtr::new(ProcessFactoryWrapper::new(plugin, pf)?);
    core_worker
        .store_mut()
        .processes
        .register_factory(type_name.as_str(), pfw.clone())?;
    log::info!("【作成】ProcessFactory(type_name={type_name})");
    log::trace!("【終了】register_process_factory()");
    Ok(pfw)
}
