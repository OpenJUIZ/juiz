//! Value 操作ユーティリティ (value.rs / utils) のテスト

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex, RwLock};

use juiz_sdk::prelude::*;
use juiz_sdk::utils::manifest_util::{construct_id, id_from_manifest, id_from_manifest_and_class_name, manifest_merge, type_name};
use juiz_sdk::utils::yaml_conf_load::yaml_conf_load_with;
use juiz_sdk::utils::{check_corebroker_manifest, check_manifest_before_call, yaml_conf_load};
use juiz_sdk::value::array_merge;
use juiz_sdk::manifests::ProcessProfile;

#[test]
fn load_str_parses_json() -> JuizResult<()> {
    assert_eq!(load_str(r#"{"a": 1}"#)?, jvalue!({"a": 1}));
    assert!(load_str("{not json").is_err());
    Ok(())
}

#[test]
fn obj_getters_return_typed_values() -> JuizResult<()> {
    let v = jvalue!({
        "s": "str", "i": 3, "f": 1.5, "b": true,
        "o": {"k": "v"}, "a": [1, 2]
    });
    assert!(as_obj(&v)?.contains_key("s"));
    assert_eq!(obj_get(&v, "i")?, &jvalue!(3));
    assert_eq!(obj_get_str(&v, "s")?, "str");
    assert_eq!(obj_get_i64(&v, "i")?, 3);
    assert_eq!(obj_get_f64(&v, "f")?, 1.5);
    // 整数も f64 として取り出せる
    assert_eq!(obj_get_f64(&v, "i")?, 3.0);
    assert!(obj_get_bool(&v, "b")?);
    assert_eq!(obj_get_obj(&v, "o")?.get("k"), Some(&jvalue!("v")));
    assert_eq!(obj_get_array(&v, "a")?.len(), 2);
    let hm = obj_get_hashmap(&v, "o")?;
    assert_eq!(hm.get("k"), Some(&jvalue!("v")));
    Ok(())
}

#[test]
fn obj_getters_report_type_and_missing_errors() {
    let v = jvalue!({"s": "str", "i": 3});
    assert!(as_obj(&jvalue!(1)).is_err());
    assert!(obj_get(&jvalue!(1), "s").is_err());
    assert!(obj_get(&v, "missing").is_err());
    assert!(obj_get_str(&v, "i").is_err());
    assert!(obj_get_i64(&v, "s").is_err());
    assert!(obj_get_f64(&v, "s").is_err());
    assert!(obj_get_bool(&v, "s").is_err());
    assert!(obj_get_obj(&v, "s").is_err());
    assert!(obj_get_array(&v, "s").is_err());
    assert!(obj_get_hashmap(&v, "s").is_err());
}

#[test]
fn obj_get_mut_modifies_in_place() -> JuizResult<()> {
    let mut v = jvalue!({"i": 1});
    *obj_get_mut(&mut v, "i")? = jvalue!(2);
    assert_eq!(v, jvalue!({"i": 2}));
    assert!(obj_get_mut(&mut v, "missing").is_err());
    assert!(obj_get_mut(&mut jvalue!([]), "i").is_err());
    Ok(())
}

#[test]
fn obj_insert_and_merge_mut() -> JuizResult<()> {
    let mut v = jvalue!({"a": 1});
    obj_insert(&mut v, "b", jvalue!(2))?;
    assert_eq!(v, jvalue!({"a": 1, "b": 2}));
    obj_merge_mut(&mut v, &jvalue!({"a": 10, "c": 3}))?;
    assert_eq!(v, jvalue!({"a": 10, "b": 2, "c": 3}));
    assert!(obj_insert(&mut jvalue!(1), "x", jvalue!(1)).is_err());
    assert!(obj_merge_mut(&mut v, &jvalue!(1)).is_err());
    Ok(())
}

#[test]
fn value_merge_merges_objects_recursively_and_concatenates_arrays() -> JuizResult<()> {
    let base = jvalue!({"obj": {"x": 1}, "arr": [1], "only_base": true});
    let data = jvalue!({"obj": {"y": 2}, "arr": [2], "only_data": "d"});
    let merged = value_merge(base, &data)?;
    assert_eq!(merged, jvalue!({
        "obj": {"x": 1, "y": 2},
        "arr": [1, 2],
        "only_base": true,
        "only_data": "d"
    }));
    assert_eq!(array_merge(jvalue!([1]), &jvalue!([2, 3]))?, jvalue!([1, 2, 3]));
    assert_eq!(obj_merge(jvalue!({"a": {}}), &jvalue!({"a": {"b": 1}}))?, jvalue!({"a": {"b": 1}}));
    Ok(())
}

#[test]
fn value_merge_rejects_mismatched_types() {
    assert!(value_merge(jvalue!({}), &jvalue!([])).is_err());
    assert!(value_merge(jvalue!(1), &jvalue!(2)).is_err());
    // 同じキーで型が異なる値もエラーになる
    assert!(obj_merge(jvalue!({"a": 1}), &jvalue!({"a": 2})).is_err());
}

#[test]
fn manifest_util_getters() -> JuizResult<()> {
    let v = jvalue!({"type_name": "t", "arr": [1], "n": 1});
    assert_eq!(get_value(&v, "n")?, &jvalue!(1));
    assert_eq!(get_str(&v, "type_name")?, "t");
    assert_eq!(type_name(&v)?, "t");
    assert_eq!(get_array(obj_get(&v, "arr")?)?.len(), 1);
    assert_eq!(get_hashmap(&v)?.len(), 3);

    assert!(get_value(&v, "missing").is_err());
    assert!(get_str(&v, "n").is_err());
    assert!(get_array(&v).is_err());
    assert!(get_hashmap(&jvalue!([])).is_err());
    assert!(get_array_mut(&mut jvalue!({})).is_err());
    assert!(get_hashmap_mut(&mut jvalue!([])).is_err());

    let mut arr = jvalue!([1]);
    get_array_mut(&mut arr)?.push(jvalue!(2));
    assert_eq!(arr, jvalue!([1, 2]));
    let mut obj = jvalue!({});
    get_hashmap_mut(&mut obj)?.insert("k".to_owned(), jvalue!(1));
    assert_eq!(obj, jvalue!({"k": 1}));
    Ok(())
}

#[test]
fn when_contains_do_runs_only_when_key_exists() -> JuizResult<()> {
    let v = jvalue!({"a": 2});
    assert_eq!(when_contains_do(&v, "a", |x| Ok(x.as_i64().unwrap() * 10))?, Some(20));
    assert_eq!(when_contains_do(&v, "b", |_| Ok(0))?, None);
    assert!(when_contains_do(&jvalue!(1), "a", |_| Ok(0)).is_err());

    let mut count = 0;
    assert_eq!(when_contains_do_mut(&v, "a", |_| { count += 1; Ok(()) })?, Some(()));
    assert_eq!(when_contains_do_mut(&v, "b", |_| { count += 1; Ok(()) })?, None);
    assert_eq!(count, 1);
    assert!(when_contains_do_mut(&jvalue!(1), "a", |_| Ok(())).is_err());
    // 関数のエラーはそのまま返る
    assert!(when_contains_do(&v, "a", |_| -> JuizResult<()> { Err(juiz_sdk::anyhow::anyhow!("boom")) }).is_err());
    Ok(())
}

#[test]
fn id_from_manifest_variants() -> JuizResult<()> {
    assert_eq!(construct_id("Process", "t", "n", "core", "core"), "core://core/process/n::t");
    assert_eq!(id_from_manifest(&jvalue!({"id": "X"}))?, "X");
    assert_eq!(id_from_manifest(&jvalue!({"identifier": "Y"}))?, "Y");
    assert_eq!(
        id_from_manifest(&jvalue!({"type_name": "t", "name": "n", "class_name": "Process"}))?,
        "core://core/process/n::t"
    );
    assert_eq!(
        id_from_manifest(&jvalue!({"type_name": "t", "name": "n", "class_name": "Process", "broker_type": "http", "broker_name": "h"}))?,
        "http://h/process/n::t"
    );
    assert!(id_from_manifest(&jvalue!({"type_name": "t", "name": "n"})).is_err());
    assert!(id_from_manifest(&jvalue!({"type_name": "t", "name": "n", "class_name": "P", "broker_type": "http"})).is_err());

    assert_eq!(id_from_manifest_and_class_name(&jvalue!({"id": "X"}), "Process")?, "X");
    assert_eq!(id_from_manifest_and_class_name(&jvalue!({"identifier": "Y"}), "Process")?, "Y");
    assert_eq!(
        id_from_manifest_and_class_name(&jvalue!({"type_name": "t", "name": "n"}), "Container")?,
        "core://core/container/n::t"
    );
    assert_eq!(
        id_from_manifest_and_class_name(&jvalue!({"type_name": "t", "name": "n", "broker_type": "http", "broker_name": "h"}), "Container")?,
        "http://h/container/n::t"
    );
    assert!(id_from_manifest_and_class_name(&jvalue!({"name": "n"}), "Container").is_err());
    Ok(())
}

#[test]
fn manifest_merge_variants() -> JuizResult<()> {
    let merged = manifest_merge(
        jvalue!({"a": 1, "nested": {"x": 1}, "list": [1]}),
        &jvalue!({"a": 2, "nested": {"y": 2}, "list": [2], "new": true}),
    )?;
    // スカラーは上書き、オブジェクトは再帰マージ、配列は連結
    assert_eq!(merged, jvalue!({"a": 2, "nested": {"x": 1, "y": 2}, "list": [1, 2], "new": true}));
    assert_eq!(manifest_merge(jvalue!(1), &jvalue!("s"))?, jvalue!("s"));
    assert!(manifest_merge(jvalue!({}), &jvalue!([])).is_err());
    assert!(manifest_merge(jvalue!([]), &jvalue!({})).is_err());
    Ok(())
}

#[test]
fn check_manifest_before_call_detects_missing_argument() -> JuizResult<()> {
    let profile = ProcessProfile::new("p0", "p").add_int_arg("arg1", "", 1).add_string_arg("arg2", "", "x");
    let ok: CapsuleMap = vec![("arg1", jvalue!(1)), ("arg2", jvalue!("y"))].into();
    assert!(check_manifest_before_call(&profile, &ok).is_ok());
    let missing: CapsuleMap = vec![("arg1", jvalue!(1))].into();
    assert!(check_manifest_before_call(&profile, &missing).is_err());

    assert_eq!(check_corebroker_manifest(jvalue!({"a": 1})).unwrap(), jvalue!({"a": 1}));
    assert_eq!(check_connection_manifest(jvalue!({"b": 2})).unwrap(), jvalue!({"b": 2}));
    Ok(())
}

fn write_temp_yaml(name: &str, body: &str) -> String {
    let dir = std::env::temp_dir().join(format!("juiz_sdk_value_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn yaml_conf_load_converts_yaml_to_value() {
    let path = write_temp_yaml("conf.yaml", r#"
name: "juiz"
port: 8000
ratio: 0.5
enabled: true
empty:
list:
  - 1
  - two
nested:
  key: value
"#);
    let v = yaml_conf_load(path).unwrap();
    assert_eq!(v, jvalue!({
        "name": "juiz",
        "port": 8000,
        "ratio": 0.5,
        "enabled": true,
        "empty": {},
        "list": [1, "two"],
        "nested": {"key": "value"}
    }));
}

#[test]
fn yaml_conf_load_returns_empty_object_on_error() {
    assert_eq!(yaml_conf_load("/nonexistent/juiz/conf.yaml".to_owned()).unwrap(), jvalue!({}));
    // 文字列以外のキーは扱えない
    let path = write_temp_yaml("bad_key.yaml", "1: one\n");
    assert_eq!(yaml_conf_load(path).unwrap(), jvalue!({}));
}

#[test]
fn yaml_conf_load_handles_empty_and_multi_document() {
    let empty = write_temp_yaml("empty.yaml", "");
    assert_eq!(yaml_conf_load(empty).unwrap(), jvalue!({}));
    let multi = write_temp_yaml("multi.yaml", "a: 1\n---\nb: 2\n");
    assert_eq!(yaml_conf_load(multi).unwrap(), jvalue!([{"a": 1}, {"b": 2}]));
}

#[test]
fn yaml_conf_load_with_replaces_placeholders() -> JuizResult<()> {
    let path = write_temp_yaml("with.yaml", "home: ${HOME_DIR}/juiz\n");
    let mut map = HashMap::new();
    map.insert("${HOME_DIR}", "/home/user".to_owned());
    assert_eq!(yaml_conf_load_with(path, map)?, jvalue!({"home": "/home/user/juiz"}));
    assert!(yaml_conf_load_with("/nonexistent/juiz.yaml".to_owned(), HashMap::new()).is_err());
    Ok(())
}

#[test]
fn sync_util_locks() -> JuizResult<()> {
    let m = Arc::new(Mutex::new(1));
    {
        let mut g = juiz_lock(&m)?;
        *g += 1;
        // ロック中は try_lock が失敗する
        assert!(juiz_try_lock(&m).is_err());
    }
    assert_eq!(*juiz_try_lock(&m)?, 2);

    let mut rw = Arc::new(RwLock::new(10));
    *juiz_borrow_mut(&mut rw)? += 5;
    assert_eq!(*juiz_borrow(&rw)?, 15);
    Ok(())
}

#[test]
fn sync_util_reports_poisoned_locks() {
    let m = Arc::new(Mutex::new(0));
    let m2 = m.clone();
    let _ = std::thread::spawn(move || {
        let _g = m2.lock().unwrap();
        panic!("poison");
    }).join();
    assert!(juiz_lock(&m).is_err());
    assert!(juiz_try_lock(&m).is_err());

    let rw = Arc::new(RwLock::new(0));
    let rw2 = rw.clone();
    let _ = std::thread::spawn(move || {
        let _g = rw2.write().unwrap();
        panic!("poison");
    }).join();
    assert!(juiz_borrow(&rw).is_err());
    let mut rw = rw;
    assert!(juiz_borrow_mut(&mut rw).is_err());
}
