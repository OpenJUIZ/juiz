use image::EncodableLayout;
use juiz_sdk::anyhow::anyhow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
// use crate::prelude::*;
use crate::brokers::CRUDBroker;

use axum::extract::Multipart;
use axum::http::HeaderValue;

// #[cfg(feature="opencv4")]
// use opencv::{core::{Vector, VectorToVec}, imgcodecs::imencode};

use tower_http::services::ServeDir;
use utoipa::{IntoParams, OpenApi};
#[cfg(feature = "swagger-ui")]
use utoipa_swagger_ui::SwaggerUi;

use axum::{
    body::Body,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use serde::Deserialize;

use crate::prelude::*;

pub mod any;
pub mod any_process;
pub mod broker;
pub mod composite;
pub mod connection;
pub mod container;
pub mod container_process;
pub mod execution_context;
pub mod process;
pub mod system;
pub mod topic;
pub mod types;
// use cv_convert::TryFromCv;

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct IdentifierQuery {
    identifier: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct PathQuery {
    path: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct RecursiveQuery {
    recursive: Option<String>,
}

/// list の引数。namespace を指定すると、そのネームスペースとその下だけを返す (issue #150)
#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct ListQuery {
    recursive: Option<String>,
    namespace: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct TopicNameAndUuidQuery {
    topic_name: Option<String>,
    system_uuid: Option<String>,
}

#[derive(Deserialize, IntoParams, Debug)]
pub struct FullQuery {
    identifier: Option<String>,
    path: Option<String>,
    recursive: Option<String>,
    system_uuid: Option<String>,
    topic_name: Option<String>,
    namespace: Option<String>,
}

pub fn full_query_to_map(query: &Query<FullQuery>) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    match query.identifier.clone() {
        None => {}
        Some(v) => {
            map.insert("identifier".to_owned(), v);
        }
    }
    match query.recursive.clone() {
        None => {}
        Some(v) => {
            map.insert("recursive".to_owned(), v);
        }
    }
    match query.path.clone() {
        None => {}
        Some(v) => {
            map.insert("path".to_owned(), v);
        }
    }
    match query.system_uuid.clone() {
        None => {}
        Some(v) => {
            map.insert("system_uuid".to_owned(), v);
        }
    }
    match query.topic_name.clone() {
        None => {}
        Some(v) => {
            map.insert("topic_name".to_owned(), v);
        }
    }
    match query.namespace.clone() {
        None => {}
        Some(v) => {
            map.insert("namespace".to_owned(), v);
        }
    }
    map
}

pub fn json_output_wrap(result: JuizResult<CapsulePtr>) -> impl IntoResponse {
    //log::trace!("json_output_wrap() called");
    if result.is_err() {
        let e = result.err().unwrap();
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(jvalue!({
                "message": format!("Internal Server Error:  {:#}, {:}", e, e.to_string())
            })),
        )
            .into_response();
    }
    match result {
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(jvalue!({
                "message": format!("Internal Server Error:  {:#}, {:}", e, e.to_string())
            })),
        )
            .into_response(),
        Ok(v) => capsule_ptr_to_response(v),
    }
}

//#[cfg(not(feature= "opencv4"))]
fn capsule_ptr_to_response(v: CapsulePtr) -> axum::http::Response<Body> {
    use juiz_sdk::image::ImageFormat;

    // Native の値はここで encode する。失敗したらエラーの応答にする (lazy-evaluation.md §5)
    let encode_error = |e: juiz_sdk::anyhow::Error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(jvalue!({
                "message": format!("Internal Server Error:  {:#}, {:}", e, e.to_string())
            })),
        )
            .into_response()
    };
    if v.is_value().unwrap() {
        v.lock_as_value(|value| {
            let mut r = Json(value).into_response();
            let hdrs = r.headers_mut();
            hdrs.append("Cache-Control", HeaderValue::from_str("no-cache").unwrap());
            r
        })
        .unwrap_or_else(encode_error)
    } else if v.is_image().unwrap() {
        use std::io::{BufWriter, Cursor};
        // log::trace!("Detect Image Response v={v:?}");
        v.lock_as_image(|image| {
            let mut buffer = BufWriter::new(Cursor::new(Vec::new()));
            image.write_to(&mut buffer, ImageFormat::Png).unwrap();

            //let bytes = buffer.into_inner().unwrap().into_inner().as_bytes();
            Response::builder()
                .extension("png")
                .header("Content-Type", "image/png")
                .status(StatusCode::OK)
                .body(Body::from(buffer.into_inner().unwrap().into_inner()))
                .unwrap()
                .into_response()
        })
        .unwrap_or_else(encode_error)
    } else {
        Json(jvalue!({})).into_response()
    }
}

