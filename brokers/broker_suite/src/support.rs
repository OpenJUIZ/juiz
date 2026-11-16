//! 確認の関数で共通に使う小さな部品

use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

/// 引数 1 つの CapsuleMap
pub fn args(name: &str, v: i64) -> CapsuleMap {
    vec![(name, jvalue!(v))].into()
}

pub fn find_process<'a>(ids: &'a [ProcessIdentifier], name: &str) -> &'a ProcessIdentifier {
    ids.iter()
        .find(|id| id.name == name)
        .unwrap_or_else(|| panic!("process {name} not found in {ids:?}"))
}

pub fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("call failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// get_output などでカプセルが Value でない場合も考えて、整数として取り出す
pub fn try_int_of(c: &CapsulePtr) -> Option<i64> {
    c.lock_as_value(|v| v.as_i64()).ok().flatten()
}

/// 結果のカプセルに、要求の経路 (class_name / function_name など) が option として残っていない (#277)
pub fn assert_no_options(c: JuizResult<CapsulePtr>) -> CapsulePtr {
    let c = c.expect("call failed");
    let options = c.lock_as_capsule(|c| c.get_options().clone()).unwrap();
    assert!(options.is_empty(), "options should be empty: {options:?}");
    c
}

/// Value を返す BrokerProxy の API は、どのブローカーでも core と同じく `{"__value__": ..., "__option__": ...}` で
/// 包まずに返す (issue #532)。包まれていないことを確かめて、そのまま返す
pub fn not_wrapped(v: Value) -> Value {
    if let Value::Object(map) = &v {
        assert!(
            !map.contains_key("__value__") && !map.contains_key("__option__"),
            "value should not be wrapped: {v:?}"
        );
    }
    v
}

/// 名前だけを変えた識別子 (存在しない Process を指すのに使う)
pub fn renamed(id: &ProcessIdentifier, name: &str) -> ProcessIdentifier {
    let mut id = id.clone();
    id.name = name.to_owned();
    id
}

/// cond が真になるまで待つ。timeout を過ぎたら false
pub fn wait_until(
    timeout: Duration,
    mut cond: impl FnMut() -> JuizResult<bool>,
) -> JuizResult<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        if cond()? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
