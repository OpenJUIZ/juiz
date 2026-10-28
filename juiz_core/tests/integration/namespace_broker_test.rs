//! ネームスペース付きのオブジェクトを HTTP ブローカー越しに扱うテスト (issue #150)
//!
//! - サブシステム越しに、ネームスペース付きの Process / ContainerProcess を呼べる
//! - list の `?namespace=` で、そのネームスペースとその下だけに絞り込める
//! - 再帰的な system/profile_full に namespace が載る
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

/// 1 台目の juiz。2 台目は同じポートを指定して次のポート (SUB_PORT) で待ち受け、サブシステムになる
const MASTER_PORT: i64 = 8040;
const SUB_PORT: i64 = 8041;

fn master_manifest() -> Value {
    jvalue!({
        "name": "namespace_broker_master",
        "option": { "http_broker": { "start": true, "port": MASTER_PORT } }
    })
}

/// robot1 を既定にして、robot1・robot1/arm・robot2・ルートに Process を置く
fn sub_manifest() -> Value {
    jvalue!({
        "name": "namespace_broker_sub",
        "namespace": "robot1",
        "option": { "http_broker": { "start": true, "port": MASTER_PORT } },
        "plugins": {
            "process_factories": {
                "increment_process": { "path": "../target/debug" }
            },
            "container_factories": {
                "example_container": {
                    "path": "../target/debug",
                    "processes": {
                        "example_container_increment": { "path": "../target/debug" },
                    }
                }
            }
        },
        "processes": [
            { "type_name": "increment_process", "name": "inc0" },
            { "type_name": "increment_process", "name": "inc1", "namespace": "arm" },
            { "type_name": "increment_process", "name": "inc0", "namespace": "/robot2" },
            { "type_name": "increment_process", "name": "inc0", "namespace": "/" },
        ],
        "containers": [
            {
                "type_name": "example_container", "name": "container0", "initial_value": 100,
                "processes": [ { "type_name": "example_container_increment", "name": "increment0" } ]
            },
        ],
        "ecs": [
            { "type_name": "main_loop_ec", "rate": 10.0, "name": "ec0", "auto_start": false },
        ],
    })
}

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

fn find<'a>(ids: &'a [ProcessIdentifier], full_name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.full_name() == full_name)
        .unwrap_or_else(|| panic!("{full_name} not found in {ids:?}"))
}

fn client() -> JuizResult<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?)
}

/// GET /api/<class>/list?namespace=... の結果。Process などは識別子のオブジェクト、EC は識別子の文字列
fn http_list(port: i64, class_name: &str, query: &[(&str, &str)]) -> JuizResult<Vec<Value>> {
    let res = client()?
        .get(format!("http://127.0.0.1:{port}/api/{class_name}/list"))
        .query(query)
        .send()?;
    assert!(
        res.status().is_success(),
        "{class_name} {query:?}: {:?}",
        res.text()
    );
    let v = res.json::<Value>()?;
    Ok(v.as_array()
        .unwrap_or_else(|| panic!("{class_name} list is not an array: {v}"))
        .clone())
}

/// 一覧の各要素の `<namespace>/<name>` (並べ替え済み)
fn full_names(ids: &[Value]) -> Vec<String> {
    let mut names: Vec<String> = ids
        .iter()
        .map(|v| match v.as_str() {
            Some(id) => {
                let s: IdentifierStruct = id.to_owned().try_into().expect("invalid identifier");
                s.full_name()
            }
            None => {
                let id: ProcessIdentifier =
                    serde_json::from_value(v.clone()).expect("invalid identifier");
                id.full_name()
            }
        })
        .collect();
    names.sort();
    names
}

fn check_call_over_subsystem(master: &System) -> JuizResult<()> {
    let ids = master.core_broker().lock()?.process_list(true, None)?;
    // サブシステムの Process は http://<住所>/process/<namespace>/<name>::<type> で見える
    let r1 = find(&ids, "robot1/inc0");
    assert_eq!(r1.broker_type_name, "http", "{r1:?}");
    assert_eq!(r1.namespace, "robot1");
    assert!(
        r1.to_string()
            .ends_with("/process/robot1/inc0::increment_process"),
        "{r1}"
    );
    let arm = find(&ids, "robot1/arm/inc1");
    let r2 = find(&ids, "robot2/inc0");
    let root = find(&ids, "inc0");
    assert_eq!(root.namespace, "");

    let core = master.core_broker();
    assert_eq!(int_of(core.lock()?.process_call(r1, args("arg1", 10))), 11);
    assert_eq!(int_of(core.lock()?.process_call(arm, args("arg1", 20))), 21);
    assert_eq!(int_of(core.lock()?.process_call(r2, args("arg1", 30))), 31);
    assert_eq!(
        int_of(core.lock()?.process_call(root, args("arg1", 40))),
        41
    );

    // ContainerProcess も所属 Container のネームスペースで呼べる
    let cps = core.lock()?.container_process_list(true, None)?;
    let inc = find(&cps, "robot1/increment0");
    assert_eq!(
        int_of(core.lock()?.container_process_call(inc, args("arg1", 1))),
        101
    );
    assert_eq!(
        int_of(core.lock()?.any_process_call(inc, args("arg1", 2))),
        103
    );

    // 一覧が返す http:// の識別子をそのまま 2 台目に渡しても、自分宛てとして呼べる
    for id in [r1, arm, r2] {
        let res = client()?
            .patch(format!("http://127.0.0.1:{SUB_PORT}/api/process/call"))
            .query(&[("identifier", id.to_string().as_str())])
            .json(&jvalue!({"arg1": 1}))
            .send()?;
        assert!(res.status().is_success(), "{id}: {:?}", res.text());
        assert_eq!(res.json::<Value>()?, jvalue!(2), "{id}");
    }
    Ok(())
}

