//! ネーミングサービスの API の OpenAPI 定義 (docs/spec/naming.md §9.4、issue #618)
//!
//! 処理は any.rs の汎用ハンドラから crud_callback_container/naming.rs に届く。
use utoipa::{IntoParams, OpenApi};

use axum::extract::Query;
use serde::Deserialize;

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct NamingListQuery {
    /// true なら各登録に system/profile を問い合わせ、応答しないものに alive: false を付ける
    check: Option<String>,
    /// true なら LAN (mDNS、feature naming-mdns の juiz のとき) で見つけた登録も足す。machine は相手の機械名、found: "lan"
    lan: Option<String>,
}

#[allow(unused)]
#[derive(Deserialize, IntoParams, Debug)]
pub struct NamingResolveQuery {
    /// 登録名 (mobile_sim、robot1/mobile_sim、/mobile_sim、mobile_sim@robot1-pc)
    name: Option<String>,
    /// true ならこの機械と LAN の両方を見る。既定はこの機械で見つからなかったときだけ LAN を見る
    lan: Option<String>,
}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/naming/list",
    params(NamingListQuery),
    responses(
        (status = 200, description = "この System がいる機械の登録の配列 ({format, name, namespace, uuid, pid, host, juiz_version, started_at, package, brokers, alive, machine})。juiz name list --json と同じ形")
    ),
    tag = "universal.naming",
)]
pub fn list_dummy(_query: Query<NamingListQuery>) {}

#[allow(unused)]
#[utoipa::path(
    get,
    path = "/api/naming/resolve",
    params(NamingResolveQuery),
    responses(
        (status = 200, description = "名前に当てはまる、生きている候補の配列 (naming/list の要素と同じ形)。1 つに選ばない")
    ),
    tag = "universal.naming",
)]
pub fn resolve_dummy(_query: Query<NamingResolveQuery>) {}

#[derive(OpenApi)]
#[openapi(paths(list_dummy, resolve_dummy), components(schemas()))]
pub struct ApiDoc;
