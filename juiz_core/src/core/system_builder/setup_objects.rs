
use juiz_sdk::anyhow::Context;

use crate::{core::system_builder::subsystems::{setup_mastersystem, setup_subsystems}, prelude::*};
use crate::core::system_builder::{brokers::{setup_broker_proxies, setup_brokers}, connections::setup_connections, containers::setup_containers, ecs::setup_ecs, http_broker::{setup_http_broker, setup_http_broker_factory}, local_broker::{setup_local_broker, setup_local_broker_factory}, processes::setup_processes};

pub(crate) fn setup_objects(system: &mut System, manifest: &Value) -> JuizResult<()> {
    log::trace!("【呼出】System::setup_objects()");
    let manifest_copied = manifest.clone();

    let _ = when_contains_do(manifest, "processes", |v| {
        setup_processes(system, v)
            .with_context(||{format!("setup_processes({}) in setup_objects() in setup_objects.rs", manifest.get("processes").unwrap())})
    })?;

    let _ = when_contains_do(manifest, "containers", |v| {
        setup_containers(system, v)
            .with_context(||{format!("setup_containers({}) in setup_objects() in setup_objects.rs", manifest.get("containers").unwrap())})
    })?;

    setup_http_broker_factory(system).context("system_builder::setup_http_broker_factory in System::setup() failed.")?;
    
    {
        let options = get_options(manifest);
        if is_http_broker_start(options) {
            let port_number: i64 = get_http_port(options);
            //let opt = options.unwrap().clone();
            log::trace!("http_broker is being created. option is {options:?}");
            setup_http_broker(system, port_number, options).context("system_builder::setup_http_broker in System::setup() failed.")?;
        } else {
            log::trace!("http_broker was not created. option is {options:?}");
        }
    }

    setup_local_broker_factory(system).context("system_builder::setup_local_broker_factory in System::setup() failed.")?;
    setup_local_broker(system).context("system_builder::setup_local_broker in System::setup() failed.")?;

    // system_builder::setup_ipc_broker_factory(self).context("system_builder::setup_ipc_broker_factory in System::setup() failed.")?;
    //system_builder::setup_ipc_broker(self).context("system_builder::setup_ipc_broker in System::setup() failed.")?;
    
    let _ = when_contains_do_mut(&manifest_copied, "brokers", |v| {
        setup_brokers(system, v).context("system_builder::setup_brokers in System::setup() failed.")
    })?;

    system.start_brokers()?;

    // BrokerProxyをセットアップする
    let _ = when_contains_do_mut(&manifest_copied, "broker_proxies", |v| {
        setup_broker_proxies(system, v).context("system_builder::setup_broker_proxies in System::setup() failed.")
    })?;

    // ここでbrokerがスタートした時にマニフェストが更新されている可能性があるので最新版を取得
    // system.wait_brokers_started()?;
    let manifest_updated = system.core_broker().lock()?.worker().manifest();
    log::trace!("manifest_updated: {manifest_updated:?}");

    let _ =  when_contains_do(&manifest_updated, "subsystems", |v| {
        setup_subsystems(system, v).context("system_builder::setup_subsystems in System::setup() failed.")
    })?;

    let _ =  when_contains_do(&manifest_updated, "mastersystem", |v| {
        setup_mastersystem(system, v).context("system_builder::setup_mastersystem in System::setup() failed.")
    })?;

    let _ = when_contains_do(&manifest_copied, "ecs", |v| {
        setup_ecs(system, v).context("system_builder::setup_ecs in System::setup() failed")
    })?;

    let _ =  when_contains_do(&manifest, "connections", |v| {
        setup_connections(system, v).context("コネクションの作成(system_builder::setup_connections)が失敗しました。")
    })?;
    log::debug!("System::setup() successfully finished.");
    Ok(())
}

fn get_options(manifest: &Value) -> Option<&Value> {
    match manifest.as_object() {
        Some(obj_manif) => {
            match obj_manif.get("option") {
                Some(v) => Some(v),
                None => None
            }
        },
        None => None
    }
}

fn get_http_option(option: Option<&Value>) -> Option<&Value> {
    if option.is_none() {
        None
    } else {
        match option.unwrap().as_object() {
            Some(opt_obj) => {
                match opt_obj.get("http_broker") {
                    Some(v) => Some(v),
                    None => None
                }
            },
            None => {
                log::error!("Manifest file is invalid. option value must be object type.");
                None
            }
        }
    }
}

fn is_http_broker_start(option: Option<&Value>) -> bool {
    match get_http_option(option) {
        Some(http_broker_opt) => {
            match obj_get_bool(http_broker_opt, "start") {
                Ok(v) => return v,
                Err(_e) => return true,
            }
        }
        None => return true,
    }
}

fn get_http_port(option: Option<&Value>) -> i64 {
    match get_http_option(option) {
        Some(http_broker_opt) => {
            match obj_get_i64(http_broker_opt, "port") {
                Ok(v) => return v,
                Err(_e) => return 8000,
            }
        }
        None => return 8000,
    }
}