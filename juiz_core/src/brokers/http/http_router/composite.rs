//! 実行中に Composite を定義する API の OpenAPI 定義 (issue #171)
//!
//! 処理は any.rs の汎用ハンドラから crud_callback_container/composite.rs に届く。
//! `identifier` には Composite の型名を渡す。
use crate::prelude::*;
use utoipa::OpenApi;

use super::IdentifierQuery;
use axum::{extract::Query, Json};

#[allow(unused)]
#[utoipa::path(
    post,
    path = "/api/composite/create",
    request_body(content = Value, description = "{\"__manifest__\": <Composite の定義 (conf の composites: の 1 件と同じ形)>}"),
    responses(
        (status = 200, description = "Factory の manifest")
    ),
    tag = "universal.composite",
)]
pub fn create_dummy(Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/composite/list",
    responses(
        (status = 200, description = "Composite の型名の配列")
    ),
    tag = "universal.composite",
)]
pub fn list_dummy() {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/composite/profile_full",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "Factory の manifest")
    ),
    tag = "universal.composite",
)]
pub fn profile_handler_dummy(_query: Query<IdentifierQuery>) {}

#[allow(unused)]
#[utoipa::path(
    delete,
    path = "/api/composite/destroy",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "消した Factory の manifest")
    ),
    tag = "universal.composite",
)]
pub fn delete_dummy(_query: Query<IdentifierQuery>) {}

#[allow(unused)]
#[utoipa::path(
    post,
    path = "/api/composite_container/create",
    request_body(content = Value, description = "{\"__manifest__\": <conf の composite_containers: の 1 件と同じ形>}"),
    responses(
        (status = 200, description = "CompositeContainer の profile")
    ),
    tag = "universal.composite",
)]
pub fn container_create_dummy(Json(_body): Json<Value>) {}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_dummy,
        list_dummy,
        profile_handler_dummy,
        delete_dummy,
        container_create_dummy,
    ),
    components(schemas())
)]
pub struct ApiDoc;
