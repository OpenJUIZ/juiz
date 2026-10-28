//! 管理画面 (webui) が使う HTTP API の応答を、JSON のまま確かめるテスト (issue #112)
//!
//! 画面は BrokerProxy を通さず HTTP で JSON を読むので、型に読み込むと消える
//! identifier などの項目もここで確かめる。
//! Rust の例が cargo build で ../target/debug にビルドされている前提。

use juiz_core::prelude::*;

const PORT: i64 = 8015;
const ALLOWED_ORIGIN: &str = "http://localhost:5173";

fn system_manifest() -> Value {
    jvalue!({
        "name": "viewer_api_test",
        "option": {
            "http_broker": {
                "start": true,
                "port": PORT,
                "cors_allow_origins": [ALLOWED_ORIGIN]
            }
        },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" }
            },
            "container_factories": {
                "example_container": {
                    "path": "../target/debug",
                    "processes": {
                        "example_container_increment": { "path": "../target/debug" },
                        "example_container_get": { "path": "../target/debug" }
                    }
                }
            }
        },
        "processes": [
            { "type_name": "increment_process", "name": "pub0", "publishes": ["topic0"] },
            { "type_name": "increment_process", "name": "sub0",
              "subscribes": { "arg1": "topic0" },
              "p_apply": { "arg1": 5 } }
        ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [
                  { "type_name": "example_container_increment", "name": "c0inc" },
                  { "type_name": "example_container_get", "name": "c0get" } ] }
        ]
    })
}

struct Api {
    client: reqwest::blocking::Client,
}

