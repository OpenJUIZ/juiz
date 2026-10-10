


use std::path::PathBuf;

use crate::{core::system_builder::{containers::{register_container_factory, register_container_process_factory}, processes::register_process_factory}, plugin::JuizObjectPlugin, prelude::*};




pub(super) fn setup_components(system: &System, manifest: &Value, option: &Value) -> JuizResult<()> {
    log::trace!("【呼出】system_builder::setup_component_factories({manifest:?})");
    for (name, v) in get_hashmap(manifest)?.iter() {
        log::debug!("【ロード】コンポーネント (name={name:})");
        setup_component(system, name, v, option)?;
        log::debug!("【ロード完了】コンポーネント (name={name:})")
    }
    Ok(())
}


fn setup_component(system: &System, name: &String, v: &Value, option: &Value) -> JuizResult<()> {
    let manifest_entry_point = "component_manifest";
    
    log::trace!("setup_component(name={:}, value={:}) called", name, v);
    let language = obj_get_str(v, "language").or::<JuizResult<&str>>(Ok("rust")).unwrap();
    let plugin = JuizObjectPlugin::new(language, name, v, system.get_working_dir(), manifest_entry_point, option)?;
    let working_dir = system.get_working_dir();
    register_component(system.core_broker().lock_mut()?.worker_mut(), working_dir, plugin)?;
    // let component_manifest = plugin.load_component_manifest(system.get_working_dir())?;
    // for container_profile in component_manifest.containers.iter() {
    //     log::debug!(" - ContainerFactory ({container_profile:?}) Loading...");
    //     register_container_factory(system.core_broker().lock_mut()?.worker_mut(), system.get_working_dir(), plugin.clone(), container_profile.factory.as_str())?;
    //     log::info!(" - ContainerFactory ({container_profile:?}) Loaded");
    //     for container_process_profile in container_profile.processes.iter() {
    //         log::debug!(" - ContainerProcessFactory ({container_process_profile:?}) Loading...");
    //         register_container_process_factory(system.core_broker().lock_mut()?.worker_mut(), system.get_working_dir(), plugin.clone(), container_process_profile.factory.as_str())?;
    //         log::debug!(" - ContainerProcessFactory ({container_process_profile:?}) Loaded");
            
    //     }
    // }
    // for process_manifest in component_manifest.processes.iter() {
    //     log::debug!(" - ProcessFactory ({process_manifest:?}) Loading...");
    //     let working_dir = system.get_working_dir();
    //     register_process_factory(system.core_broker().lock_mut()?.worker_mut(), working_dir, plugin.clone(), process_manifest.factory.as_str())?;
    //     log::info!(" - ProcessFactory ({process_manifest:?}) Loaded"); 
    // }
    Ok(())
}


pub(crate) fn register_component(core_worker: &mut CoreWorker, working_dir: Option<PathBuf>, plugin: JuizObjectPlugin) -> JuizResult<ComponentManifest> {
    //let language = obj_get_str(v, "language").or::<JuizResult<&str>>(Ok("rust")).unwrap();
    //let plugin = JuizObjectPlugin::new(language, name, v, manifest_entry_point, option)?;
    let component_manifest = plugin.load_component_manifest(working_dir.clone())?;
    for container_profile in component_manifest.containers.iter() {
        log::debug!(" - ContainerFactory ({container_profile:?}) Loading...");
        register_container_factory(core_worker, working_dir.clone(), plugin.clone(), container_profile.factory.as_str(), Some(container_profile.type_name.as_str()))?;
        log::debug!("【ロード】コンテナファクトリ ({})", container_profile.type_name);
        for container_process_profile in container_profile.processes.iter() {
            log::debug!(" - ContainerProcessFactory ({container_process_profile:?}) Loading...");
            register_container_process_factory(core_worker, working_dir.clone(), plugin.clone(), container_process_profile.factory.as_str(), Some(container_process_profile.type_name.as_str()))?;
            log::debug!(" - ContainerProcessFactory ({container_process_profile:?}) Loaded");
            
        }
    }
    for process_manifest in component_manifest.processes.iter() {
        log::debug!(" - ProcessFactory ({process_manifest:?}) Loading...");
        register_process_factory(core_worker, working_dir.clone(), plugin.clone(), process_manifest.factory.as_str(), Some(process_manifest.type_name.as_str()))?;
        log::info!(" - ProcessFactory ({process_manifest:?}) Loaded"); 
    }
    Ok(component_manifest)
}
