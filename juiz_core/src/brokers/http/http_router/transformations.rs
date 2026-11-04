//! Transformations の API の OpenAPI 定義 (issue #370、仕様 docs/spec/transformations.md §13.1)
//!
//! 処理は any.rs の汎用ハンドラから crud_callback_container/transformations.rs に届く。
//! 値の型 (`juiz_std.geometry` の Pose・PoseStamped・Transform) は [`add_transformations_schemas`] が
//! `components.schemas` への参照として足す。
use crate::prelude::*;
use utoipa::{IntoParams, OpenApi};

use super::IdentifierQuery;
use axum::{extract::Query, Json};
use serde::Deserialize;

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct PoseQuery {
    /// Container の ID か名前 (根のネームスペースから解決する)
    identifier: Option<String>,
    /// 基準 Container の ID か名前
    reference: Option<String>,
    /// 基準を、この UUID の System から取り込んだ写しだけから探す (別 System から回されたとき、§19.8)
    reference_system: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct SetPoseQuery {
    /// Container の ID か名前
    identifier: Option<String>,
    /// 基準を、この UUID の System から取り込んだ写しだけから探す (§19.8)
    reference_system: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct PoseStateQuery {
    /// Container の ID か名前
    identifier: Option<String>,
    /// 前回受け取った version。同じなら entries を省く
    since: Option<String>,
    /// 前回受け取った system (相手の UUID)。今の UUID と違えば since を無視する
    system: Option<String>,
    /// 取りに来た System の UUID (container/profile_full の remote_children に出る)
    subscriber: Option<String>,
    /// 取りに来た System での終点の ID (複数なら `,` 区切り)
    destination: Option<String>,
    /// 取りに来た側の timeout [秒] (remote_children から消すまでの時間、既定 3.0)
    timeout: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct TransformQuery {
    /// Transform の ID (transform://<始点>|<終点>)
    identifier: Option<String>,
    /// 別 System から回されたとき、始点の System の UUID (§19.2 の 7)
    source_system: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct AnchorQuery {
    /// Anchor の ID か名前
    anchor: Option<String>,
}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container/pose",
    params(PoseQuery),
    responses((status = 200, description = "基準から見た ContainerPose (juiz_std.geometry.PoseStamped)")),
    tag = "universal.transformations",
)]
pub fn pose_dummy(_query: Query<PoseQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/container/pose_state",
    params(PoseStateQuery),
    responses((status = 200, description = "{identifier, system, version, changed, entries: [{anchor, origin_system, origin_anchor, pose, stamp, version}]} (System をまたぐ Transform の取り込みに使う、§19.4)")),
    tag = "universal.transformations",
)]
pub fn pose_state_dummy(_query: Query<PoseStateQuery>) {}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/container/set_pose",
    params(SetPoseQuery),
    request_body(content = Value, description = "{\"reference\": <基準 Container>, \"pose\": <juiz_std.geometry.Pose>}"),
    responses((status = 200, description = "書いた後の ContainerPose (juiz_std.geometry.PoseStamped)")),
    tag = "universal.transformations",
)]
pub fn set_pose_dummy(_query: Query<SetPoseQuery>, Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/transform/list",
    responses((status = 200, description = "Transform の ID (transform://<始点>|<終点>) の配列")),
    tag = "universal.transformations",
)]
pub fn transform_list_dummy() {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/transform/profile_full",
    params(TransformQuery),
    responses((status = 200, description = "{identifier, source, destination, transform, stamp, remote?}")),
    tag = "universal.transformations",
)]
pub fn transform_profile_dummy(_query: Query<TransformQuery>) {}

#[allow(unused)]
#[utoipa::path(
    post,
    path = "/api/transform/create",
    request_body(content = Value, description = "{\"source\": <始点>, \"destination\": <終点>, \"translation\": <juiz_std.geometry.Vector3>, \"rotation\": <juiz_std.geometry.Quaternion>, \"rate\": <Hz>, \"timeout\": <秒>}。translation・rotation・rate・timeout は省略可。終点が別 System なら、その System で作る"),
    responses((status = 200, description = "Transform の profile")),
    tag = "universal.transformations",
)]
pub fn transform_create_dummy(Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/transform/update",
    params(TransformQuery),
    request_body(content = Value, description = "{\"translation\": <Vector3>, \"rotation\": <Quaternion>}。省いたものは今のまま"),
    responses((status = 200, description = "Transform の profile")),
    tag = "universal.transformations",
)]
pub fn transform_update_dummy(_query: Query<TransformQuery>, Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    delete,
    path = "/api/transform/destroy",
    params(TransformQuery),
    responses((status = 200, description = "消した Transform の profile")),
    tag = "universal.transformations",
)]
pub fn transform_destroy_dummy(_query: Query<TransformQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/anchor/list",
    responses((status = 200, description = "Anchor の ID の配列")),
    tag = "universal.transformations",
)]
pub fn anchor_list_dummy() {}

#[allow(unused)]
#[utoipa::path(
    patch,
    path = "/api/anchor/update",
    params(IdentifierQuery),
    request_body(content = Value, description = "{}"),
    responses((status = 200, description = "{identifier, stamp}")),
    tag = "universal.transformations",
)]
pub fn anchor_update_dummy(_query: Query<IdentifierQuery>, Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/transformations/tree",
    params(AnchorQuery),
    responses((status = 200, description = "{identifier, pose, transform?, children: [...]} の入れ子")),
    tag = "universal.transformations",
)]
pub fn tree_dummy(_query: Query<AnchorQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/pose_listener/list",
    responses((status = 200, description = "Pose 通知の ID の配列")),
    tag = "universal.transformations",
)]
pub fn pose_listener_list_dummy() {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/pose_listener/profile_full",
    params(IdentifierQuery),
    responses((status = 200, description = "{identifier, container, process, arg, reference, on_update}")),
    tag = "universal.transformations",
)]
pub fn pose_listener_profile_dummy(_query: Query<IdentifierQuery>) {}

