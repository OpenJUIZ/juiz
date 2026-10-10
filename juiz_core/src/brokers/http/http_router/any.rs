

use juiz_sdk::anyhow;
use reqwest::StatusCode;
use std::{net::SocketAddr, sync::{Arc, Mutex}};
use axum::{extract::{ConnectInfo, Multipart, Path, Query, State}, http::HeaderMap, response::IntoResponse, routing, Json, Router};

use crate::{brokers::http::http_router::{multipart_to_capsule_map, FullQuery}, prelude::*};
use crate::brokers::crud_broker::CRUDBroker;

use super::{json_output_wrap, full_query_to_map};
use utoipa::OpenApi;

#[utoipa::path(
    post,
    path = "/api/{class_name}/{function_name}",
    params(
        FullQuery,
    ),
    request_body = Value,
    responses(
        (status = 200, description = "Post object parameter", body = [String])
    ),
    tag = "universal.any",
)]
pub async fn object_post_handler(
    Path((class_name, function_name)): Path<(String, String)>,
    query: Query<FullQuery>, //, path_query: Query<PathQuery>,
    headers: HeaderMap,
    remote_addr: ConnectInfo<SocketAddr>,
    State(crud_broker): State<Arc<Mutex<CRUDBroker>>>, 
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let map = full_query_to_map(&query);
    log::trace!("[POST] HTTPBroker/object_post_handler({class_name}, {function_name}, {body}, {map:?}) called");
    let v = tokio::task::spawn_blocking(move ||{
        juiz_lock(&crud_broker).unwrap().create_class(class_name.as_str(), function_name.as_str(), construct_capsule_map(CapsuleMap::new(), "CREATE", class_name.as_str(), function_name.as_str(), query, headers, remote_addr))
    }).await;
    let r = json_output_wrap(v.unwrap());
    r
}

fn body_to_capsule_map(body: Value, _headers: &HeaderMap) -> Result<CapsuleMap, anyhow::Error> {
    body.try_into()
}


fn construct_capsule_map(mut capsule_map: CapsuleMap, method_name: &str, class_name: &str, function_name: &str, query: Query<FullQuery>, headers: HeaderMap, remote_addr: ConnectInfo<SocketAddr>) -> CapsuleMap {
    capsule_map.set_param("method_name", method_name);
    capsule_map.set_param("class_name", class_name);
    capsule_map.set_param("function_name", function_name);
    match query.identifier.clone() {
        None => {},
        Some(v) => {
            capsule_map.set_param("identifier", v.as_str());
        }
    }
    match query.path.clone() {
        None => {},
        Some(v) => {
            capsule_map.set_param("path", v.as_str());
        }
    }
    match query.recursive.clone() {
        None => {},
        Some(v) => {
            capsule_map.set_param("recursive", v.as_str());
        }
    }
    match query.system_uuid.clone() {
        None => {},
        Some(v) => {
            capsule_map.set_param("system_uuid", v.as_str());
        }
    }
    match query.topic_name.clone() {
        None => {},
        Some(v) => {
            capsule_map.set_param("topic_name", v.as_str());
        }
    }
    // println!("HEADER>>>> {headers:?}");
    match headers.get("host") {
        Some(header) => {
            match header.to_str() {
                Ok(host) => {
                    let accessed_broker_id = format!("http://{}", host);
                    capsule_map.set_param("accessed_broker_id", accessed_broker_id.as_str());

                    let accessed_broker_type = "http".to_owned();
                    capsule_map.set_param("accessed_broker_type", accessed_broker_type.as_str());
                    let accessed_broker_name = host.to_owned();
                    capsule_map.set_param("accessed_broker_name", accessed_broker_name.as_str());
                }
                Err(_) => {}
            }
        },
        None => {}
    }
    let remote_addr_str = remote_addr.0.to_string().as_str().to_owned();
    capsule_map.set_param("remote_addr", remote_addr_str.as_str());

    if class_name == "system" && function_name == "add_mastersystem"  {
        let _r = match capsule_map.get("profile") {
            Ok(capsule_ptr) => {
                capsule_ptr.lock_modify_as_value(|v|{
                    match v.as_object_mut().unwrap().get_mut("subsystem").unwrap().as_object_mut() {
                        Some(obj) => {
                            let broker_name = obj.get("broker_name").unwrap().as_str().unwrap().to_owned();
                            let broker_tokens = broker_name.split(":").collect::<Vec<&str>>();
                            let port_str = broker_tokens.get(1).unwrap();
                            let remote_tokens = remote_addr_str.split(":").collect::<Vec<&str>>();
                            let addr_str = (*remote_tokens.get(0).unwrap()).to_owned();
                            
                            let new_broker_name = addr_str + ":" + port_str;
                            obj.insert("broker_name".to_owned(), jvalue!(new_broker_name));
                        }
                        None => todo!(),
                    }
                })
            }
            Err(_) => todo!(),
        };
    }

    capsule_map
}

