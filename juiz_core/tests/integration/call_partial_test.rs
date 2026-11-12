//! 一部の引数だけで呼び出す call_partial (issue #83)
extern crate juiz_core;
use juiz_core::prelude::*;

use crate::common;

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.unwrap().lock_as_value(|v| v.as_i64().unwrap()).unwrap()
}

fn args(pairs: &[(&str, i64)]) -> CapsuleMap {
    pairs
        .iter()
        .map(|(k, v)| (*k, jvalue!(*v)))
        .collect::<Vec<(&str, Value)>>()
        .into()
}

/// 引数ごとの今の値 (profile の value)
fn bound_value(p: &ProcessPtr, name: &str) -> JuizResult<Option<Value>> {
    Ok(p.lock()?
        .profile()?
        .arguments
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| a.value.clone()))
}

/// 渡さなかった引数は p_apply 済みの値で、それも無ければ既定値で補う
#[test]
fn call_partial_fills_from_p_apply_and_default_test() -> JuizResult<()> {
    let p = ProcessPtr::new(common::new_add_process("add0")?);

    // 何も拘束していなければ既定値 (arg1 = 1, arg2 = 1)
    assert_eq!(int_of(p.lock()?.call_partial(args(&[("arg1", 5)]))), 6);

    p.lock_mut()?.p_apply("arg2", jvalue!(10).into())?;
    assert_eq!(int_of(p.lock()?.call_partial(args(&[("arg1", 2)]))), 12);
    // 何も渡さなければ arg1 は既定値、arg2 は拘束値
    assert_eq!(int_of(p.lock()?.call_partial(CapsuleMap::new())), 11);
    // 渡した値が拘束値より優先される
    assert_eq!(
        int_of(p.lock()?.call_partial(args(&[("arg1", 2), ("arg2", 3)]))),
        5
    );

    // 渡した引数で inlet のバッファを書き換えない
    assert_eq!(bound_value(&p, "arg1")?, None);
    assert_eq!(bound_value(&p, "arg2")?, Some(jvalue!(10)));

    // call は今までどおり拘束値を使わない (既定値で補う、issue #265)
    assert_eq!(int_of(p.lock()?.call(args(&[("arg1", 2)]))), 3);
    // execute は拘束値と既定値で計算する
    assert_eq!(int_of(p.lock()?.execute()), 11);
    Ok(())
}

/// 結果は outlet に書かず、下流へも push しない
#[test]
fn call_partial_does_not_push_downstream_test() -> JuizResult<()> {
    let p = ProcessPtr::new(common::new_add_process("add1")?);
    let down = ProcessPtr::new(common::new_increment_process("down1")?);
    let manifest = ConnectionManifest::new(
        ConnectionType::Push,
        p.identifier().clone(),
        "arg1".to_owned(),
        down.identifier().clone(),
    );
    connect(p.clone(), down.clone(), &manifest)?;

    assert_eq!(int_of(p.lock()?.call_partial(args(&[("arg1", 100)]))), 101);
    assert!(p.lock()?.get_output().is_empty()?, "outlet must stay empty");
    assert!(
        down.lock()?.get_output().is_empty()?,
        "downstream must not run"
    );
    assert_eq!(bound_value(&down, "arg1")?, None);

    // execute なら下流へ流れる
    assert_eq!(int_of(p.lock()?.execute()), 2);
    assert_eq!(bound_value(&down, "arg1")?, Some(jvalue!(2)));
    Ok(())
}

/// Pull 型の接続は pull せず、バッファにある値 (最後に集めた値) を使う
#[test]
fn call_partial_does_not_pull_test() -> JuizResult<()> {
    let up = ProcessPtr::new(common::new_increment_process("up2")?);
    let p = ProcessPtr::new(common::new_add_process("add2")?);
    let manifest = ConnectionManifest::new(
        ConnectionType::Pull,
        up.identifier().clone(),
        "arg1".to_owned(),
        p.identifier().clone(),
    );
    connect(up.clone(), p.clone(), &manifest)?;

    // まだ何も集めていないので arg1 は既定値 1
    assert_eq!(int_of(p.lock()?.call_partial(args(&[("arg2", 10)]))), 11);
    assert!(
        up.lock()?.get_output().is_empty()?,
        "upstream must not be pulled"
    );

    // invoke で pull すると、上流の 1+1=2 がバッファに入る
    assert_eq!(int_of(p.lock()?.invoke()), 3);
    up.lock_mut()?.p_apply("arg1", jvalue!(5).into())?;
    // 上流の値が変わっても pull しないので、集めた 2 を使う
    assert_eq!(int_of(p.lock()?.call_partial(args(&[("arg2", 10)]))), 12);
    Ok(())
}
