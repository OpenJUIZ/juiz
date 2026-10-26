//! Container を含まない Composite (issue #167) のテスト
//!
//! 仕様は docs/spec/composition.md の §5 (評価)、§6 (エラーの位置)。
//! conf を使わずに、コードから Composite の Factory を作って動かす。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use juiz_core::prelude::*;
use juiz_core::{anyhow, ExecutionContextCore};
use juiz_sdk::manifests::{
    CompositeArgument, CompositeManifest, CompositeNode, ExposeSpec,
    COMPOSITE_PROCESS_FACTORY_NAME, MIXED_LANGUAGE,
};
use juiz_sdk::processes::ProcessBodyFunctionType;

fn int_arg(args: &CapsuleMap, name: &str) -> JuizResult<i64> {
    args.get(name)?.lock_as_value(|v| v.as_i64().unwrap())
}

fn increment(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(int_arg(&args, "arg1")? + 1).into())
}

fn double(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(int_arg(&args, "arg1")? * 2).into())
}

fn sub(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(int_arg(&args, "a")? - int_arg(&args, "b")?).into())
}

fn fail_if_negative(args: CapsuleMap) -> JuizResult<Capsule> {
    let v = int_arg(&args, "arg1")?;
    if v < 0 {
        return Err(anyhow::anyhow!(JuizError::InvalidArgumentError {
            message: format!("negative value {v}"),
        }));
    }
    Ok(jvalue!(v).into())
}

/// 評価順を記録する (テストごとに別のログを使う)
static ORDER_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static STOP_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn record(
    log: &Mutex<Vec<&'static str>>,
    tag: &'static str,
    args: &CapsuleMap,
) -> JuizResult<Capsule> {
    log.lock().unwrap().push(tag);
    Ok(jvalue!(int_arg(args, "arg1")?).into())
}

fn order_a(args: CapsuleMap) -> JuizResult<Capsule> {
    record(&ORDER_LOG, "a", &args)
}
fn order_b(args: CapsuleMap) -> JuizResult<Capsule> {
    record(&ORDER_LOG, "b", &args)
}
fn order_c(args: CapsuleMap) -> JuizResult<Capsule> {
    record(&ORDER_LOG, "c", &args)
}
fn order_root(args: CapsuleMap) -> JuizResult<Capsule> {
    ORDER_LOG.lock().unwrap().push("root");
    sub(args)
}
fn stop_after(args: CapsuleMap) -> JuizResult<Capsule> {
    record(&STOP_LOG, "after", &args)
}

fn one_arg(type_name: &str) -> ProcessManifest {
    ProcessManifest::new(type_name).add_int_arg("arg1", "input", 1)
}

/// テスト用の Factory の集まり。lookup に使う
struct Factories(HashMap<String, ProcessFactoryPtr>);

impl Factories {
    fn new() -> JuizResult<Self> {
        let mut f = Factories(HashMap::new());
        f.add(one_arg("increment"), increment)?;
        f.add(one_arg("double"), double)?;
        f.add(
            ProcessManifest::new("sub")
                .add_int_arg("a", "minuend", 10)
                .add_int_arg("b", "subtrahend", 3),
            sub,
        )?;
        f.add(one_arg("fail_if_negative"), fail_if_negative)?;
        f.add(one_arg("order_a"), order_a)?;
        f.add(one_arg("order_b"), order_b)?;
        f.add(one_arg("order_c"), order_c)?;
        f.add(
            ProcessManifest::new("order_root")
                .add_int_arg("a", "minuend", 0)
                .add_int_arg("b", "subtrahend", 0),
            order_root,
        )?;
        f.add(one_arg("stop_after"), stop_after)?;
        Ok(f)
    }

    fn add(
        &mut self,
        manifest: ProcessManifest,
        function: ProcessBodyFunctionType,
    ) -> JuizResult<()> {
        let type_name = manifest.type_name.clone();
        self.0
            .insert(type_name, process_factory_create(manifest, function)?);
        Ok(())
    }

    fn composite(&mut self, composite: &CompositeManifest) -> JuizResult<ProcessFactoryPtr> {
        let pf = composite_process_factory_create(composite, |name| self.0.get(name).cloned())?;
        self.0.insert(composite.type_name.clone(), pf.clone());
        Ok(pf)
    }
}

fn node(process: &str) -> CompositeNode {
    CompositeNode::new(process)
}

fn instance(pf: &ProcessFactoryPtr, name: &str) -> JuizResult<ProcessPtr> {
    pf.lock()?
        .create_process(&ProcessManifest::new(pf.type_name()).name(name))
}

fn call(p: &ProcessPtr, args: Vec<(&str, Value)>) -> JuizResult<i64> {
    p.lock()?
        .call(args.into())?
        .lock_as_value(|v| v.as_i64().unwrap())
}

