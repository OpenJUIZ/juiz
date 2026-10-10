use utoipa::OpenApi;

use crate::prelude::*;
use super::{RecursiveQuery, IdentifierQuery};
use axum::{extract::Query, Json};


#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container_process/profile_full",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn profile_handler_dummy(
    _query: Query<IdentifierQuery>,) {
}


#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container_process/openapi_spec",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn openapi_spec_handler_dummy(
    _query: Query<IdentifierQuery>,) {
}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container_process/list",
    params(
        RecursiveQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn list_dummy(_query: Query<RecursiveQuery>) {
}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/container_process/call",
    params(
        IdentifierQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn call_dummy(
_query: Query<IdentifierQuery>,
Json(_body): Json<Value>) {
}


#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/container_process/execute",
    params(
        IdentifierQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn execute_dummy(
_query: Query<IdentifierQuery>,
Json(_body): Json<Value>) {
}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/container_process/p_apply",
    params(
        IdentifierQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn p_apply_dummy(
_query: Query<IdentifierQuery>,
Json(_body): Json<Value>) {
}

#[allow(unused)]
#[utoipa::path(
    delete,
    path = "/api/container_process/destroy",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container_process",
)]
pub fn delete_dummy(
_query: Query<IdentifierQuery>) {
}

#[derive(OpenApi)]
#[openapi(
    paths(
        profile_handler_dummy,
        openapi_spec_handler_dummy,
        list_dummy,
        call_dummy,
        execute_dummy,
        delete_dummy,
        p_apply_dummy,
    ),
    components(schemas(
    ))
)]
pub struct ApiDoc;