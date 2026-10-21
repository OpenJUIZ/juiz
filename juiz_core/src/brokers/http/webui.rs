//! 管理画面(webui)を HTTP ブローカーの /ui で配信する(issue #117)。
//!
//! feature "webui" のとき、juiz_webui クレートが埋め込んだ webui のビルド成果物を配信する。
//! ビルド成果物が無いままビルドしたときは、/ui/ で画面のビルド方法を案内する。

use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};

/// 画面を配信する URL
pub const WEBUI_PATH: &str = "/ui";

/// 画面のビルド成果物が埋め込まれているか
pub fn webui_embedded() -> bool {
    juiz_webui::embedded()
}

/// /ui を配信する Router。/ui は /ui/ に付け直す(画面は相対パスで部品を読むため)。
pub fn webui_router() -> Router {
    Router::new()
        .route(
            WEBUI_PATH,
            get(|| async { Redirect::permanent(&format!("{WEBUI_PATH}/")) }),
        )
        .route(
            &format!("{WEBUI_PATH}/"),
            get(|| async { serve("index.html") }),
        )
        .route(
            &format!("{WEBUI_PATH}/*path"),
            get(|Path(path): Path<String>| async move { serve(&path) }),
        )
}

fn serve(path: &str) -> Response {
    let path = if path.is_empty() || path.ends_with('/') {
        format!("{path}index.html")
    } else {
        path.to_owned()
    };
    match juiz_webui::get(&path) {
        Some(file) => {
            // assets/ の下はファイル名にハッシュが付くので長く持たせ、index.html は毎回確かめさせる
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [
                    (header::CONTENT_TYPE, file.metadata.mimetype().to_owned()),
                    (header::CACHE_CONTROL, cache.to_owned()),
                ],
                Body::from(file.data.into_owned()),
            )
                .into_response()
        }
        None if path == "index.html" => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            NOT_BUILT_HTML,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 画面を埋め込まずにビルドしたときの案内
const NOT_BUILT_HTML: &str = r#"<!doctype html>
<html lang="ja">
<head><meta charset="utf-8"><title>juiz 管理画面</title></head>
<body style="background:#05070a;color:#d8e2ea;font-family:sans-serif;padding:2em;line-height:1.7">
<h1>juiz 管理画面は同梱されていません</h1>
<p>この juiz は管理画面(webui)のビルド成果物が無いままビルドされました。リポジトリの最上位で次を実行してから juiz をビルドし直すと、ここに画面が出ます。</p>
<pre style="background:#10161c;padding:1em">cd webui
npm ci
npm run build
cd ..
cargo build</pre>
<p>HTTP API は <a href="../api-docs/openapi.json" style="color:#6fe">/api-docs/openapi.json</a> で確かめられます。</p>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_not_found() {
        assert_eq!(serve("no_such_file.js").status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn index_is_html_with_or_without_build() {
        let res = serve("");
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html"));
    }
}