/// sub(a = increment(arg1 = x), b = double(arg1 = y))
fn two_stage() -> CompositeManifest {
    CompositeManifest::new(
        "inc_minus_double",
        node("sub")
            .arg_node(
                "a",
                node("increment").arg_expose("arg1", ExposeSpec::new("x")),
            )
            .arg_node("b", node("double").arg_expose("arg1", ExposeSpec::new("y"))),
    )
}

#[test]
fn factory_body_returns_child_function() -> JuizResult<()> {
    let f = Factories::new()?;
    let pf = f.0.get("double").unwrap();
    let body = pf.lock()?.body()?;
    let result = body(vec![("arg1", jvalue!(21))].into())?;
    assert_eq!(result.as_value().and_then(|v| v.as_i64()), Some(42));
    assert_eq!(pf.lock()?.manifest()?.type_name, "double");
    Ok(())
}

#[test]
fn composite_manifest_is_built_from_children() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&two_stage())?;
    let m = pf.lock()?.manifest()?;
    let names: Vec<&str> = m.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["x", "y"]);
    assert_eq!(m.factory, COMPOSITE_PROCESS_FACTORY_NAME);
    assert_eq!(m.language, "rust");
    assert!(m.composition.is_some());

    let prof = pf.lock()?.profile_full()?;
    assert_eq!(prof["composition"]["body"]["process"], jvalue!("sub"));

    let p = instance(&pf, "c0")?;
    let profile = p.lock()?.profile()?;
    assert_eq!(profile.type_name, "inc_minus_double");
    assert_eq!(profile.factory, COMPOSITE_PROCESS_FACTORY_NAME);
    assert!(profile.composition.is_some());
    Ok(())
}

#[test]
fn composite_call_evaluates_two_stages() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&two_stage())?;
    let p = instance(&pf, "c0")?;
    // (5 + 1) - (2 * 2)
    assert_eq!(call(&p, vec![("x", jvalue!(5)), ("y", jvalue!(2))])?, 2);
    // 引数が足りなければ子を呼ぶ前にエラー
    assert!(p.lock()?.call(vec![("x", jvalue!(5))].into()).is_err());
    Ok(())
}

#[test]
fn composite_call_evaluates_three_stages_with_const() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // double(increment(increment(arg1))) - 4
    let pf = f.composite(&CompositeManifest::new(
        "three_stages",
        node("sub")
            .arg_node(
                "a",
                node("double").arg_node(
                    "arg1",
                    node("increment").arg_node("arg1", node("increment")),
                ),
            )
            .arg_const("b", jvalue!(4)),
    ))?;
    let m = pf.lock()?.manifest()?;
    // 書かなかった引数は元の名前で公開、const は公開しない
    let names: Vec<&str> = m.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["arg1"]);
    let p = instance(&pf, "c0")?;
    assert_eq!(call(&p, vec![("arg1", jvalue!(3))])?, 6);
    Ok(())
}

#[test]
fn composite_can_be_nested() -> JuizResult<()> {
    let mut f = Factories::new()?;
    f.composite(&two_stage())?;
    // double(inc_minus_double(x, y))
    let pf = f.composite(&CompositeManifest::new(
        "nested",
        node("double").arg_node("arg1", node("inc_minus_double")),
    ))?;
    let p = instance(&pf, "n0")?;
    assert_eq!(call(&p, vec![("x", jvalue!(5)), ("y", jvalue!(2))])?, 4);
    Ok(())
}

#[test]
fn composite_evaluates_depth_first_in_argument_order() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // order_root(a = order_a(order_c(x)), b = order_b(y))。conf の書き順に関係なく、子の引数の宣言順
    let pf = f.composite(&CompositeManifest::new(
        "ordered",
        node("order_root")
            .arg_node(
                "b",
                node("order_b").arg_expose("arg1", ExposeSpec::new("y")),
            )
            .arg_node(
                "a",
                node("order_a").arg_node(
                    "arg1",
                    node("order_c").arg_expose("arg1", ExposeSpec::new("x")),
                ),
            ),
    ))?;
    let p = instance(&pf, "o0")?;
    assert_eq!(call(&p, vec![("x", jvalue!(7)), ("y", jvalue!(3))])?, 4);
    assert_eq!(*ORDER_LOG.lock().unwrap(), vec!["c", "a", "b", "root"]);
    Ok(())
}

