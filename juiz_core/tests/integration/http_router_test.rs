//! HTTP ルータのテスト (issue #221)
//!
//! http_broker_api_test は BrokerProxy から JSON の API を呼ぶ。ここでは HTTP クライアントで直接叩き、
//! 画像を返すプロセスの応答 (PNG)、multipart/form-data で画像を送る PUT、エラーの応答 (500 と message)、
//! OpenAPI の定義を返す口を確かめる。
//! Python のプロセスは tests/python から読む。

use std::io::Cursor;

use juiz_core::prelude::*;
use juiz_sdk::image::{self, DynamicImage, ImageFormat, Rgb, RgbImage};
use reqwest::blocking::Client;

const PORT: i64 = 8026;
const MASTER_PORT: i64 = 8027;
const BOUNDARY: &str = "juiz-test-boundary";

fn system_manifest() -> Value {
    jvalue!({
        "name": "http_router_test",
        "option": {
            "http_broker": { "start": true, "port": PORT },
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": {
            "process_factories": {
                "py_bench_make_image": { "language": "python", "path": "tests/python" },
                "py_bench_invert": { "language": "python", "path": "tests/python" }
            }
        },
        "processes": [
            { "type_name": "py_bench_make_image", "name": "make0" },
            { "type_name": "py_bench_invert", "name": "inv0" }
        ]
    })
}

fn url(path: &str) -> String {
    format!("http://localhost:{PORT}{path}")
}

fn client() -> JuizResult<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?)
}

fn png_bytes(img: &RgbImage) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(img.clone())
        .write_to(&mut buf, ImageFormat::Png)
        .unwrap();
    buf.into_inner()
}