// #[cfg(feature= "opencv4")]
// fn capsule_ptr_to_response(v: CapsulePtr) -> axum::http::Response<Body>  {
//     if v.is_value().unwrap() {
//         v.lock_as_value(|value| {
//             let mut r = Json(value).into_response();
//             let hdrs = r.headers_mut();
//             hdrs.append("Cache-Control", HeaderValue::from_str("no-cache").unwrap());
//             r
//         }).unwrap()
//     } else if v.is_mat().unwrap() {
//         v.lock_as_mat(|result| {
//             let mut buf : opencv::core::Vector<u8> = Vector::new();

//             match imencode(".png", result, &mut buf, &Vector::new()) {
//                 Ok(_result) => {
//                     Response::builder()
//                         .extension("png")
//                         .header("Content-Type", "image/png")
//                         .status(StatusCode::OK)
//                         .body(Body::from(buf.to_vec())).unwrap().into_response()
//                 },
//                 Err(e) => {
//                     Json(jvalue!({
//                         "result": "Error",
//                         "error": format!("{}", e)
//                     })).into_response()
//                 },
//             }

//         }).unwrap()
//     } else {
//         Json(jvalue!({})).into_response()
//     }
// }

/// multipart を読めなかったときの応答の状態と文 (issue #233、docs/spec/large-data.md §7)。
/// 本文が上限 (max_body_bytes) を超えたら 413、形が正しくなければ 400
#[derive(Debug)]
pub struct MultipartReadError {
    pub status: StatusCode,
    pub message: String,
}

impl MultipartReadError {
    fn bad_request(message: String) -> Self {
        log::error!("{message}");
        MultipartReadError {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    fn from_multipart(e: axum::extract::multipart::MultipartError) -> Self {
        let status = e.status();
        let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
            format!("request body is too large ({}). Raise option.http_broker.max_body_bytes of the server", e.body_text())
        } else {
            format!("Multipart/FormData parsing error ({})", e.body_text())
        };
        log::error!("{message}");
        MultipartReadError { status, message }
    }

    pub fn into_response(self) -> Response {
        (self.status, Json(jvalue!({ "message": self.message }))).into_response()
    }
}

/// multipart/form-data の本文を CapsuleMap にする。部分の Content-Type が image/png なら画像、
/// application/json なら JSON (読めなければ文字列のまま) にする
pub async fn multipart_to_capsule_map(
    mut multipart: Multipart,
) -> Result<CapsuleMap, MultipartReadError> {
    let mut cm = CapsuleMap::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(MultipartReadError::from_multipart)?
    {
        let Some(field_name) = field.name().map(|n| n.to_owned()) else {
            return Err(MultipartReadError::bad_request(
                "Multipart/FormData does not have 'name' in a part. This can not be handled.".to_owned(),
            ));
        };
        let Some(content_type) = field.content_type().map(|c| c.to_owned()) else {
            return Err(MultipartReadError::bad_request(format!(
                "Multipart/FormData part '{field_name}' does not have 'content-type'. This can not be handled."
            )));
        };
        match content_type.as_str() {
            "image/png" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(MultipartReadError::from_multipart)?;
                let img = image::load_from_memory_with_format(bytes.as_bytes(), image::ImageFormat::Png)
                    .map_err(|e| {
                        MultipartReadError::bad_request(format!(
                            "Multipart/FormData part '{field_name}' is not a PNG image ({e})"
                        ))
                    })?;
                cm.insert(field_name, img.into());
            }
            "application/json" => {
                let text = field
                    .text()
                    .await
                    .map_err(MultipartReadError::from_multipart)?;
                // pyjuiz などは値を JSON の文字列にして送る。JSON として読めないものは文字列のまま渡す
                let v = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
                cm.insert(field_name, v.into());
            }
            _ => {
                return Err(MultipartReadError::bad_request(format!(
                    "Multipart/FormData part '{field_name}' has unsupported 'content-type' ({content_type}). Use image/png or application/json."
                )));
            }
        }
    }
    Ok(cm)
}

