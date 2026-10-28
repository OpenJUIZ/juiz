use juiz_sdk::anyhow::Context;

use crate::{
    brokers::{
        broker_factories_wrapper::BrokerFactoriesWrapper,
        http::{http_broker_factory, http_broker_proxy_factory},
    },
    prelude::*,
};

pub fn setup_http_broker_factory(system: &mut System) -> JuizResult<()> {
    log::trace!("system_builder::setup_http_broker_factory() called");
    let hbf = http_broker_factory(system.core_broker().clone())?;
    let hbpf = http_broker_proxy_factory()?;
    let _wrapper =
        system.register_broker_factories_wrapper(BrokerFactoriesWrapper::new(None, hbf, hbpf)?)?;
    Ok(())
}

/// option.http_broker のうち、HTTP ブローカーにそのまま渡す項目
/// (static_filepaths: 静的ファイルの配信、cors_allow_origins: CORS の許可オリジン (issue #112)、
/// webui: false で埋め込みの管理画面を /ui で配信しない (issue #117)、
/// max_body_bytes: 本文の上限、既定 256 MiB (issue #233))
const HTTP_BROKER_PASS_THROUGH_OPTIONS: [&str; 4] = [
    "static_filepaths",
    "cors_allow_origins",
    "webui",
    "max_body_bytes",
];

pub(super) fn setup_http_broker(
    system: &mut System,
    port_number: i64,
    options: Option<&Value>,
) -> JuizResult<()> {
    log::trace!("system_builder::setup_http_broker() called");
    let mut manifest = jvalue!({
        "type_name": "http",
        "name": format!("0.0.0.0:{}", port_number),
        "host": "0.0.0.0",
        "port": port_number,
    });
    if let Some(http_opt) = options.and_then(|opt| obj_get_obj(opt, "http_broker").ok()) {
        let map = get_hashmap_mut(&mut manifest)?;
        for key in HTTP_BROKER_PASS_THROUGH_OPTIONS {
            if let Some(v) = http_opt.get(key) {
                map.insert(key.to_owned(), v.clone());
            }
        }
    }

    let _http_broker = system
        .create_broker(&manifest)
        .context("system.create_broker() failed in system_builder::setup_http_broker()")?;
    //system.register_broker(http_broker)?;
    log::info!("【作成】ブローカ(HTTP) setup_http_brokerよりHTTPBrokerを作成しました。");
    Ok(())
}
