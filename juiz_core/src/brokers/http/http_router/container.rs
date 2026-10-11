use utoipa::OpenApi;

use super::{IdentifierQuery, RecursiveQuery};
use axum::extract::Query;

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
        RecursiveQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.container",
)]
pub fn list_dummy(_query: Query<RecursiveQuery>) {}

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
    paths(profile_handler_dummy, list_dummy, delete_dummy,),
    components(schemas())
)]
pub struct ApiDoc;