/// multipart/form-data の本体を作る。(フィールド名, Content-Type, 中身)。Content-Type が None なら付けない
fn multipart_body(fields: &[(&str, Option<&str>, Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, content_type, data) in fields {
        body.extend(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend(format!("Content-Disposition: form-data; name=\"{name}\"\r\n").as_bytes());
        if let Some(ct) = content_type {
            body.extend(format!("Content-Type: {ct}\r\n").as_bytes());
        }
        body.extend(b"\r\n");
        body.extend(data);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn put_multipart(
    c: &Client,
    identifier: &str,
    fields: &[(&str, Option<&str>, Vec<u8>)],
) -> JuizResult<reqwest::blocking::Response> {
    Ok(c.put(url("/api/process/call"))
        .query(&[("identifier", identifier)])
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(multipart_body(fields))
        .send()?)
}

fn decode_png(res: reqwest::blocking::Response) -> JuizResult<RgbImage> {
    assert!(res.status().is_success(), "{:?}", res.status());
    assert_eq!(
        res.headers()
            .get("Content-Type")
            .map(|v| v.to_str().unwrap()),
        Some("image/png")
    );
    let bytes = res.bytes()?;
    Ok(image::load_from_memory_with_format(&bytes, ImageFormat::Png)?.to_rgb8())
}

/// 500 の応答の本体は {"message": ...} で、理由が読める
fn assert_error(res: reqwest::blocking::Response, expected: &str) -> JuizResult<()> {
    assert_eq!(res.status(), reqwest::StatusCode::INTERNAL_SERVER_ERROR);
    let body: Value = res.json()?;
    let message = body["message"].as_str().expect("message");
    assert!(message.contains(expected), "{message}");
    Ok(())
}

fn check_image_response(c: &Client) -> JuizResult<()> {
    // 画像を返すプロセスの call は PNG で返る
    let res = c
        .patch(url("/api/process/call"))
        .query(&[(
            "identifier",
            "core://core/process/make0::py_bench_make_image",
        )])
        .json(&jvalue!({"seed": 1}))
        .send()?;
    let img = decode_png(res)?;
    assert_eq!(img.dimensions(), (640, 480));
    assert_eq!(*img.get_pixel(0, 0), Rgb([1, 64, 128]));
    Ok(())
}

fn check_multipart_put(c: &Client) -> JuizResult<()> {
    let inv0 = "core://core/process/inv0::py_bench_invert";
    let src = RgbImage::from_pixel(4, 3, Rgb([10, 20, 30]));
    let res = put_multipart(c, inv0, &[("img", Some("image/png"), png_bytes(&src))])?;
    let img = decode_png(res)?;
    assert_eq!(img.dimensions(), (4, 3));
    assert_eq!(*img.get_pixel(3, 2), Rgb([245, 235, 225]));

    // 扱えない Content-Type、Content-Type が無いフィールドはエラーになる
    assert_error(
        put_multipart(c, inv0, &[("img", Some("text/plain"), b"abc".to_vec())])?,
        "content-type",
    )?;
    assert_error(
        put_multipart(c, inv0, &[("img", None, b"abc".to_vec())])?,
        "content-type",
    )?;
    // multipart として読めない本体
    let res = c
        .put(url("/api/process/call"))
        .query(&[("identifier", inv0)])
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body("broken")
        .send()?;
    assert_error(res, "Multipart")?;
    Ok(())
}

fn check_errors(c: &Client) -> JuizResult<()> {
    // 無いプロセスの call は 500 と理由
    let res = c
        .patch(url("/api/process/call"))
        .query(&[("identifier", "core://core/process/nothing::py_bench_invert")])
        .json(&jvalue!({}))
        .send()?;
    assert_error(res, "nothing")?;
    let res = c
        .get(url("/api/process/profile_full"))
        .query(&[("identifier", "core://core/process/nothing::py_bench_invert")])
        .send()?;
    assert_eq!(res.status(), reqwest::StatusCode::INTERNAL_SERVER_ERROR);
    Ok(())
}

/// identifier が要る口に identifier を付けずに要求すると、どれも 500 と理由を返す
fn check_missing_identifier(c: &Client) -> JuizResult<()> {
    use reqwest::Method;
    let cases = [
        (Method::GET, "process/profile_full"),
        (Method::GET, "process/openapi_spec"),
        (Method::PATCH, "process/call"),
        (Method::PATCH, "process/execute"),
        (Method::PATCH, "process/p_apply"),
        (Method::PATCH, "process/push_by"),
        (Method::DELETE, "process/destroy"),
        (Method::POST, "container_process/create"),
        (Method::GET, "container_process/profile_full"),
        (Method::GET, "container_process/openapi_spec"),
        (Method::PATCH, "container_process/call"),
        (Method::PATCH, "container_process/execute"),
        (Method::PATCH, "container_process/p_apply"),
        (Method::DELETE, "container_process/destroy"),
        (Method::GET, "container/profile_full"),
        (Method::DELETE, "container/destroy"),
        (Method::GET, "execution_context/profile_full"),
        (Method::GET, "execution_context/get_state"),
        (Method::PATCH, "execution_context/start"),
        (Method::PATCH, "execution_context/stop"),
        (Method::DELETE, "execution_context/destroy"),
        (Method::GET, "connection/profile_full"),
        (Method::DELETE, "connection/destroy"),
    ];
    for (method, path) in cases {
        let res = c
            .request(method.clone(), url(&format!("/api/{path}")))
            .json(&jvalue!({}))
            .send()?;
        assert_eq!(
            res.status(),
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{method} {path}"
        );
        let body: Value = res.json()?;
        let message = body["message"].as_str().unwrap_or_default();
        assert!(message.contains("identifier"), "{method} {path}: {message}");
    }

    // 引数が足りない・形が違う要求もエラーになる
    for function in [
        "load_process",
        "load_container",
        "load_container_process",
        "load_component",
    ] {
        let res = c
            .patch(url(&format!("/api/system/{function}")))
            .json(&jvalue!({"language": "rust"}))
            .send()?;
        assert_eq!(
            res.status(),
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{function}"
        );
    }
    for function in ["try_connect_to", "notify_connected_from"] {
        let res = c
            .patch(url(&format!("/api/process/{function}")))
            .json(&jvalue!({"nothing": 1}))
            .send()?;
        assert_eq!(
            res.status(),
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{function}"
        );
    }
    Ok(())
}

fn check_openapi(c: &Client) -> JuizResult<()> {
    let v: Value = c.get(url("/api/system/openapi.json")).send()?.json()?;
    assert_eq!(v["openapi"], jvalue!("3.0.1"));

    // /api-docs/openapi.json には各クラスの口と、読み込んだ Factory の引数のスキーマが入る
    let v: Value = c.get(url("/api-docs/openapi.json")).send()?.json()?;
    for path in [
        "/api/system/profile_full",
        "/api/system/uuid",
        "/api/process/call",
    ] {
        assert!(
            v["paths"][path].is_object(),
            "{path} not in {:?}",
            v["paths"]
        );
    }
    assert!(v["components"]["schemas"].is_object());
    Ok(())
}

/// HTTP の system/add_mastersystem は、渡された broker_name のホストを要求元のアドレスに置き換えてから
/// 相手 (マスター) の System に自分をサブシステムとして登録してもらう
fn check_add_mastersystem(c: &Client, system: &System) -> JuizResult<()> {
    let master_manifest = jvalue!({
        "name": "http_router_test_master",
        "option": { "http_broker": { "start": true, "port": MASTER_PORT } }
    });
    System::new(master_manifest)?
        .setup()?
        .run_and_do_once(|master| {
            let res = c
                .patch(url("/api/system/add_mastersystem"))
                .json(&jvalue!({
                    "profile": {
                        "subsystem": {
                            "broker_type": "http",
                            "broker_name": format!("somewhere:{MASTER_PORT}")
                        }
                    }
                }))
                .send()?;
            assert!(res.status().is_success(), "{:?}", res.text());
            // somewhere は要求元 (127.0.0.1) に置き換わる
            let profile = system.core_broker().lock()?.system_profile_full()?;
            assert_eq!(
                profile["mastersystem"]["name"],
                jvalue!(format!("127.0.0.1:{MASTER_PORT}"))
            );
            let subs = master.core_broker().lock()?.system_profile_full()?["subsystems"].clone();
            assert_eq!(subs.as_array().map(|a| a.len()), Some(1), "{subs:?}");
            Ok(())
        })
}

#[test]
fn http_router_image_multipart_and_errors() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|system| {
            let c = client()?;
            check_image_response(&c)?;
            check_multipart_put(&c)?;
            check_errors(&c)?;
            check_missing_identifier(&c)?;
            check_openapi(&c)?;
            check_add_mastersystem(&c, system)?;
            Ok(())
        })
}