#[allow(unused)]
#[utoipa::path(
    post,
    path = "/api/pose_listener/create",
    request_body(content = Value, description = "{\"container\", \"process\", \"arg\"?, \"reference\"?, \"on_update\"?}"),
    responses((status = 200, description = "Pose 通知の profile")),
    tag = "universal.transformations",
)]
pub fn pose_listener_create_dummy(Json(_body): Json<Value>) {}

#[allow(unused)]
#[utoipa::path(
    delete,
    path = "/api/pose_listener/destroy",
    params(IdentifierQuery),
    responses((status = 200, description = "外した Pose 通知の profile")),
    tag = "universal.transformations",
)]
pub fn pose_listener_destroy_dummy(_query: Query<IdentifierQuery>) {}

#[derive(OpenApi)]
#[openapi(
    paths(
        pose_dummy,
        pose_state_dummy,
        set_pose_dummy,
        transform_list_dummy,
        transform_profile_dummy,
        transform_create_dummy,
        transform_update_dummy,
        transform_destroy_dummy,
        anchor_list_dummy,
        anchor_update_dummy,
        tree_dummy,
        pose_listener_list_dummy,
        pose_listener_profile_dummy,
        pose_listener_create_dummy,
        pose_listener_destroy_dummy,
    ),
    components(schemas())
)]
pub struct ApiDoc;

const POSE_STAMPED: &str = "juiz_std.geometry.PoseStamped";
const POSE: &str = "juiz_std.geometry.Pose";
const TRANSFORM: &str = "juiz_std.geometry.Transform";
const VECTOR3: &str = "juiz_std.geometry.Vector3";
const QUATERNION: &str = "juiz_std.geometry.Quaternion";

fn schema_ref(name: &str) -> Value {
    jvalue!({ "$ref": format!("#/components/schemas/{name}") })
}

fn set(api: &mut Value, pointer: &str, schema: Value) {
    if let Some(v) = api.pointer_mut(pointer) {
        *v = jvalue!({ "application/json": { "schema": schema } });
    }
}

fn set_response(api: &mut Value, path: &str, method: &str, schema: Value) {
    let pointer = format!("/paths/{}/{method}/responses/200", path.replace('/', "~1"));
    if let Some(r) = api.pointer_mut(&pointer) {
        r["content"] = jvalue!({ "application/json": { "schema": schema } });
    }
}

/// OpenAPI 定義の Transformations の API に、値の型 (`components.schemas` の `juiz_std.geometry` の型) を書き足す。
/// `components.schemas` に型が無ければ何もしない
pub fn add_transformations_schemas(api: &mut Value) {
    if api
        .pointer(&format!("/components/schemas/{POSE_STAMPED}"))
        .is_none()
    {
        return;
    }
    set_response(api, "/api/container/pose", "get", schema_ref(POSE_STAMPED));
    set_response(
        api,
        "/api/container/set_pose",
        "patch",
        schema_ref(POSE_STAMPED),
    );
    set(
        api,
        "/paths/~1api~1container~1set_pose/patch/requestBody/content",
        jvalue!({
            "type": "object",
            "required": ["reference", "pose"],
            "properties": {
                "reference": {"type": "string", "description": "基準 Container の ID か名前"},
                "pose": schema_ref(POSE),
            }
        }),
    );
    let transform_profile = jvalue!({
        "type": "object",
        "properties": {
            "identifier": {"type": "string"},
            "source": {"type": "string"},
            "destination": {"type": "string"},
            "transform": schema_ref(TRANSFORM),
            "stamp": schema_ref("juiz_std.common.Time"),
            "remote": {
                "type": "object",
                "description": "System をまたぐ Transform の取り込みの様子 (§19.10)",
                "properties": {
                    "system": {"type": "string", "nullable": true},
                    "rate": {"type": "number"},
                    "timeout": {"type": "number"},
                    "state": {"type": "string", "enum": ["ok", "stale", "lost"]},
                    "last_success": schema_ref("juiz_std.common.Time"),
                }
            },
        }
    });
    for (path, method) in [
        ("/api/transform/profile_full", "get"),
        ("/api/transform/create", "post"),
        ("/api/transform/update", "patch"),
        ("/api/transform/destroy", "delete"),
    ] {
        set_response(api, path, method, transform_profile.clone());
    }
    set(
        api,
        "/paths/~1api~1transform~1create/post/requestBody/content",
        jvalue!({
            "type": "object",
            "required": ["source", "destination"],
            "properties": {
                "source": {"type": "string", "description": "始点 (親) の Container の ID か名前"},
                "destination": {"type": "string", "description": "終点 (子) の Container の ID か名前"},
                "translation": schema_ref(VECTOR3),
                "rotation": schema_ref(QUATERNION),
                "rate": {"type": "number", "description": "System をまたぐ Transform の取り込みの周期 [Hz] (既定 10.0)"},
                "timeout": {"type": "number", "description": "取り込めないとき写しを消すまでの時間 [秒] (既定 3.0)"},
                "source_system": {"type": "string", "description": "別 System から回されたとき、始点の System の UUID"},
            }
        }),
    );
    set(
        api,
        "/paths/~1api~1transform~1update/patch/requestBody/content",
        jvalue!({
            "type": "object",
            "properties": {
                "translation": schema_ref(VECTOR3),
                "rotation": schema_ref(QUATERNION),
            }
        }),
    );
}
