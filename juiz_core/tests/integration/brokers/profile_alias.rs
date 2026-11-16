//! API の `*/profile` と古い名前 `*/profile_full` のテスト (issue #508、docs/spec/manifest-profile.md §7)
//!
//! - 新しい juiz は `*/profile` と `*/profile_full` のどちらにも同じ応答を返す
//! - 無い関数は 404 を返す
//! - OpenAPI には `*/profile` と、`deprecated: true` を付けた `*/profile_full` が載る
//! - BrokerProxy は `*/profile` を呼び、古い juiz (`*/profile` が無い) なら `*/profile_full` を呼び直し、
//!   以後その相手には `*/profile_full` を呼ぶ

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use juiz_core::prelude::*;
use reqwest::blocking::Client;

use crate::common;

const PORT: i64 = 8032;
const OLD_PORT: u16 = 8033;

fn url(path: &str) -> String {
    format!("http://localhost:{PORT}/api/{path}")
}

fn get(c: &Client, path: &str, identifier: Option<&str>) -> JuizResult<Value> {
    let mut req = c.get(url(path));
    if let Some(id) = identifier {
        req = req.query(&[("identifier", id)]);
    }
    let res = req.send()?;
    assert_eq!(res.status(), reqwest::StatusCode::OK, "{path}");
    Ok(res.json()?)
}

#[test]
fn profile_and_profile_full_return_the_same() -> JuizResult<()> {
    common::logger::init();
    let manifest = jvalue!({
        "name": "profile_alias_test",
        "option": { "http_broker": { "start": true, "port": PORT } },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" }
            }
        },
        "processes": [ { "type_name": "increment_process", "name": "inc0" } ]
    });
    System::new(manifest)?.setup()?.run_and_do_once(|_system| {
        let c = Client::new();
        let id = "core://core/process/inc0::increment_process";
        let system = get(&c, "system/profile", None)?;
        assert!(system["core_store"].is_object(), "{system:?}");
        // system には時刻などで変わる所が無いので、まるごと同じ
        assert_eq!(get(&c, "system/profile_full", None)?, system);
        let process = get(&c, "process/profile", Some(id))?;
        assert_eq!(process["name"], jvalue!("inc0"));
        assert_eq!(get(&c, "process/profile_full", Some(id))?, process);
        assert_eq!(get(&c, "any_process/profile", Some(id))?, process);
        assert_eq!(get(&c, "any_process/profile_full", Some(id))?, process);

        // 無い関数は 404
        let res = c.get(url("process/no_such_function")).send()?;
        assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);

        // OpenAPI の profile_full は deprecated
        let api: Value = c
            .get(format!("http://localhost:{PORT}/api-docs/openapi.json"))
            .send()?
            .json()?;
        for class_name in [
            "system",
            "process",
            "container",
            "container_process",
            "any_process",
            "connection",
            "execution_context",
            "broker",
            "composite",
            "transform",
            "pose_listener",
        ] {
            let new = &api["paths"][format!("/api/{class_name}/profile")]["get"];
            let old = &api["paths"][format!("/api/{class_name}/profile_full")]["get"];
            assert!(new.is_object(), "{class_name}/profile is not in OpenAPI");
            assert_ne!(new["deprecated"], jvalue!(true), "{class_name}/profile");
            assert_eq!(old["deprecated"], jvalue!(true), "{class_name}/profile_full");
            assert_eq!(old["responses"], new["responses"], "{class_name}");
        }
        Ok(())
    })
}

/// `*/profile` が無い古い juiz の HTTP ブローカーのふり。
/// `profile` には古い juiz と同じ 500 とエラーの文を、`profile_full` には `{"name": "old"}` を返す。
/// 受けた要求のパスを覚えておく
fn start_old_juiz() -> JuizResult<Arc<Mutex<Vec<String>>>> {
    let listener = TcpListener::bind(("127.0.0.1", OLD_PORT))?;
    let paths = Arc::new(Mutex::new(Vec::new()));
    let paths2 = paths.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut line = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            // ヘッダを読み捨てる
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
            let path = path.split('?').next().unwrap_or("").to_owned();
            paths2.lock().unwrap().push(path.clone());
            let (status, body) = if path.ends_with("/profile_full") {
                ("200 OK", r#"{"name":"old"}"#.to_owned())
            } else {
                (
                    "500 Internal Server Error",
                    r#"{"message":"Internal Server Error:  CRUDBroker can not detect function (profile) in class_name (system), CRUDBroker can not detect function (profile) in class_name (system)"}"#.to_owned(),
                )
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    Ok(paths)
}

#[test]
fn broker_proxy_falls_back_to_profile_full() -> JuizResult<()> {
    common::logger::init();
    let paths = start_old_juiz()?;
    System::new(jvalue!({ "name": "profile_alias_client" }))?
        .setup()?
        .run_and_do_once(|system| {
            let bp = system.core_broker().lock()?.worker().broker_proxy(
                "http",
                format!("127.0.0.1:{OLD_PORT}").as_str(),
                true,
            )?;
            let p = bp.read().unwrap().system_profile_recursive()?;
            assert_eq!(p["name"], jvalue!("old"));
            // 2 回目からは、はじめから profile_full を呼ぶ
            let p = bp.read().unwrap().system_profile_recursive()?;
            assert_eq!(p["name"], jvalue!("old"));
            let system_paths: Vec<String> = paths
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.starts_with("/api/system/profile"))
                .cloned()
                .collect();
            assert_eq!(
                system_paths,
                vec![
                    "/api/system/profile",
                    "/api/system/profile_full",
                    "/api/system/profile_full"
                ]
            );
            Ok(())
        })
}
