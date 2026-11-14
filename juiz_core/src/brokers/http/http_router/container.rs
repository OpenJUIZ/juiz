use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use utoipa::OpenApi;

use super::{IdentifierQuery, ListQuery};
use crate::brokers::{localize_identifier, CRUDBroker};
use crate::models::ContainerModelData;
use crate::prelude::*;

/// `container/model_info`・`container/model` の引数 (docs/spec/container-model.md §6.1)
#[allow(unused)]
#[derive(serde::Deserialize, utoipa::IntoParams, Debug)]
pub struct ModelQuery {
    /// Container の ID か名前 (根のネームスペースから解決する)。パスは受け付けない
    identifier: Option<String>,
}

/// `If-None-Match` に ETag `etag` (引用符つき) か `*` があるか (弱い比較の `W/` も同じとみなす)
fn if_none_match_hits(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim())
        .any(|t| t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag)
}

/// `Content-Disposition: inline; filename="<file_name>"`。ASCII でない名前は filename* (RFC 6266) も付ける
fn content_disposition(file_name: &str) -> HeaderValue {
    let plain = file_name
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ');
    let v = if plain {
        format!("inline; filename=\"{file_name}\"")
    } else {
        let encoded: String = file_name
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        format!("inline; filename=\"model\"; filename*=UTF-8''{encoded}")
    };
    HeaderValue::from_str(&v).unwrap_or_else(|_| HeaderValue::from_static("inline"))
}

/// 304 Not Modified (本文なし、ETag は付ける)
fn not_modified(sha256: &str) -> Response {
    let etag = HeaderValue::from_str(&format!("\"{sha256}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("\"\""));
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .body(Body::empty())
        .unwrap()
        .into_response()
}

/// モデルの中身の HTTP の応答 (§6.2)。`If-None-Match` が当たれば 304
pub fn model_response(d: ContainerModelData, headers: &HeaderMap) -> Response {
    let etag = format!("\"{}\"", d.sha256());
    let etag_value = HeaderValue::from_str(&etag).unwrap_or_else(|_| HeaderValue::from_static("\"\""));
    let no_cache = HeaderValue::from_static("no-cache");
    if if_none_match_hits(headers, &etag) {
        return not_modified(d.sha256());
    }
    let media_type = HeaderValue::from_str(d.media_type())
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let disposition = content_disposition(d.file_name());
    let len = d.data.len();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media_type)
        .header(header::CONTENT_LENGTH, len)
        .header(header::ETAG, etag_value)
        .header(header::CACHE_CONTROL, no_cache)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(d.data))
        .unwrap()
        .into_response()
}

/// `GET /api/container/model`: モデルの中身を生のバイト列で返す (§6.2)。
/// 別 System の Container も、中継してここで生のバイト列にする
pub async fn model_handler(
    query: Query<ModelQuery>,
    headers: HeaderMap,
    uri: Uri,
    State(crud_broker): State<Arc<Mutex<CRUDBroker>>>,
) -> Response {
    let Some(identifier) = query.identifier.clone() else {
        return super::error_response(
            juiz_sdk::anyhow::anyhow!(JuizError::CRUDBrokerCanNotParameterFunctionError {
                key_name: "identifier".to_owned()
            }),
            false,
        );
    };
    let host = super::any::accessed_host(&headers, &uri);
    let identifier = localize_identifier(&identifier, "http", host.as_deref());
    log::trace!("[GET] container/model({identifier})");
    // ファイルの読み出しは重いのでワーカーの外で (issue #363)
    let v = tokio::task::spawn_blocking(move || {
        let dispatcher = juiz_lock(&crud_broker).unwrap().dispatcher();
        // If-None-Match があれば、先にモデル情報 (SHA-256) だけ見て、同じなら中身を読まずに 304
        if headers.contains_key(header::IF_NONE_MATCH) {
            match dispatcher.container_model_info(&identifier) {
                Ok(info) => {
                    if let Some(sha) = info.get("sha256").and_then(|s| s.as_str()) {
                        if if_none_match_hits(&headers, &format!("\"{sha}\"")) {
                            return not_modified(sha);
                        }
                    }
                }
                Err(e) => return super::error_response(e, false),
            }
        }
        match dispatcher.container_model_data(&identifier) {
            Ok(d) => model_response(d, &headers),
            Err(e) => super::error_response(e, false),
        }
    })
    .await;
    v.unwrap()
}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container/model_info",
    params(
        ModelQuery
    ),
    responses(
        (status = 200, description = "モデル情報 (juiz_std.model.ContainerModelInfo)。モデルが無ければ null"),
        (status = 404, description = "モデルのファイルが無い (ContainerModelFileNotFoundError)"),
    ),
    tag = "universal.container",
)]
pub fn model_info_dummy(_query: Query<ModelQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container/model",
    params(
        ModelQuery,
        ("If-None-Match" = Option<String>, Header, description = "前に受け取った ETag。同じなら 304"),
    ),
    responses(
        (status = 200, description = "モデルのファイルの中身 (Content-Type は形式による、ETag は SHA-256)"),
        (status = 304, description = "If-None-Match の ETag と同じ"),
        (status = 404, description = "モデルが無い (ContainerHasNoModelError)・ファイルが無い (ContainerModelFileNotFoundError)"),
    ),
    tag = "universal.container",
)]
pub fn model_dummy(_query: Query<ModelQuery>) {}

const CONTAINER_MODEL_INFO: &str = "juiz_std.model.ContainerModelInfo";

/// OpenAPI 定義の `container/model_info` と `container/profile_full` の `model` に
/// `juiz_std.model.ContainerModelInfo` を参照させる (§8)。`components.schemas` に型が無ければ何もしない
pub fn add_container_model_schemas(api: &mut Value) {
    // container/model は生のバイト列 (§6.2)
    if let Some(r) = api.pointer_mut("/paths/~1api~1container~1model/get/responses/200") {
        let binary = jvalue!({ "schema": { "type": "string", "format": "binary" } });
        let mut content = serde_json::Map::new();
        for f in juiz_sdk::manifests::ModelFormat::ALL {
            content.insert(f.media_type().to_owned(), binary.clone());
        }
        r["content"] = Value::Object(content);
    }
    if api
        .pointer(&format!("/components/schemas/{CONTAINER_MODEL_INFO}"))
        .is_none()
    {
        return;
    }
    let info = jvalue!({
        "oneOf": [
            { "$ref": format!("#/components/schemas/{CONTAINER_MODEL_INFO}") },
            { "type": "null" },
        ]
    });
    if let Some(r) = api.pointer_mut("/paths/~1api~1container~1model_info/get/responses/200") {
        r["content"] = jvalue!({ "application/json": { "schema": info.clone() } });
    }
    if let Some(r) = api.pointer_mut("/paths/~1api~1container~1profile_full/get/responses/200") {
        r["content"] = jvalue!({ "application/json": { "schema": {
            "type": "object",
            "properties": { "model": info },
        } } });
    }
}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container/profile_full",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "Container")
    ),
    tag = "universal.container",
)]
pub fn profile_handler_dummy(_query: Query<IdentifierQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container/list",
    params(
        ListQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container",
)]
pub fn list_dummy(_query: Query<ListQuery>) {}

#[allow(unused)]
#[utoipa::path(
    delete,
    path = "/api/container/destroy",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container",
)]
pub fn delete_dummy(_query: Query<IdentifierQuery>) {}

#[derive(OpenApi)]
#[openapi(
    paths(profile_handler_dummy, list_dummy, delete_dummy, model_info_dummy, model_dummy,),
    components(schemas())
)]
pub struct ApiDoc;
