use std::{io::ErrorKind, net::SocketAddr, path::PathBuf, sync::{Arc, Mutex}};
//use axum::extract::path::ErrorKind;
use tokio::net::TcpListener;

use super::super::core_broker::CoreBrokerPtr;
use crate::{brokers::broker_ptr::BrokerPtr, prelude::*};
use crate::brokers::{broker_factory_impl::create_broker_factory_impl, BrokerFactory, CRUDBrokerHolder};
use crate::brokers::CRUDBroker;

use super::http_router::app_new;

fn into_address(host: &str, port: i64) -> String {
    return format!("{:}:{:}", host, port);
}
fn check_server_is_juiz(_host: &str, _port: i64) -> bool {
    return true;
}


async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    tokio::spawn(on_start_inner(broker_manifest, crud_broker) );
}

async fn on_start_inner(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    log::trace!("【呼出】http_broker::on_start(broker_manifest={broker_manifest})");
    let host = obj_get_str(&broker_manifest, "host").or::<&str>(Ok("0.0.0.0") ).unwrap();
    let mut port  = obj_get_i64(&broker_manifest, "port").or::<i64>( Ok(8080)).unwrap();
    //let address = format!("{:}:{:}", host, port);
    log::debug!("HTTPブローカ起動試行中(host={host}, port={port}, {broker_manifest:?}))");
    let static_filepaths: Option<Vec<(String, PathBuf)>> = match obj_get_obj(&broker_manifest, "static_filepaths") {
        Ok(v) => {
            Some(v.iter().map(|(s, val)| {
                let path: PathBuf = val.as_str().or(Some("")).unwrap().into();
                (s.clone(), path)
            }).collect::<Vec<(String, PathBuf)>>())
        }
        Err(_e) => {
            None
        }
    };

    loop {
        let address = into_address(host, port);
        log::debug!("HTTPブローカはポートバインドを試行中(host={host}, port={port}, {broker_manifest:?}))");
        match TcpListener::bind( address ).await {
            Ok(listener) => {
                log::info!("バインド成功(host={host}, port={port})");
                crud_broker.lock().unwrap().set_started();
                let new_broker_name = format!("127.0.0.1:{:}", port);
                juiz_lock(&crud_broker).unwrap().update_broker_name(new_broker_name.as_str());
                log::trace!("HTTPブローカ起動します。");
                return axum::serve(listener, app_new(crud_broker, static_filepaths).into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
            },
            Err(e) => {
                if e.kind() == ErrorKind::AddrInUse {
                    // TODO: ここで同じポートにすでにhttpがあったら、スレーブになってそこに接続する
                    // check server is juiz?
                    if !check_server_is_juiz(host, port) {
                        // ポートを占有しているサーバーはJUIZではない。
                        log::error!("on_start(broker_manifest='{broker_manifest:}') failed. Error({e:?}, {e})");
                        return ();
                    }
                    match juiz_lock(&crud_broker) {
                        Ok(mut b) => {
                            log::warn!("{host}:{port} is already occupied by JUIZ. This server is reserved as master server.");
                            let _result = b.reserve_master_broker(jvalue!({
                                "host": "127.0.0.1",
                                "port": port,
                                "broker_type": "http",
                                "broker_name": format!("127.0.0.1:{port}"),
                            })).or_else(|e| {
                                log::error!("【失敗】CRUDBroker.reserve_master_broker(). エラーは({e})。このエラーは無視されます。");
                                Err(e)
                            });
                        },
                        Err(_) => {
                            log::error!("【ロック失敗】CRUDBroker。エラー:{e:?}");
                            panic!()
                        }
                    }
                    port = port + 1;
                } else {
                    log::error!("on_start(broker_manifest='{broker_manifest:}') failed. Error({e:?}, {e})");
                    return ();
                }
            },
        }
    }// loop
//    axum::serve(TcpListener::bind( address ).await.unwrap(), app_new(crud_broker)).await.unwrap();
    
}


pub fn http_broker_factory(core_broker: CoreBrokerPtr) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    fn create_broker_function(core_broker: CoreBrokerPtr, manifest: Value) -> JuizResult<BrokerPtr> {
        Ok(BrokerPtr::new(CRUDBrokerHolder::new("HTTPBroker", "http", core_broker, &on_start, manifest.clone())?))
    }

    let manifest = jvalue!({
        "type_name": "http"
    });
    create_broker_factory_impl(core_broker, manifest, create_broker_function)
}

