use juiz_sdk::value::jvalue;
use utoipa::OpenApi;

use axum::{extract::Query, response::IntoResponse, Json};
use super::{FullQuery, IdentifierQuery, PathQuery, Value};

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/system/profile_full",
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.system",
)]
pub async fn profile_handler_dummy(){
}


#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/system/uuid",
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.system",
)]
pub async fn uuid_dummy(){
}


#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/system/filesystem_list",
    params(
        PathQuery
    ),
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.system",
)]
pub fn fslist_handler_dummy(
    _query: Query<PathQuery>,
){
}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/system/add_subsystem",
    params(
        IdentifierQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.system",
)]
pub fn add_subsystem_dummy(
_query: Query<IdentifierQuery>,
Json(_body): Json<Value>) {
}


#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/system/add_mastersystem",
    params(
        FullQuery
    ),
    request_body = Value,
    responses(
        (status = 200, description = "System")
    ),
    tag = "universal.system",
)]
pub fn add_mastersystem_dummy(
_query: Query<FullQuery>,
Json(_body): Json<Value>) {
}

pub fn system_openapi_handler() -> Json<juiz_sdk::serde_json::Value> {
    Json(jvalue!({
        "openapi": "3.0.1",
        "info": {
            "title": "system openapi",
            "license": "MIT",
            "version": "0.0.1",
        },
        "servers": [
            {
                "url": "http://localhost:8000",
                "description": "localhost"
            }
        ],
        "paths": {
            "/system/list" : {
                "get": {
                    "responses": {
                        "200": {
                            "description": "",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "$ref" : "#/components/schemas/Recipe"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "components" : {
            "schemas": {
                "Recipe": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "integer",
                            "format": "int64"
                        }
                    }
                }
            }
        }

    }))
}


#[derive(OpenApi)]
#[openapi(
    paths(
        profile_handler_dummy,
        uuid_dummy,
        fslist_handler_dummy,
        add_subsystem_dummy,
        add_mastersystem_dummy,
    ),
    components(schemas(
    ))
)]
pub struct ApiDoc;