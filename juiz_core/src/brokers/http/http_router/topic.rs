use crate::{
    brokers::http::http_router::{RecursiveQuery, TopicNameAndUuidQuery},
    prelude::*,
};
use utoipa::OpenApi;

use axum::{extract::Query, Json};

// #[allow(unused)]
// #[utoipa::path(
//     post,
//     path = "/api/process/create",
//     request_body = Value,
//     responses(
//         (status = 200, description = "System")
//     ),
//     tag = "universal.process",
// )]
// pub fn create_dummy(
// _query: Query<IdentifierQuery>,
// Json(_body): Json<Value>) {
// }

// #[allow(unused)]
// #[utoipa::path(
//     get,
//     path = "/api/process/profile_full",
//     params(
//         IdentifierQuery
//     ),
//     responses(
//         (status = 200, description = "System")
//     ),
//     tag = "universal.process",
// )]
// pub fn profile_handler_dummy(
//     _query: Query<IdentifierQuery>,) {
// }

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/topic/list",
    params(
        RecursiveQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.topic",
)]
pub fn list_dummy(_query: Query<RecursiveQuery>) {}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/topic/push",
    params(
        TopicNameAndUuidQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.topic",
)]
pub fn push_dummy(_query: Query<TopicNameAndUuidQuery>, Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/topic/request_subscribe",
    params(
        TopicNameAndUuidQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.topic",
)]
pub fn request_subscribe_dummy(_query: Query<TopicNameAndUuidQuery>, Json(_body): Json<Value>) {}

// #[allow(unused)]
// #[utoipa::path(
//     patch,
//     path = "/api/process/execute",
//     params(
//         IdentifierQuery
//     ),
//     request_body = Value,
//     responses(
//         (status = 200, description = "System")
//     ),
//     tag = "universal.process",
// )]
// pub fn execute_dummy(
// _query: Query<IdentifierQuery>,
// Json(_body): Json<Value>) {
// }

// #[allow(unused)]
// #[utoipa::path(
//     delete,
//     path = "/api/process/destroy",
//     params(
//         IdentifierQuery
//     ),
//     responses(
//         (status = 200, description = "System")
//     ),
//     tag = "universal.process",
// )]
// pub fn delete_dummy(
// _query: Query<IdentifierQuery>) {
// }

#[derive(OpenApi)]
#[openapi(
    paths(list_dummy, push_dummy, request_subscribe_dummy,),
    components(schemas())
)]
pub struct ApiDoc;