impl Api {
    fn new() -> JuizResult<Self> {
        Ok(Self {
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    fn url(path: &str) -> String {
        format!("http://localhost:{PORT}/api/{path}")
    }

    fn get(&self, path: &str) -> JuizResult<Value> {
        let res = self.client.get(Self::url(path)).send()?;
        assert!(res.status().is_success(), "{path}: {:?}", res.text());
        Ok(res.json()?)
    }

    fn profile_full(&self, class_name: &str, identifier: &str) -> JuizResult<Value> {
        let res = self
            .client
            .get(Self::url(&format!("{class_name}/profile_full")))
            .query(&[("identifier", identifier)])
            .send()?;
        assert!(
            res.status().is_success(),
            "{class_name} {identifier}: {:?}",
            res.text()
        );
        Ok(res.json()?)
    }
}

fn names(v: &Value) -> Vec<String> {
    let mut names = v
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect::<Vec<String>>();
    names.sort();
    names
}

/// profile_full に identifier と class_name が入り、そのまま profile_full に渡せる
fn check_profile_identifiers(api: &Api) -> JuizResult<()> {
    let id = "core://core/process/sub0::increment_process";
    let p = api.profile_full("process", id)?;
    assert_eq!(p["identifier"], jvalue!(id));
    assert_eq!(p["class_name"], jvalue!("process"));

    let id = "core://core/container_process/c0get::example_container_get:c0";
    let p = api.profile_full("container_process", id)?;
    assert_eq!(p["identifier"], jvalue!(id));
    assert_eq!(p["class_name"], jvalue!("container_process"));

    let id = "core://core/container/c0::example_container";
    let c = api.profile_full("container", id)?;
    assert_eq!(c["identifier"], jvalue!(id));
    assert_eq!(c["class_name"], jvalue!("container"));
    // Container の processes に、属する ContainerProcess が入る
    assert_eq!(names(&c["processes"]), vec!["c0get", "c0inc"]);
    for p in c["processes"].as_array().unwrap() {
        let pid = p["identifier"].as_str().unwrap();
        assert_eq!(
            api.profile_full("container_process", pid)?["name"],
            p["name"]
        );
    }

    // system/profile_full の中身にも入る
    let sys = api.get("system/profile_full")?;
    let store = &sys["core_store"];
    for key in ["processes", "container_processes", "containers", "topics"] {
        for p in store[key].as_array().unwrap() {
            assert!(p["identifier"].is_string(), "{key}: {p}");
            assert!(p["class_name"].is_string(), "{key}: {p}");
        }
    }
    assert_eq!(
        names(&store["containers"][0]["processes"]),
        vec!["c0get", "c0inc"]
    );
    Ok(())
}

/// 引数の今の値 (拘束値) が value に出る。値が無ければ value は無い
fn check_argument_values(api: &Api) -> JuizResult<()> {
    let arg = |p: &Value| p["arguments"][0].clone();
    let sub = api.profile_full("process", "core://core/process/sub0::increment_process")?;
    assert_eq!(arg(&sub)["value"], jvalue!(5), "conf の p_apply");
    let pub0 = api.profile_full("process", "core://core/process/pub0::increment_process")?;
    assert!(arg(&pub0).get("value").is_none(), "{pub0}");

    let id = "core://core/container_process/c0inc::example_container_increment:c0";
    let res = api
        .client
        .patch(Api::url("container_process/p_apply"))
        .query(&[("identifier", id)])
        .json(&jvalue!({"arg_name": "arg1", "value": 7}))
        .send()?;
    assert!(res.status().is_success(), "{:?}", res.text());
    assert_eq!(
        arg(&api.profile_full("container_process", id)?)["value"],
        jvalue!(7)
    );
    Ok(())
}

/// process / container / container_process / any_process の list は、どれも HTTP ブローカーを指す識別子を返す
fn check_list_broker_names(api: &Api) -> JuizResult<()> {
    for (path, count) in [
        ("process/list", 2),
        ("container/list", 1),
        ("container_process/list", 2),
        ("any_process/list", 4),
    ] {
        let ids = api.get(path)?;
        let ids = ids.as_array().unwrap();
        assert_eq!(ids.len(), count, "{path}: {ids:?}");
        for id in ids {
            assert_eq!(id["broker_type_name"], jvalue!("http"), "{path}: {id}");
            assert_eq!(
                id["broker_name"],
                jvalue!(format!("localhost:{PORT}")),
                "{path}: {id}"
            );
        }
    }
    // any_process/profile_full は Process と ContainerProcess のどちらも引ける
    let id = format!("http://localhost:{PORT}/container_process/c0get::example_container_get:c0");
    assert_eq!(
        api.profile_full("any_process", &id)?["name"],
        jvalue!("c0get")
    );
    let id = format!("http://localhost:{PORT}/process/pub0::increment_process");
    assert_eq!(
        api.profile_full("any_process", &id)?["name"],
        jvalue!("pub0")
    );
    Ok(())
}

/// 話題との接続も含めて、connection/list の識別子はすべて connection/profile_full で引ける (issue #121)
fn check_topic_connections(api: &Api) -> JuizResult<()> {
    let ids = api.get("connection/list")?;
    let ids = ids.as_array().unwrap();
    assert_eq!(ids.len(), 2, "{ids:?}");
    let mut arg_names = vec![];
    for id in ids {
        let c = api.profile_full("connection", id.as_str().unwrap())?;
        arg_names.push(c["arg_name"].as_str().unwrap().to_owned());
    }
    arg_names.sort();
    // pub0 → topic0 (話題の入力 input) と topic0 → sub0 (arg1)
    assert_eq!(arg_names, vec!["arg1", "input"]);
    Ok(())
}

/// conf で許可したオリジンにだけ CORS のヘッダを返す
fn check_cors(api: &Api) -> JuizResult<()> {
    let allow_origin = |origin: &str| -> JuizResult<Option<String>> {
        let res = api
            .client
            .get(Api::url("process/list"))
            .header("Origin", origin)
            .send()?;
        Ok(res
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap().to_owned()))
    };
    assert_eq!(
        allow_origin(ALLOWED_ORIGIN)?,
        Some(ALLOWED_ORIGIN.to_owned())
    );
    assert_eq!(allow_origin("http://example.com")?, None);

    // ブラウザが PATCH の前に送る問い合わせ (preflight) にも答える
    let res = api
        .client
        .request(reqwest::Method::OPTIONS, Api::url("process/p_apply"))
        .header("Origin", ALLOWED_ORIGIN)
        .header("Access-Control-Request-Method", "PATCH")
        .header("Access-Control-Request-Headers", "content-type")
        .send()?;
    assert!(res.status().is_success(), "{:?}", res.status());
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        ALLOWED_ORIGIN
    );
    Ok(())
}

#[test]
fn viewer_api_returns_identifiers_values_and_cors() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|_system| {
            let api = Api::new()?;
            check_profile_identifiers(&api)?;
            check_argument_values(&api)?;
            check_list_broker_names(&api)?;
            check_topic_connections(&api)?;
            check_cors(&api)?;
            Ok(())
        })
}