#[test]
fn composite_error_has_node_path_and_stops() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // sub(a = increment(fail_if_negative(x)), b = stop_after(y))
    let pf = f.composite(&CompositeManifest::new(
        "fails",
        node("sub")
            .arg_node(
                "a",
                node("increment").arg_node(
                    "arg1",
                    node("fail_if_negative").arg_expose("arg1", ExposeSpec::new("x")),
                ),
            )
            .arg_node(
                "b",
                node("stop_after").arg_expose("arg1", ExposeSpec::new("y")),
            ),
    ))?;
    let p = instance(&pf, "f0")?;
    assert_eq!(call(&p, vec![("x", jvalue!(1)), ("y", jvalue!(1))])?, 1);
    assert_eq!(STOP_LOG.lock().unwrap().len(), 1);

    let err = p
        .lock()?
        .call(vec![("x", jvalue!(-1)), ("y", jvalue!(1))].into())
        .expect_err("負の値でエラーにならない");
    match err.downcast_ref::<JuizError>() {
        Some(JuizError::CompositeNodeError {
            type_name,
            path,
            child_type_name,
            error,
        }) => {
            assert_eq!(type_name, "fails");
            assert_eq!(path, "body.a.arg1");
            assert_eq!(child_type_name, "fail_if_negative");
            assert!(error.contains("negative value -1"), "{error}");
        }
        other => panic!("CompositeNodeError ではない: {other:?}"),
    }
    let message = err.to_string();
    assert!(
        message.starts_with("Composite(fails) failed at body.a.arg1 (fail_if_negative)"),
        "{message}"
    );
    // 元のエラーは context の下に残る
    assert!(matches!(
        err.root_cause().downcast_ref::<JuizError>(),
        Some(JuizError::InvalidArgumentError { .. })
    ));
    // エラーより後のノード (b) は呼ばない
    assert_eq!(STOP_LOG.lock().unwrap().len(), 1);
    Ok(())
}

#[test]
fn nested_composite_error_keeps_both_paths() -> JuizResult<()> {
    let mut f = Factories::new()?;
    f.composite(&CompositeManifest::new(
        "inner",
        node("increment").arg_node("arg1", node("fail_if_negative")),
    ))?;
    let pf = f.composite(&CompositeManifest::new(
        "outer",
        node("double").arg_node("arg1", node("inner")),
    ))?;
    let p = instance(&pf, "o0")?;
    let message = p
        .lock()?
        .call(vec![("arg1", jvalue!(-5))].into())
        .expect_err("負の値でエラーにならない")
        .to_string();
    assert!(
        message.starts_with(
            "Composite(outer) failed at body.arg1 (inner): Composite(inner) failed at body.arg1 (fail_if_negative)"
        ),
        "{message}"
    );
    Ok(())
}

#[test]
fn composite_definition_errors() -> JuizResult<()> {
    let mut f = Factories::new()?;
    // 子が見つからない
    let err = f
        .composite(&CompositeManifest::new("missing", node("no_such_process")))
        .err()
        .expect("見つからない子でエラーにならない");
    assert!(matches!(
        err.downcast_ref::<JuizError>(),
        Some(JuizError::CompositeManifestInvalidError { .. })
    ));
    // 自分自身を参照する
    assert!(f
        .composite(&CompositeManifest::new("selfref", node("selfref")))
        .is_err());
    // スロットを持つ Composite は C3 (#168) まで扱わない
    assert!(f
        .composite(
            &CompositeManifest::new("slots", node("increment")).add_container("cam", "camera")
        )
        .is_err());
    Ok(())
}

#[test]
fn composite_invoke_execute_and_memo() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&two_stage())?;
    let p = pf.lock()?.create_process(
        &ProcessManifest::new("inc_minus_double")
            .name("m0")
            .use_memo(true),
    )?;
    // 既定値 (子の引数の既定値) で invoke: (1 + 1) - (1 * 2)
    assert_eq!(p.lock()?.invoke()?.lock_as_value(|v| v.as_i64())?, Some(0));
    assert!(!p.lock()?.is_updated()?);
    assert_eq!(p.lock()?.execute()?.lock_as_value(|v| v.as_i64())?, Some(0));
    // p_apply で引数を拘束すると、次の invoke で計算し直す
    p.lock_mut()?.p_apply("x", jvalue!(10).into())?;
    assert_eq!(p.lock()?.invoke()?.lock_as_value(|v| v.as_i64())?, Some(9));
    Ok(())
}