fn check_list_filter() -> JuizResult<()> {
    let names = |class_name: &str, namespace: &str| -> JuizResult<Vec<String>> {
        Ok(full_names(&http_list(
            SUB_PORT,
            class_name,
            &[("namespace", namespace)],
        )?))
    };
    assert_eq!(
        names("process", "robot1")?,
        vec!["robot1/arm/inc1", "robot1/inc0"]
    );
    // 先頭の / は付けても同じ
    assert_eq!(
        names("process", "/robot1")?,
        vec!["robot1/arm/inc1", "robot1/inc0"]
    );
    assert_eq!(names("process", "robot1/arm")?, vec!["robot1/arm/inc1"]);
    assert_eq!(names("process", "robot2")?, vec!["robot2/inc0"]);
    // 要素単位で比べる (robot は robot1 の前方一致にならない)
    assert!(names("process", "robot")?.is_empty());
    // 空・ルートなら絞り込まない
    assert_eq!(names("process", "")?.len(), 4);
    assert_eq!(names("process", "/")?.len(), 4);
    assert_eq!(http_list(SUB_PORT, "process", &[])?.len(), 4);

    assert_eq!(names("container", "robot1")?, vec!["robot1/container0"]);
    assert!(names("container", "robot2")?.is_empty());
    assert_eq!(
        names("container_process", "robot1")?,
        vec!["robot1/increment0"]
    );
    assert!(names("container_process", "robot2")?.is_empty());
    assert_eq!(
        names("any_process", "robot1")?,
        vec!["robot1/arm/inc1", "robot1/inc0", "robot1/increment0"]
    );
    assert_eq!(names("execution_context", "robot1")?, vec!["robot1/ec0"]);
    assert!(names("execution_context", "robot2")?.is_empty());

    // 不正なネームスペースはエラー
    let res = client()?
        .get(format!("http://127.0.0.1:{SUB_PORT}/api/process/list"))
        .query(&[("namespace", "a|b")])
        .send()?;
    assert!(res.status().is_server_error(), "{:?}", res.text());

    // 1 台目からの再帰的な一覧でも、サブシステムの中身を絞り込める
    let ids = http_list(
        MASTER_PORT,
        "process",
        &[("recursive", "true"), ("namespace", "robot2")],
    )?;
    assert_eq!(full_names(&ids), vec!["robot2/inc0"]);
    assert_eq!(ids[0]["broker_type_name"], jvalue!("http"), "{ids:?}");
    Ok(())
}

/// 管理画面向けの再帰的な system/profile_full で、サブシステムの中身に namespace が載る
fn check_recursive_profile(master: &System) -> JuizResult<()> {
    let profile = client()?
        .get(format!(
            "http://127.0.0.1:{MASTER_PORT}/api/system/profile_full"
        ))
        .query(&[("recursive", "true")])
        .send()?
        .json::<Value>()?;
    let ss = profile["subsystems"]
        .as_array()
        .unwrap_or_else(|| panic!("subsystems must be array: {profile}"));
    assert_eq!(ss.len(), 1);
    let store = &ss[0]["core_store"];
    let processes = store["processes"].as_array().expect("processes");
    let mut ns: Vec<(String, String)> = processes
        .iter()
        .map(|p| {
            (
                p["namespace"].as_str().expect("namespace").to_owned(),
                p["name"].as_str().expect("name").to_owned(),
            )
        })
        .collect();
    ns.sort();
    assert_eq!(
        ns,
        vec![
            ("".to_owned(), "inc0".to_owned()),
            ("robot1".to_owned(), "inc0".to_owned()),
            ("robot1/arm".to_owned(), "inc1".to_owned()),
            ("robot2".to_owned(), "inc0".to_owned()),
        ],
        "{store}"
    );
    assert_eq!(
        store["containers"][0]["namespace"],
        jvalue!("robot1"),
        "{store}"
    );
    assert_eq!(
        store["container_processes"][0]["namespace"],
        jvalue!("robot1"),
        "{store}"
    );
    // ecs は ID をキーにしたオブジェクト
    assert_eq!(
        store["ecs"]["core://core/execution_context/robot1/ec0::main_loop_ec"]["namespace"],
        jvalue!("robot1"),
        "{store}"
    );
    // 同じ内容は CoreBroker から直接でも取れる
    let direct = master
        .core_broker()
        .lock()?
        .system_profile_full_recursive()?;
    assert_eq!(
        direct["subsystems"][0]["core_store"]["processes"]
            .as_array()
            .map(|a| a.len()),
        Some(4)
    );
    Ok(())
}

#[cfg(test)]
#[test]
fn test_namespaced_objects_over_http_subsystem() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(master_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            System::new(sub_manifest())?
                .setup()?
                .run_and_do_once(|_sub| {
                    check_call_over_subsystem(master)?;
                    check_list_filter()?;
                    check_recursive_profile(master)?;
                    Ok(())
                })
        })
}