#[derive(OpenApi)]
#[openapi(tags(), paths(), components(schemas()))]
struct ApiDoc;

/// HTTP ブローカーの Router を作る。
///
/// serve_webui が true なら、埋め込んだ管理画面を /ui で配信する(feature "webui" のときだけ、issue #117)。
pub fn app_new(
    crud_broker: Arc<Mutex<CRUDBroker>>,
    static_filepaths: Option<Vec<(String, PathBuf)>>,
    serve_webui: bool,
) -> Router {
    let mut api = ApiDoc::openapi();
    api.merge(system::ApiDoc::openapi());
    api.merge(process::ApiDoc::openapi());
    api.merge(container::ApiDoc::openapi());
    api.merge(container_process::ApiDoc::openapi());
    api.merge(broker::ApiDoc::openapi());
    api.merge(execution_context::ApiDoc::openapi());
    api.merge(connection::ApiDoc::openapi());
    api.merge(topic::ApiDoc::openapi());
    api.merge(any_process::ApiDoc::openapi());
    api.merge(composite::ApiDoc::openapi());
    api.merge(types::ApiDoc::openapi());
    log::trace!("app_new(static_filepaths: {static_filepaths:?}) called");
    let r = Router::new();
    // Swagger UI は feature "swagger-ui" のときだけ。無いときも OpenAPI 定義は返す。
    #[cfg(feature = "swagger-ui")]
    let r = r.merge(
        SwaggerUi::new("/docs").config(utoipa_swagger_ui::Config::from("/api-docs/openapi.json")),
    );
    // OpenAPI 定義には、問い合わせのたびに今の型と Factory の引数のスキーマを足す (issue #197)
    let api = serde_json::to_value(&api).unwrap_or_default();
    let crud = crud_broker.clone();
    let r = r.route(
        "/api-docs/openapi.json",
        axum::routing::get(move || {
            let api = api.clone();
            let crud = crud.clone();
            async move { axum::Json(openapi_with_types(api, &crud)) }
        }),
    );
    let mut r = r.nest("/api/", any::object_router(crud_broker.clone()));
    #[cfg(feature = "webui")]
    if serve_webui {
        r = r.merge(super::webui::webui_router());
    }
    #[cfg(not(feature = "webui"))]
    let _ = serve_webui;

    match static_filepaths {
        Some(paths) => {
            for (url, path) in paths.iter() {
                log::debug!("http_broker serves url={url} for path={path:?}");
                r = r.nest_service(url.as_str(), ServeDir::new(path));
            }
            r
        }
        None => r,
    }
}

/// 静的な OpenAPI 定義に、型の登録所の型と Factory ごとの引数・戻り値のスキーマを足す (issue #197、仕様 datatype.md §23.3)
///
/// `components.schemas` に足し、`process/call`・`container_process/call` の本体のスキーマを
/// Factory ごとの引数のスキーマの `oneOf` にする (Swagger UI で Factory を選んで例を出せる)。
/// 足せないときは静的な定義のまま返す。
pub fn openapi_with_types(mut api: Value, crud_broker: &Arc<Mutex<CRUDBroker>>) -> Value {
    let components = match juiz_lock(crud_broker).and_then(|c| c.openapi_components()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("openapi.json: types are not added ({e})");
            return api;
        }
    };
    for kind in ["process", "container_process"] {
        let refs: Vec<Value> = components
            .keys()
            .filter(|k| k.starts_with(&format!("{kind}.")) && k.ends_with(".arguments"))
            .map(|k| jvalue!({ "$ref": format!("#/components/schemas/{k}") }))
            .collect();
        if refs.is_empty() {
            continue;
        }
        if let Some(content) = api.pointer_mut(&format!(
            "/paths/~1api~1{kind}~1call/patch/requestBody/content/application~1json"
        )) {
            content["schema"] = jvalue!({ "oneOf": refs });
        }
    }
    if !api["components"].is_object() {
        api["components"] = jvalue!({});
    }
    if !api["components"]["schemas"].is_object() {
        api["components"]["schemas"] = jvalue!({});
    }
    if let Some(schemas) = api["components"]["schemas"].as_object_mut() {
        schemas.extend(components);
    }
    api
}

