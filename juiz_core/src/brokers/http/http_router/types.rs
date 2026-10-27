//! 型の登録所の API の OpenAPI 定義 (issue #193)
//!
//! 処理は any.rs の汎用ハンドラから crud_callback_container/types.rs に届く。
use utoipa::OpenApi;

use super::IdentifierQuery;
use axum::extract::Query;

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/type/list",
    responses(
        (status = 200, description = "登録している名前付き型の配列 ({identifier, name, fingerprint, short_identifier, kind, used_by})")
    ),
    tag = "universal.type",
)]
pub fn list_dummy() {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/type/schema",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "型参照 {name, fingerprint, avro}。identifier は <完全名>@sha256:… か完全名だけ")
    ),
    tag = "universal.type",
)]
pub fn schema_dummy(_query: Query<IdentifierQuery>) {}

#[derive(OpenApi)]
#[openapi(paths(list_dummy, schema_dummy,), components(schemas()))]
pub struct ApiDoc;
