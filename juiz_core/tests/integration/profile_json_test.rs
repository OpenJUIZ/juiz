//! profile の JSON の形が変わらないことを確かめるテスト (issue #505、docs/spec/manifest-profile.md §3 の 4)
//!
//! Profile の型を「Manifest + 実行時の状態」に作り替えても、API の応答の JSON は
//! 他の版の juiz・pyjuiz・管理画面とつながるために今のまま保たなければならない。
//! HTTP の `*/profile` と `system/profile` の `core_store` の応答を、
//! 作り替える前 (main d758bd9) の応答を写した golden/profile_json.json と比べる。
//! qmp は juiz_qmp_broker の qmp_profile_json_test で同じことを確かめる。
//!
//! golden を作り直すときは、`JUIZ_PROFILE_JSON_DUMP=<出力先>` を付けてこのテストを実行する。
//! Rust の例が cargo build で ../target/debug にビルドされている前提。

use juiz_core::prelude::*;

const PORT: i64 = 8098;

fn system_manifest() -> Value {
    jvalue!({
        "name": "profile_json_test",
        "option": {
            "http_broker": { "start": true, "port": PORT }
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
              "description": "subscriber",
              "subscribes": { "arg1": "topic0" },
              "p_apply": { "arg1": 5 } }
        ],
        "containers": [
            { "type_name": "example_container", "name": "c0",
              "processes": [
                  { "type_name": "example_container_increment", "name": "c0inc", "p_apply": { "arg1": 3 } },
                  { "type_name": "example_container_get", "name": "c0get" } ] },
            { "type_name": "static_anchor", "name": "world" }
        ]
    })
}

fn get(client: &reqwest::blocking::Client, path: &str, identifier: Option<&str>) -> JuizResult<Value> {
    let mut req = client.get(format!("http://localhost:{PORT}/api/{path}"));
    if let Some(id) = identifier {
        req = req.query(&[("identifier", id)]);
    }
    let res = req.send()?;
    assert!(res.status().is_success(), "{path}: {:?}", res.text());
    Ok(res.json()?)
}

/// 配列を name の順に並べる (System の中の並びは決まっていないため)
fn sorted_by_name(v: &Value) -> Value {
    let mut items = v.as_array().cloned().unwrap_or_default();
    items.sort_by_key(|p| p["name"].as_str().unwrap_or_default().to_owned());
    Value::Array(items)
}

/// Container の profile の processes も name の順に並べる
fn normalize_container(mut c: Value) -> Value {
    let processes = sorted_by_name(&c["processes"]);
    c["processes"] = processes;
    c
}

fn collect(client: &reqwest::blocking::Client) -> JuizResult<Value> {
    let system = get(client, "system/profile", None)?;
    let store = &system["core_store"];
    Ok(jvalue!({
        "process": get(client, "process/profile", Some("core://core/process/sub0::increment_process"))?,
        "container_process": get(
            client,
            "container_process/profile",
            Some("core://core/container_process/c0inc::example_container_increment:c0"),
        )?,
        "container": normalize_container(get(
            client,
            "container/profile",
            Some("core://core/container/c0::example_container"),
        )?),
        "anchor": get(client, "container/profile", Some("core://core/container/world::static_anchor"))?,
        "core_store": {
            "processes": sorted_by_name(&store["processes"]),
            "container_processes": sorted_by_name(&store["container_processes"]),
            "containers": Value::Array(
                sorted_by_name(&store["containers"])
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(normalize_container)
                    .collect()
            ),
        },
    }))
}

/// 2 つの JSON の違いを、場所つきで並べる
pub(crate) fn json_diff(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (k, ev) in e.iter() {
                match a.get(k) {
                    Some(av) => json_diff(&format!("{path}.{k}"), ev, av, out),
                    None => out.push(format!("{path}.{k}: missing (expected {ev})")),
                }
            }
            for k in a.keys().filter(|k| !e.contains_key(*k)) {
                out.push(format!("{path}.{k}: unexpected key ({})", a[k]));
            }
        }
        (Value::Array(e), Value::Array(a)) if e.len() == a.len() => {
            for (i, (ev, av)) in e.iter().zip(a.iter()).enumerate() {
                json_diff(&format!("{path}[{i}]"), ev, av, out);
            }
        }
        _ => {
            if expected != actual {
                out.push(format!("{path}: expected {expected}, actual {actual}"));
            }
        }
    }
}

#[test]
fn profile_json_is_unchanged() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|_system| {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?;
            let actual = collect(&client)?;
            if let Ok(out) = std::env::var("JUIZ_PROFILE_JSON_DUMP") {
                std::fs::write(&out, serde_json::to_string_pretty(&actual)?)?;
                return Ok(());
            }
            let golden: Value = serde_json::from_str(include_str!("golden/profile_json.json"))?;
            let mut diff = Vec::new();
            json_diff("$", &golden, &actual, &mut diff);
            assert!(diff.is_empty(), "profile の JSON が変わった:\n{}", diff.join("\n"));
            Ok(())
        })
}
