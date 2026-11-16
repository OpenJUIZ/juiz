//! 遅延評価 L1 (issue #175) のテスト
//!
//! 仕様は docs/spec/lazy-evaluation.md。Python の Process の戻り値は Native(python) で包まれ、
//! - Python 同士 (接続と Composite) では変換せずにそのまま渡る
//! - Python 以外が読むときだけ encode され、今と同じ値になる
//! ことを確かめる。
//!
//! 変換が起きていないことは、juiz の値に変換できないオブジェクト (py_lazy_opaque) が
//! Python 同士なら渡せることで確かめる。

use std::path::Path;

use juiz_core::prelude::*;
use juiz_sdk::manifests::{CompositeArgument, CompositeNode};
use juiz_sdk::process_identifier::ProcessIdentifier;

const PY_PROCESSES: [&str; 5] = [
    "py_lazy_numpy",
    "py_lazy_opaque",
    "py_lazy_type_name",
    "py_lazy_gray",
    "py_lazy_image_mode",
];

fn conf() -> Value {
    let mut factories = serde_json::Map::new();
    for name in PY_PROCESSES {
        factories.insert(
            name.to_owned(),
            jvalue!({ "language": "python", "path": "python" }),
        );
    }
    let instance =
        |t: &str| jvalue!({ "type_name": t, "name": format!("{t}0"), "use_memo": false });
    let pull = |source: &str, destination: &str, arg_name: &str| {
        jvalue!({
            "connection_type": "Pull",
            "source": { "type_name": source, "name": format!("{source}0") },
            "destination": { "type_name": destination, "name": format!("{destination}0") },
            "arg_name": arg_name
        })
    };
    jvalue!({
        "name": "lazy_eval_test",
        "option": {
            "http_broker": { "start": false },
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": { "process_factories": factories },
        "composites": [
            {
                "type_name": "opaque_type_name",
                "body": { "process": "py_lazy_type_name", "args": { "x": { "process": "py_lazy_opaque" } } }
            },
            {
                "type_name": "gray_mode",
                "body": { "process": "py_lazy_image_mode", "args": { "img": { "process": "py_lazy_gray" } } }
            }
        ],
        "processes": [
            instance("py_lazy_numpy"),
            instance("py_lazy_opaque"),
            instance("py_lazy_type_name"),
            instance("py_lazy_gray"),
            instance("py_lazy_image_mode"),
            instance("opaque_type_name"),
            instance("gray_mode"),
        ],
        "connections": [
            pull("py_lazy_numpy", "py_lazy_type_name", "x"),
            pull("py_lazy_gray", "py_lazy_image_mode", "img"),
        ]
    })
}

fn process(system: &System, type_name: &str) -> JuizResult<ProcessPtr> {
    let id: ProcessIdentifier =
        format!("core://core/process/{type_name}0::{type_name}").try_into()?;
    system
        .core_broker()
        .lock()?
        .worker()
        .store()
        .processes
        .get(&id.to_string())
        .cloned()
}

fn execute_str(system: &System, type_name: &str) -> JuizResult<String> {
    process(system, type_name)?
        .lock()?
        .execute()?
        .lock_as_str(|s| s.to_owned())
}

/// Rust の Process: 配列の和
fn rust_sum(args: CapsuleMap) -> JuizResult<Capsule> {
    let sum = args.get("arr")?.lock_as_value(|v| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_f64().unwrap())
            .sum::<f64>()
    })?;
    Ok(jvalue!(sum).into())
}

fn with_system(f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?
        .set_working_dir(Path::new("tests"))
        .setup()?
        .run_and_do_once(f)
}

/// Python → Python の接続では numpy の配列がそのまま渡る (今までは todo!() で落ちていた)
#[test]
fn python_to_python_connection_passes_numpy_array() -> JuizResult<()> {
    with_system(|system| {
        assert_eq!(execute_str(system, "py_lazy_type_name")?, "numpy.ndarray");
        Ok(())
    })
}

/// Python → Python の Composite では、変換できないオブジェクトも変換せずに渡る
#[test]
fn python_to_python_composite_does_not_encode() -> JuizResult<()> {
    with_system(|system| {
        let name = execute_str(system, "opaque_type_name")?;
        assert!(name.ends_with(".Opaque"), "{name}");
        Ok(())
    })
}

/// 画像も PIL の画像のまま渡るので、グレースケールが RGB にならない
#[test]
fn python_to_python_keeps_pil_image() -> JuizResult<()> {
    with_system(|system| {
        assert_eq!(execute_str(system, "py_lazy_image_mode")?, "L");
        assert_eq!(execute_str(system, "gray_mode")?, "L");
        Ok(())
    })
}

/// Python 以外が読むときは encode され、今と同じ値になる
#[test]
fn reading_python_result_encodes_it() -> JuizResult<()> {
    with_system(|system| {
        // 戻り値は Native のまま返り、読むと numpy の配列は tolist() の値になる
        let result = process(system, "py_lazy_numpy")?.lock()?.execute()?;
        assert!(result.is_native()?);
        assert!(result.is_value()?);
        assert_eq!(
            result.lock_as_value(|v| v.clone())?,
            jvalue!([0.0, 1.0, 2.0])
        );
        // ブローカーの応答と同じ形にできる
        assert_eq!(
            capsule_to_value(result)?,
            jvalue!({ "__value__": [0.0, 1.0, 2.0], "__option__": {} })
        );

        // PIL の画像は Image として読める
        let gray = process(system, "py_lazy_gray")?.lock()?.execute()?;
        assert!(gray.is_image()?);
        assert_eq!(
            gray.lock_as_image(|img| (img.width(), img.height()))?,
            (4, 3)
        );
        Ok(())
    })
}

/// 変換できない値を Python 以外が読むと、落ちずに変換のエラーになる
#[test]
fn encoding_unsupported_python_object_is_error() -> JuizResult<()> {
    with_system(|system| {
        let result = process(system, "py_lazy_opaque")?.lock()?.execute()?;
        let err = result.lock_as_value(|v| v.clone()).unwrap_err();
        assert!(
            format!("{err:#}").contains("Opaque"),
            "unexpected error: {err:#}"
        );
        Ok(())
    })
}

/// Python → Rust の Composite では Rust の段が encode された値を受け取る
#[test]
fn python_to_rust_composite_encodes() -> JuizResult<()> {
    with_system(|system| {
        let type_name = "rust_sum";
        let pf = process_factory_create(
            ProcessManifest::new(type_name).add_array_arg("arr", "array", vec![]),
            rust_sum,
        )?;
        system
            .core_broker()
            .lock_mut()?
            .worker_mut()
            .store_mut()
            .processes
            .register_factory(type_name, pf)?;
        let mut worker = system.core_broker().lock_mut()?;
        worker
            .worker_mut()
            .register_composite(&juiz_sdk::manifests::CompositeManifest::new(
                "numpy_sum",
                CompositeNode::new("rust_sum").arg(
                    "arr",
                    CompositeArgument::Node(Box::new(
                        CompositeNode::new("py_lazy_numpy")
                            .arg("n", CompositeArgument::Const(jvalue!(5))),
                    )),
                ),
            ))?;
        let p = worker
            .worker_mut()
            .create_process_ref(&ProcessManifest::new("numpy_sum").name("ns0"))?;
        drop(worker);
        let sum = p
            .lock()?
            .call(CapsuleMap::new())?
            .lock_as_value(|v| v.as_f64())?;
        assert_eq!(sum, Some(10.0));
        Ok(())
    })
}
