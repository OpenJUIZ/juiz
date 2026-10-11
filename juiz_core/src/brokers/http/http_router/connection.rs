use juiz_sdk::serde;
use serde::Deserialize;
use utoipa::OpenApi;

use super::IdentifierQuery;

use axum::extract::Query;
use utoipa::ToSchema;

#[allow(dead_code)]
#[derive(Deserialize, ToSchema)]
pub struct IdentifierManifest {
    // Identifier
    identifier: String,
    //type_name: String,
    //name: String,
}

#[allow(dead_code)]
#[derive(Deserialize, ToSchema)]
pub struct CreateConnectionRequest {
    source: IdentifierManifest,
    destination: IdentifierManifest,
    arg_name: String,
}

#[allow(unused)]
#[utoipa::path(
    post,
    path = "/api/connection/create",
    responses(
        (status = 200, description = "System")
    ),
    request_body = CreateConnectionRequest,
    tag = "universal.connection",
)]
pub fn create_dummy() {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/connection/profile_full",
    params(
        IdentifierQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.connection",
)]
pub fn profile_handler_dummy(_query: Query<IdentifierQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/connection/list",
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.connection",
)]
pub fn list_dummy() {}

#[derive(OpenApi)]
#[openapi(
    paths(create_dummy, profile_handler_dummy, list_dummy,),
    components(schemas(CreateConnectionRequest, IdentifierManifest))
)]
pub struct ApiDoc;