#[test]
fn composite_can_be_connected() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&two_stage())?;
    let comp = instance(&pf, "c0")?;
    let src = instance(f.0.get("double").unwrap(), "src0")?;
    let dst = instance(f.0.get("increment").unwrap(), "dst0")?;
    src.lock_mut()?.p_apply("arg1", jvalue!(4).into())?;
    comp.lock_mut()?.p_apply("y", jvalue!(3).into())?;

    // src -> comp.x (pull), comp -> dst.arg1 (pull)
    let m1 = ConnectionManifest::new(
        ConnectionType::Pull,
        src.identifier().clone(),
        "x".to_owned(),
        comp.identifier().clone(),
    );
    comp.lock_mut()?.notify_connected_from(src.clone(), &m1)?;
    src.lock_mut()?.try_connect_to(comp.clone(), &m1)?;
    let m2 = ConnectionManifest::new(
        ConnectionType::Pull,
        comp.identifier().clone(),
        "arg1".to_owned(),
        dst.identifier().clone(),
    );
    dst.lock_mut()?.notify_connected_from(comp.clone(), &m2)?;
    comp.lock_mut()?.try_connect_to(dst.clone(), &m2)?;

    // dst = ((4 * 2) + 1 - (3 * 2)) + 1
    assert_eq!(
        dst.lock()?.invoke()?.lock_as_value(|v| v.as_i64())?,
        Some(4)
    );
    Ok(())
}

#[test]
fn composite_can_be_bound_to_ec() -> JuizResult<()> {
    let mut f = Factories::new()?;
    let pf = f.composite(&two_stage())?;
    let comp = pf.lock()?.create_process(
        &ProcessManifest::new("inc_minus_double")
            .name("e0")
            .use_memo(true),
    )?;
    comp.lock_mut()?.p_apply("x", jvalue!(20).into())?;
    let ec = ExecutionContextCore::new();
    ec.lock().unwrap().bind(comp.clone())?;
    ec.lock().unwrap().svc()?;
    // (20 + 1) - (1 * 2)
    assert_eq!(
        comp.lock()?.get_output().lock_as_value(|v| v.as_i64())?,
        Some(19)
    );
    Ok(())
}

#[test]
fn composite_trace_logs_each_node() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let mut f = Factories::new()?;
    let pf = f.composite(&two_stage())?;
    let p = pf.lock()?.create_process(
        &ProcessManifest::new("inc_minus_double")
            .name("t0")
            .trace(true),
    )?;
    assert_eq!(call(&p, vec![("x", jvalue!(1)), ("y", jvalue!(1))])?, 0);
    Ok(())
}

/// Rust の Factory を System に直接登録する
fn register_rust_factory(
    system: &System,
    manifest: ProcessManifest,
    function: ProcessBodyFunctionType,
) -> JuizResult<()> {
    let type_name = manifest.type_name.clone();
    let pf = process_factory_create(manifest, function)?;
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .store_mut()
        .processes
        .register_factory(&type_name, pf)
}

#[test]
fn register_composite_in_system() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(jvalue!({
        "name": "composite_system_test",
        "option": { "http_broker": { "start": false } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        register_rust_factory(system, one_arg("increment"), increment)?;
        register_rust_factory(system, one_arg("double"), double)?;
        let composite = CompositeManifest::new(
            "inc_double",
            node("double").arg_node("arg1", node("increment")),
        );
        system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .register_composite(&composite)?;
        // 同じ type_name は二重に登録できない
        assert!(system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .register_composite(&composite)
            .is_err());

        let mut bp = system.core_broker().lock_mut()?;
        let profile = bp.process_create(&ProcessManifest::new("inc_double").name("id0"))?;
        let id = profile.identifier();
        let result = bp.process_call(&id, vec![("arg1", jvalue!(4))].into())?;
        assert_eq!(result.lock_as_value(|v| v.as_i64())?, Some(10));
        bp.process_p_apply(&id, "arg1", jvalue!(1).into())?;
        assert_eq!(
            bp.process_execute(&id)?.lock_as_value(|v| v.as_i64())?,
            Some(4)
        );
        Ok(())
    })
}

/// Python の子と Rust の子を混ぜた Composite が汎用経路で動く (仕様 §9)
#[test]
fn composite_mixes_python_and_rust() -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(jvalue!({
        "name": "composite_mixed_test",
        "option": {
            "http_broker": { "start": false },
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": {
            "process_factories": {
                "py_increment": { "language": "python", "path": "python" }
            }
        }
    }))?
    .set_working_dir(Path::new("tests"))
    .setup()?
    .run_and_do_once(|system| {
        register_rust_factory(system, one_arg("double"), double)?;
        // double(py_increment(double(arg1)))
        let pf = system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .register_composite(&CompositeManifest::new(
                "mixed",
                node("double").arg_node(
                    "arg1",
                    node("py_increment")
                        .arg("arg1", CompositeArgument::Node(Box::new(node("double")))),
                ),
            ))?;
        assert_eq!(pf.lock()?.manifest()?.language, MIXED_LANGUAGE);
        let p = system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .create_process_ref(&ProcessManifest::new("mixed").name("mx0"))?;
        // ((3 * 2) + 1) * 2
        assert_eq!(call(&p, vec![("arg1", jvalue!(3))])?, 14);
        Ok(())
    })
}