#[utoipa::path(
    patch,
    path = "/api/{class_name}/{function_name}",
    params(
        FullQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "Get object parameter", body = [String])
    ),
    tag = "universal.any",
)]
pub async fn object_patch_handler(
    Path((class_name, function_name)): Path<(String, String)>,
    query: Query<FullQuery>,
    headers: HeaderMap,
    // multipart: Multipart,
    remote_addr: ConnectInfo<SocketAddr>,
    State(crud_broker): State<Arc<Mutex<CRUDBroker>>>, 
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let map = full_query_to_map(&query);
    log::trace!("[PATCH] ({class_name}, {function_name}, {body}, {map:?})が呼ばれました");
    let v = tokio::task::spawn_blocking(move ||{
        juiz_lock(&crud_broker).unwrap().update_class(class_name.as_str(), function_name.as_str(), construct_capsule_map(body_to_capsule_map(body, &headers)?, "UPDATE", class_name.as_str(), function_name.as_str(), query, headers, remote_addr))
    }).await;
    log::debug!("【patch】 Resultは{v:?}");
    let r = json_output_wrap(v.unwrap());
    r
}


#[utoipa::path(
    put,
    path = "/api/{class_name}/{function_name}",
    params(
        FullQuery
    ),
    request_body(content_type = "multipart/formdata", content = MultiPart),
    responses(
        (status = 200, description = "Get object parameter", body = [String])
    ),
    tag = "universal.any",
)]
pub async fn object_put_handler(
    Path((class_name, function_name)): Path<(String, String)>,
    query: Query<FullQuery>,
    headers: HeaderMap,
    //multipart: Multipart,
    remote_addr: ConnectInfo<SocketAddr>,
    State(crud_broker): State<Arc<Mutex<CRUDBroker>>>, 
    multipart: Multipart,
) -> impl IntoResponse {
    let map = full_query_to_map(&query);
    log::trace!("[PUT] ({class_name}, {function_name}, {map:?}, {multipart:?}) called");
    match multipart_to_capsule_map(multipart).await {
        Ok(capsule_map) => {
            let v = tokio::task::spawn_blocking(move ||{
                juiz_lock(&crud_broker).unwrap().update_class(class_name.as_str(), function_name.as_str(), construct_capsule_map(capsule_map, "UPDATE", class_name.as_str(), function_name.as_str(), query, headers, remote_addr))
            }).await;
            let r = json_output_wrap(v.unwrap());
            r.into_response()
        }
        Err(e) => {
            log::error!("multipart_to_capsule_map() failed. Err({e:?})");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(
                jvalue!({
                    "message": format!("Internal Server Error:  {:#}, {:}", e, e.to_string())
                }))).into_response()
        }
    }
}



#[utoipa::path(
    get,
    path = "/api/{class_name}/{function_name}",
    //context_path = "{full_path}", 
    params(
        //FullQuery
        ("query" = FullQuery, Query, deprecated = false, description = ""),
        ("Host" = String, Header, deprecated = false, description = "")
    ),
    responses(
        (status = 200, description = "Get object parameter", body = [String])
    ),
    tag = "universal.any",
)]
pub async fn object_get_handler(
    Path((class_name, function_name)): Path<(String, String)>,
    query: Query<FullQuery>,
    headers: HeaderMap,
    remote_addr: ConnectInfo<SocketAddr>,
    State(crud_broker): State<Arc<Mutex<CRUDBroker>>>, 
) -> impl IntoResponse {
    //let host = "";
    let full_path = "";
    let map = full_query_to_map(&query);
    log::trace!("[GET] ({class_name}, {function_name}, {map:?}, {full_path:?}, {headers:?}) called");
    let v = tokio::task::spawn_blocking(move ||{
        juiz_lock(&crud_broker).unwrap().read_class(class_name.as_str(), function_name.as_str(), construct_capsule_map(CapsuleMap::new(), "READ", class_name.as_str(), function_name.as_str(), query, headers, remote_addr))
    }).await;
    let r = json_output_wrap(v.unwrap());
    r
}

#[utoipa::path(
    delete,
    path = "/api/{class_name}/{function_name}",
    params(
        FullQuery
    ),
    responses(
        (status = 200, description = "Delete object parameter", body = [String])
    ),
    tag = "universal.any"
)]
pub async fn object_delete_handler(
    Path((class_name, function_name)): Path<(String, String)>,
    query: Query<FullQuery>,
    headers: HeaderMap,
    remote_addr: ConnectInfo<SocketAddr>,
    State(crud_broker): State<Arc<Mutex<CRUDBroker>>>, 
) -> impl IntoResponse {
    let map = full_query_to_map(&query);
    log::trace!("HTTPBroker/object_delete_handler({class_name}, {map:?}) called");
    let v = tokio::task::spawn_blocking(move ||{
        juiz_lock(&crud_broker).unwrap().read_class(class_name.as_str(), function_name.as_str(), construct_capsule_map(CapsuleMap::new(), "DELETE", class_name.as_str(), function_name.as_str(), query, headers, remote_addr))
    }).await;
    let r = json_output_wrap(v.unwrap());
    r
}

pub fn object_router(crud_broker: Arc<Mutex<CRUDBroker>>) -> Router {
    Router::new()
        .route("/:class_name/:function_name", 
                routing::patch(object_patch_handler)
                .get(object_get_handler)
                .delete(object_delete_handler)
                .post(object_post_handler)
                .put(object_put_handler)
        )
        .with_state(Arc::clone(&crud_broker))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        object_get_handler,
        object_patch_handler,
        object_delete_handler,
        object_post_handler,
        object_put_handler,
    ),
    components(schemas(
    ))
)]
#[allow(unused)]
pub struct ApiDoc;