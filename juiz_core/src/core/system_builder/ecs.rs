use std::sync::{Arc, Mutex};
use juiz_sdk::{anyhow::Context, process_identifier::ProcessIdentifier};

use crate::{ecs::{execution_context_function::ExecutionContextFunction, execution_context_holder_factory::ExecutionContextHolderFactory, ExecutionContextFactory}, plugin::{concat_dirname, plugin_name_to_file_name, RustPlugin}, prelude::*};

pub(super) fn setup_execution_context_factories(system: &System, manifest: &Value) -> JuizResult<()> {
    log::trace!("system_builder::setup_execution_context_factories() called");
    for (name, value) in get_hashmap(manifest)?.iter() {
        log::debug!("ExecutionContextFactory (name={name:}, value='{value:}') Loading...");
        let working_dir = None;
        let plugin_filename = concat_dirname(value, plugin_name_to_file_name(name), working_dir)?;

        log::debug!(" - filename: {plugin_filename:?}");
        let cpf;
        unsafe {
            type ECFactorySymbolType<'a> = libloading::Symbol<'a, unsafe extern "Rust" fn() -> JuizResult<Arc<Mutex<dyn ExecutionContextFactory>>>>;
            let plugin: RustPlugin = RustPlugin::load(plugin_filename)?;
            {
                let symbol = plugin.load_symbol::<ECFactorySymbolType>(b"execution_context_factory")?;
                cpf = (symbol)().with_context(||format!("calling symbol 'execution_context_factory'. arg is {manifest:}"))?;
                let _ccpf = juiz_lock(&cpf)?;
            }
            system.core_broker().lock_mut()?.worker_mut().store_mut().ecs.register_factory(ExecutionContextHolderFactory::new(Some(plugin), cpf)?)?;
        }
        log::info!("ExecutionContextFactory (name={name:}) Loaded");
    }
    Ok(())
}

pub(super) fn setup_main_loop_ec_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("【呼出】setup_main_loop_ec()");
    let ecf = crate::ecs::main_loop_ec::execution_context_factory()?;
    system.core_broker().lock_mut()?.worker_mut().store_mut().ecs.register_factory(ExecutionContextHolderFactory::new(None, ecf)?)?;
    Ok(())
}

pub(super) fn setup_ecs(system: &mut System, manifest: &Value) -> JuizResult<()> {
    log::trace!("system_builder::setup_ecs({manifest}) called");
    for p in get_array(manifest)?.iter() {
        let name = obj_get_str(p, "name")?;
        let type_name = obj_get_str(p, "type_name")?;
        log::debug!("ExecutionContext ({:}:{:}) Creating...", name, type_name);
        let ec = system.core_broker().lock_mut()?.worker_mut().create_ec_ref(p.clone())?;
        log::info!("ExecutionContext ({:}:{:}) Created", name, type_name);
        juiz_lock(&ec)?.on_load(system);
        match obj_get(p, "bind") {
            Err(_) => {},
            Ok(binds_obj) => {
                log::trace!("start bind setup for {binds_obj}");
                for b in get_array(binds_obj)?.iter() {
                    setup_ec_bind(system, Arc::clone(&ec), b)?;
                }
            }
        };
        // match obj_get(p, "auto_start") {
        //     Err(_) => {},
        //     Ok(auto_start_obj) => {
        //         log::trace!("start ec setup for {auto_start_obj}");
        //         match auto_start_obj.as_bool() {
        //             Some(flag) => {
        //                 if flag {
        //                     juiz_lock(&ec)?.start()?;
        //                 }
        //             }
        //             None => {},
        //         }
        //     }
        // };
    } 
    Ok(())
}

pub(super) fn cleanup_ecs(system: &System) -> JuizResult<()> {
    log::trace!("system_builder::cleanup_ecs() called");
    system.core_broker().lock_mut()?.worker_mut().cleanup_ecs()
}

fn setup_ec_bind(system: &System, ec: Arc<Mutex<dyn ExecutionContextFunction>>, bind_info: &Value) -> JuizResult<()> {
    let ec_id = juiz_lock(&ec)?.identifier().clone();
    log::trace!("system_builder::setup_ec_bind(ec={:}) called", ec_id);
    let target_process = system.core_broker().lock_mut()?.worker_mut().any_process_from_identifier(&ProcessIdentifier::from_value(bind_info)?, false)?;
    let proc_id = target_process.identifier().clone();
    log::trace!("EC({:}) -> Process({:})", ec_id, proc_id);
    let ret = juiz_lock(&ec)?.bind(target_process);
    log::info!("EC({:}) -> Process({:}) Bound", ec_id, proc_id);
    ret
}
