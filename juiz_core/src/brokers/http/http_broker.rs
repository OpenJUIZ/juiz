use std::{
    io::ErrorKind,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
//use axum::extract::path::ErrorKind;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use super::super::core_broker::CoreBrokerPtr;
use crate::brokers::CRUDBroker;
use crate::brokers::{
    broker_factory_impl::create_broker_factory_impl, BrokerFactory, CRUDBrokerHolder,
};
use crate::{brokers::broker_ptr::BrokerPtr, prelude::*};

use super::http_router::app_new;

fn into_address(host: &str, port: i64) -> String {
    return format!("{:}:{:}", host, port);
}
fn check_server_is_juiz(_host: &str, _port: i64) -> bool {
    return true;
}

/// 埋め込んだ管理画面を /ui で配信するか (issue #117)。
///
/// feature "webui" が無いとき、option.http_broker.webui が false のとき、
/// static_filepaths で /ui に別のディレクトリを割り当てているときは配信しない。
#[allow(unused_variables)]
fn serve_webui(broker_manifest: &Value, static_filepaths: &Option<Vec<(String, PathBuf)>>) -> bool {
    #[cfg(feature = "webui")]
    {
        use super::webui::WEBUI_PATH;
        if broker_manifest.get("webui").and_then(|v| v.as_bool()) == Some(false) {
            return false;
        }
        let overridden = static_filepaths
            .iter()
            .flatten()
            .any(|(url, _)| url.trim_end_matches('/') == WEBUI_PATH);
        if overridden {
            log::info!("static_filepaths で {WEBUI_PATH} が指定されているので、埋め込みの管理画面は配信しません");
        }
        !overridden
    }
    #[cfg(not(feature = "webui"))]
    false
}

/// 管理画面の URL を起動時に表示する (issue #117)
#[allow(unused_variables)]
fn announce_webui(host: &str, port: i64) {
    #[cfg(feature = "webui")]
    {
        use super::webui::{webui_embedded, WEBUI_PATH};
        // すべてのアドレスで待ち受けるときは、同じ機械から開く URL を出す
        let host = match host {
            "0.0.0.0" | "::" | "[::]" | "" => "localhost",
            h => h,
        };
        let url = format!("http://{host}:{port}{WEBUI_PATH}/");
        if webui_embedded() {
            println!("juiz 管理画面: {url}");
            log::info!("管理画面を {url} で配信します");
        } else {
            log::info!("管理画面は同梱されていません ({url} はビルド方法の案内になります)");
        }
    }
}

/// option.http_broker.cors_allow_origins から CORS の設定を作る (issue #112)。
///
/// 管理画面を juiz とは別のオリジン (開発サーバーなど) から開くときに使う。既定 (指定なし) では CORS ヘッダを付けない。
/// 値はオリジンの配列か文字列。"*" ならすべてのオリジンを許可する。
/// ```yaml
/// option:
///   http_broker:
///     cors_allow_origins:
///       - "http://localhost:5173"
/// ```
fn cors_layer(broker_manifest: &Value) -> Option<CorsLayer> {
    let origins: Vec<String> = match broker_manifest.get("cors_allow_origins")? {
        Value::String(s) => vec![s.clone()],
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_owned()))
            .collect(),
        v => {
            log::error!("cors_allow_origins はオリジンの文字列か配列で指定します ({v})。CORS は無効のままです。");
            return None;
        }
    };
    let allow_origin = if origins.iter().any(|o| o == "*") {
        AllowOrigin::any()
    } else {
        let values = origins
            .iter()
            .filter_map(|o| match o.parse() {
                Ok(v) => Some(v),
                Err(e) => {
                    log::error!(
                        "cors_allow_origins のオリジン({o})を読めません ({e})。無視します。"
                    );
                    None
                }
            })
            .collect::<Vec<axum::http::HeaderValue>>();
        if values.is_empty() {
            return None;
        }
        AllowOrigin::list(values)
    };
    log::info!("HTTPブローカは CORS を許可します (origins={origins:?})");
    Some(
        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods(Any)
            .allow_headers(Any),
    )
}

async fn on_start(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    tokio::spawn(on_start_inner(broker_manifest, crud_broker));
}

