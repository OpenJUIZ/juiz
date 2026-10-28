//! 埋め込んだ管理画面を HTTP ブローカーの /ui で配信するテスト (issue #117)
//!
//! webui/dist をビルドしていなくても通るよう、画面の中身ではなく URL の振る舞いを確かめる
//! (ビルドしていなければ /ui/ は案内ページになる)。

#![cfg(feature = "webui")]

use juiz_core::prelude::*;

fn system_manifest(port: i64, http_broker: Value) -> Value {
    let mut opt = http_broker;
    let map = opt.as_object_mut().unwrap();
    map.insert("start".to_owned(), jvalue!(true));
    map.insert("port".to_owned(), jvalue!(port));
    jvalue!({
        "name": "webui_test",
        "option": { "http_broker": opt }
    })
}

fn client() -> JuizResult<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

#[test]
fn webui_is_served_at_ui() -> JuizResult<()> {
    crate::common::logger::init();
    const PORT: i64 = 8016;
    System::new(system_manifest(PORT, jvalue!({})))?
        .setup()?
        .run_and_do_once(|_system| {
            let c = client()?;
            // /ui は /ui/ に付け直す(画面は相対パスで部品を読む)
            let res = c.get(format!("http://localhost:{PORT}/ui")).send()?;
            assert!(res.status().is_redirection(), "{:?}", res.status());
            assert_eq!(res.headers()["location"], "/ui/");

            let res = c.get(format!("http://localhost:{PORT}/ui/")).send()?;
            assert!(res.status().is_success(), "{:?}", res.status());
            assert!(res.headers()["content-type"]
                .to_str()?
                .starts_with("text/html"));
            assert!(res.text()?.contains("juiz 管理画面"));

            let res = c
                .get(format!("http://localhost:{PORT}/ui/no_such_file.js"))
                .send()?;
            assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);

            // API はそのまま
            let res = c
                .get(format!("http://localhost:{PORT}/api/system/profile_full"))
                .send()?;
            assert!(res.status().is_success(), "{:?}", res.status());
            Ok(())
        })
}

#[test]
fn webui_can_be_disabled() -> JuizResult<()> {
    crate::common::logger::init();
    const PORT: i64 = 8017;
    System::new(system_manifest(PORT, jvalue!({ "webui": false })))?
        .setup()?
        .run_and_do_once(|_system| {
            let res = client()?
                .get(format!("http://localhost:{PORT}/ui/"))
                .send()?;
            assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
            Ok(())
        })
}

#[test]
fn static_filepaths_overrides_webui() -> JuizResult<()> {
    crate::common::logger::init();
    const PORT: i64 = 8018;
    let dir = std::env::temp_dir().join(format!("juiz_webui_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("index.html"), "static_filepaths の画面")?;
    let manifest = system_manifest(
        PORT,
        jvalue!({ "static_filepaths": { "/ui": dir.to_str().unwrap() } }),
    );
    let result = System::new(manifest)?.setup()?.run_and_do_once(|_system| {
        let res = client()?
            .get(format!("http://localhost:{PORT}/ui/"))
            .send()?;
        assert!(res.status().is_success(), "{:?}", res.status());
        assert_eq!(res.text()?, "static_filepaths の画面");
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&dir);
    result
}
