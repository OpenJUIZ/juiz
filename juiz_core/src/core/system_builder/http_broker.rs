use juiz_sdk::anyhow::Context;

use crate::{brokers::{broker_factories_wrapper::BrokerFactoriesWrapper, http::{http_broker_factory, http_broker_proxy_factory}}, prelude::*};



pub fn setup_http_broker_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_http_broker_factory() called");
    let hbf = http_broker_factory(system.core_broker().clone())?;
    let hbpf = http_broker_proxy_factory()?;
    let _wrapper = system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, hbf, hbpf)?)?;
    Ok(())
}



fn get_http_staticfilepaths(options: Option<&Value>) -> Option<&Value> {
    match options {
        Some(opt) => {
            match obj_get_obj(opt, "http_broker") {
                Ok(http_opt) => {
                    match http_opt.get("static_filepaths") {
                        Some(v) => { 
                            Some(v)
                        },
                        None => None
                    }
                }
                Err(_) => None
            }
        }
        None => None
    }
   
   
}

pub(super) fn setup_http_broker(system: &mut System, port_number: i64, options: Option<&Value>) -> JuizResult<()> {
    log::trace!("system_builder::setup_http_broker() called");
    let manifest = match get_http_staticfilepaths(options) {
        None => {
            jvalue!({
                "type_name": "http",
                "name": format!("0.0.0.0:{}", port_number),
                "host": "0.0.0.0",
                "port": port_number,
            })
        }
        Some(v) => {
            jvalue!({
                "type_name": "http",
                "name": format!("0.0.0.0:{}", port_number),
                "host": "0.0.0.0",
                "port": port_number,
                "static_filepaths": v
            })
        }
    };
    
    let _http_broker = system.create_broker(&manifest).context("system.create_broker() failed in system_builder::setup_http_broker()")?;
    //system.register_broker(http_broker)?;
    log::info!("【作成】ブローカ(HTTP) setup_http_brokerよりHTTPBrokerを作成しました。");
    Ok(())
}