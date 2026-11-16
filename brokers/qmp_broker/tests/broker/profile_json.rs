//! qmp ブローカー経由の profile の JSON が変わらないことを確かめるテスト
//! (issue #505、docs/spec/manifest-profile.md §3 の 4)
//!
//! `system/profile` の `core_store` の Process・Container・ContainerProcess の profile (qmp で届いた JSON のまま) と、
//! `*/profile` を qmp の BrokerProxy で型に読み、もう一度 JSON にしたものを、
//! Profile の型を作り替える前 (main d758bd9) の結果を写した golden/qmp_profile_json.json と比べる。
//! HTTP は juiz_core の core::profile_json。
//!
//! golden を作り直すときは、`JUIZ_PROFILE_JSON_DUMP=<出力先>` を付けてこのテストを実行する。

use crate::common::*;
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

const PORT: i64 = 5028;
const HTTP_PORT: i64 = 8099;

fn manifest() -> Value {
    let mut m = QMP.server_manifest("qmp_profile_json_test", PORT, HTTP_PORT, true);
    m["processes"][1]["p_apply"] = jvalue!({ "arg1": 5 });
    m["processes"][1]["description"] = jvalue!("p_apply 5");
    m["containers"]
        .as_array_mut()
        .unwrap()
        .push(jvalue!({ "type_name": "static_anchor", "name": "world" }));
    m
}

/// 配列を name の順に並べる (System の中の並びは決まっていないため)
fn sorted_by_name(v: &Value) -> Value {
    let mut items = v.as_array().cloned().unwrap_or_default();
    items.sort_by_key(|p| p["name"].as_str().unwrap_or_default().to_owned());
    Value::Array(items.into_iter().map(normalize).collect())
}

/// Container の profile の processes も name の順に並べる
fn normalize(mut v: Value) -> Value {
    if v.get("processes").is_some() {
        let processes = sorted_by_name(&v["processes"]);
        v["processes"] = processes;
    }
    v
}

fn json_diff(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
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
fn qmp_profile_json_is_unchanged() -> JuizResult<()> {
    let _ = juiz_core::env_logger::try_init();
    let broker_name = format!("127.0.0.1:{PORT}");
    System::new(manifest())?.setup()?.run_and_do_once(|system| {
        let proxy = system
            .core_broker()
            .lock()?
            .worker()
            .broker_proxy("qmp", broker_name.as_str(), true)?;
        let bp = juiz_borrow(&proxy)?;
        let system_profile = bp.system_profile()?;
        // どのブローカーでも包まずに返す (issue #532)
        let store = &system_profile["core_store"];
        let inc1: ProcessIdentifier = "core://core/process/inc1::increment_process".try_into()?;
        let c0inc: ProcessIdentifier =
            "core://core/container_process/c0inc::example_container_increment:c0".try_into()?;
        let c0 = ContainerIdentifier::try_from("core://core/container/c0::example_container".to_owned())?;
        let world = ContainerIdentifier::try_from("core://core/container/world::static_anchor".to_owned())?;
        let actual = jvalue!({
            "core_store": {
                "processes": sorted_by_name(&store["processes"]),
                "container_processes": sorted_by_name(&store["container_processes"]),
                "containers": sorted_by_name(&store["containers"]),
            },
            "typed": {
                "process": serde_json::to_value(bp.process_profile(&inc1)?)?,
                "container_process": serde_json::to_value(bp.container_process_profile(&c0inc)?)?,
                "container": normalize(serde_json::to_value(bp.container_profile(&c0)?)?),
                "anchor": serde_json::to_value(bp.container_profile(&world)?)?,
            },
        });
        if let Ok(out) = std::env::var("JUIZ_PROFILE_JSON_DUMP") {
            std::fs::write(&out, serde_json::to_string_pretty(&actual)?)?;
            return Ok(());
        }
        let golden: Value = serde_json::from_str(include_str!("../golden/qmp_profile_json.json"))?;
        let mut diff = Vec::new();
        json_diff("$", &golden, &actual, &mut diff);
        assert!(diff.is_empty(), "profile の JSON が変わった:\n{}", diff.join("\n"));
        Ok(())
    })
}