async fn on_start_inner(broker_manifest: Value, crud_broker: Arc<Mutex<CRUDBroker>>) -> () {
    log::trace!("【呼出】http_broker::on_start(broker_manifest={broker_manifest})");
    let host = obj_get_str(&broker_manifest, "host")
        .or::<&str>(Ok("0.0.0.0"))
        .unwrap();
    let mut port = obj_get_i64(&broker_manifest, "port")
        .or::<i64>(Ok(8080))
        .unwrap();
    //let address = format!("{:}:{:}", host, port);
    log::debug!("HTTPブローカ起動試行中(host={host}, port={port}, {broker_manifest:?}))");
    let static_filepaths: Option<Vec<(String, PathBuf)>> =
        match obj_get_obj(&broker_manifest, "static_filepaths") {
            Ok(v) => Some(
                v.iter()
                    .map(|(s, val)| {
                        let path: PathBuf = val.as_str().or(Some("")).unwrap().into();
                        (s.clone(), path)
                    })
                    .collect::<Vec<(String, PathBuf)>>(),
            ),
            Err(_e) => None,
        };

    let serve_webui = serve_webui(&broker_manifest, &static_filepaths);

    loop {
        let address = into_address(host, port);
        log::debug!(
            "HTTPブローカはポートバインドを試行中(host={host}, port={port}, {broker_manifest:?}))"
        );
        match TcpListener::bind(address).await {
            Ok(listener) => {
                log::info!("バインド成功(host={host}, port={port})");
                // 名前の更新を済ませてから起動済みにする。wait_until_started() の直後に
                // profile() が読まれるので、順序が逆だと古い名前が登録されることがある (issue #55)
                {
                    let new_broker_name = format!("127.0.0.1:{:}", port);
                    let mut b = juiz_lock(&crud_broker).unwrap();
                    let _ = b.update_broker_name(new_broker_name.as_str());
                    b.set_started();
                }
                log::trace!("HTTPブローカ起動します。");
                if serve_webui {
                    announce_webui(host, port);
                }
                let mut app = app_new(crud_broker, static_filepaths, serve_webui);
                if let Some(cors) = cors_layer(&broker_manifest) {
                    app = app.layer(cors);
                }
                return axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                .unwrap();
            }
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
                        }
                        Err(_) => {
                            log::error!("【ロック失敗】CRUDBroker。エラー:{e:?}");
                            panic!()
                        }
                    }
                    port = port + 1;
                } else {
                    log::error!(
                        "on_start(broker_manifest='{broker_manifest:}') failed. Error({e:?}, {e})"
                    );
                    return ();
                }
            }
        }
    } // loop
      //    axum::serve(TcpListener::bind( address ).await.unwrap(), app_new(crud_broker)).await.unwrap();
}

pub fn http_broker_factory(
    core_broker: CoreBrokerPtr,
) -> JuizResult<Arc<Mutex<dyn BrokerFactory>>> {
    fn create_broker_function(
        core_broker: CoreBrokerPtr,
        manifest: Value,
    ) -> JuizResult<BrokerPtr> {
        Ok(BrokerPtr::new(CRUDBrokerHolder::new(
            "HTTPBroker",
            "http",
            core_broker,
            &on_start,
            manifest.clone(),
        )?))
    }

    let manifest = jvalue!({
        "type_name": "http"
    });
    create_broker_factory_impl(core_broker, manifest, create_broker_function)
}

#[cfg(test)]
mod tests {
    use super::cors_layer;
    use crate::prelude::*;

    #[test]
    fn cors_is_off_by_default() {
        assert!(cors_layer(&jvalue!({"port": 8000})).is_none());
    }

    #[test]
    fn cors_accepts_string_or_array() {
        assert!(cors_layer(&jvalue!({"cors_allow_origins": "*"})).is_some());
        assert!(cors_layer(&jvalue!({"cors_allow_origins": ["http://localhost:5173"]})).is_some());
        assert!(cors_layer(&jvalue!({"cors_allow_origins": []})).is_none());
        assert!(cors_layer(&jvalue!({"cors_allow_origins": 1})).is_none());
    }
}
