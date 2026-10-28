//! Python のデータ型対応 (データ型 Step 5、issue #194) のテスト
//!
//! Python の Process は juiz-idl が生成した型 (bindings/pyjuiz/test/gen、juiz_idl/test_package/idl/geometry.jidl から作ったもの) を使う。
//!
//! - manifest に型参照 (schema / output_schema) が載り、Rust で作った同じ型と指紋が一致する
//! - Python → Python の接続では dataclass がそのまま (ネイティブ値で) 渡る
//! - Python → Rust では辞書 (to_value) になり、Rust → Python では型の値 (from_value) になる
//! - 接続時の型検査 (Step 4) が Python の型参照でも働く
//! - 型付きクライアント (client stub) で HTTP 越しに呼べる (bindings/pyjuiz/test/typed_client_test.py)
//!
//! 仕様は docs/spec/datatype.md §20。

use std::path::Path;

use juiz_core::prelude::*;
use juiz_sdk::{
    connections::ConnectionManifest,
    process_identifier::ProcessIdentifier,
    types::{compare_types, Compatibility, DataType, RecordField, RecordType, TypeSchema},
};

const PORT: i64 = 8024;

const PY_PROCESSES: [&str; 4] = [
    "py_typed_make_pose",
    "py_typed_pose_origin",
    "py_typed_mode",
    "move_to",
];

fn conf(http: bool) -> Value {
    let mut factories = serde_json::Map::new();
    for name in PY_PROCESSES {
        factories.insert(
            name.to_owned(),
            jvalue!({ "language": "python", "path": "python" }),
        );
    }
    let instance =
        |t: &str| jvalue!({ "type_name": t, "name": format!("{t}0"), "use_memo": false });
    jvalue!({
        "name": "python_datatype_test",
        "option": {
            "http_broker": { "start": http, "port": PORT },
            // Python プラグインの位置からの相対パス (tests/python -> リポジトリルート)
            "pythonpath": ["../../../bindings/pyjuiz", "../../../bindings/pyjuiz/test/gen"]
        },
        "plugins": { "process_factories": factories },
        "processes": PY_PROCESSES.iter().map(|t| instance(t)).collect::<Vec<Value>>(),
    })
}

fn with_system(http: bool, f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(conf(http))?
        .set_working_dir(Path::new("tests"))
        .setup()?
        .run_and_do_once(f)
}

fn process(system: &System, name: &str, type_name: &str) -> JuizResult<ProcessPtr> {
    let id: ProcessIdentifier = format!("core://core/process/{name}::{type_name}").try_into()?;
    system
        .core_broker()
        .lock()?
        .worker()
        .store()
        .processes
        .get(&id.to_string())
        .cloned()
}

fn py(system: &System, type_name: &str) -> JuizResult<ProcessPtr> {
    process(system, &format!("{type_name}0"), type_name)
}

fn connect(system: &System, src: &ProcessPtr, dst: &ProcessPtr, arg: &str) -> JuizResult<()> {
    let manifest = ConnectionManifest::new(
        ConnectionType::Pull,
        src.identifier(),
        arg.to_owned(),
        dst.identifier(),
    );
    system
        .core_broker()
        .lock_mut()?
        .connection_create(&manifest)
        .map(|_| ())
}

/// geometry.jidl の Pose2D を Rust (juiz_types) で組み立てたもの
fn rust_pose2d() -> TypeSchema {
    let point = DataType::Record(
        RecordType::new("robot.geometry", "Point2D")
            .doc("平面上の点")
            .field(RecordField::new("x", DataType::Double))
            .field(RecordField::new("y", DataType::Double)),
    );
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Pose2D")
            .field(RecordField::new("position", point))
            .field(
                RecordField::new("theta", DataType::Double)
                    .default_value(jvalue!(0.0))
                    .unwrap(),
            )
            .field(
                RecordField::new("frame", DataType::String)
                    .default_value(jvalue!("map"))
                    .unwrap(),
            )
            .field(
                RecordField::new("comment", DataType::optional(DataType::String))
                    .alias("note")
                    .default_value(Value::Null)
                    .unwrap(),
            ),
    ))
    .unwrap()
}

/// Rust の Factory を登録してインスタンスを作る
fn register_rust(
    system: &System,
    manifest: ProcessManifest,
    function: fn(CapsuleMap) -> JuizResult<Capsule>,
) -> JuizResult<ProcessPtr> {
    let type_name = manifest.type_name.clone();
    let mut cb = system.core_broker().lock_mut()?;
    cb.worker_mut()
        .store_mut()
        .processes
        .register_factory(&type_name, process_factory_create(manifest, function)?)?;
    cb.worker_mut()
        .create_process_ref(&ProcessManifest::new(&type_name).name(&format!("{type_name}0")))
}

/// 引数 pose をそのまま返す (Python の値が辞書で届くことを見る)
fn echo_pose(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(args.get("pose")?.lock_as_value(|v| v.clone())?.into())
}

fn make_dict_pose(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!({"position": {"x": 4.5, "y": 0.0}}).into())
}

/// manifest に型参照が載り、Rust で組み立てた同じ型と同じ指紋になる
#[test]
fn python_manifest_has_type_schema() -> JuizResult<()> {
    with_system(false, |system| {
        let make = py(system, "py_typed_make_pose")?
            .lock()?
            .profile()
            .map(|p| serde_json::to_value(p).unwrap())?;
        let schema: TypeSchema = serde_json::from_value(make["output_schema"].clone())?;
        schema.validate()?;
        assert_eq!(schema, rust_pose2d());
        assert_eq!(make["outputs"]["type_name"], "Object");

        let origin = py(system, "py_typed_pose_origin")?
            .lock()?
            .profile()
            .map(|p| serde_json::to_value(p).unwrap())?;
        let arg: TypeSchema = serde_json::from_value(origin["arguments"][0]["schema"].clone())?;
        assert!(matches!(
            compare_types(Some(&schema), Some(&arg))?,
            Compatibility::Same
        ));

        // 列挙の引数は String、既定値は記号の文字列
        let mode = py(system, "py_typed_mode")?
            .lock()?
            .profile()
            .map(|p| serde_json::to_value(p).unwrap())?;
        assert_eq!(mode["arguments"][0]["type_name"], "String");
        assert_eq!(mode["arguments"][0]["default"], "Velocity");
        assert_eq!(mode["output_schema"]["name"], "robot.geometry.MotorMode");
        Ok(())
    })
}

/// Python → Python の接続では dataclass がネイティブ値のまま渡る
#[test]
fn python_to_python_passes_dataclass() -> JuizResult<()> {
    with_system(false, |system| {
        let make = py(system, "py_typed_make_pose")?;
        let origin = py(system, "py_typed_pose_origin")?;
        connect(system, &make, &origin, "pose")?;
        let r = origin.lock()?.execute()?.lock_as_str(|s| s.to_owned())?;
        assert_eq!(r, "py_typed_make_pose");
        Ok(())
    })
}

/// Python の戻り値を Rust が読むと辞書 (to_value) になる。逆に Rust の辞書は Python で型の値になる
#[test]
fn python_and_rust_exchange_dictionaries() -> JuizResult<()> {
    with_system(false, |system| {
        // Python が返した Pose2D を読む
        let result = py(system, "py_typed_make_pose")?
            .lock()?
            .call(vec![("x", jvalue!(2.0))].into())?;
        assert!(result.is_native()?);
        assert_eq!(
            result.lock_as_value(|v| v.clone())?,
            jvalue!({"position": {"x": 2.0, "y": 0.0}, "theta": 0.0, "frame": "map", "comment": null})
        );

        // Python → Rust の接続 (同じ型の引数)
        let echo = register_rust(
            system,
            ProcessManifest::new("rust_echo_pose").add_typed_arg(
                "pose",
                "",
                rust_pose2d(),
                Value::Null,
            )?,
            echo_pose,
        )?;
        connect(system, &py(system, "py_typed_make_pose")?, &echo, "pose")?;
        let v = echo.lock()?.execute()?.lock_as_value(|v| v.clone())?;
        assert_eq!(v["position"]["x"], 0.0);
        assert_eq!(v["frame"], "map");

        // Rust → Python の接続。足りないフィールドは Python の from_value が既定値で埋める
        let make = register_rust(
            system,
            ProcessManifest::new("rust_make_pose").set_output_schema(Some(rust_pose2d()))?,
            make_dict_pose,
        )?;
        let origin = py(system, "py_typed_pose_origin")?;
        connect(system, &make, &origin, "pose")?;
        let r = origin.lock()?.execute()?.lock_as_str(|s| s.to_owned())?;
        assert_eq!(r, "dict:4.5");

        // 列挙: 辞書の記号を受け取り、記号を返す
        let mode = py(system, "py_typed_mode")?;
        let next = mode
            .lock()?
            .call(vec![("mode", jvalue!("Position"))].into())?
            .lock_as_value(|v| v.clone())?;
        assert_eq!(next, jvalue!("Idle"));
        let next = mode.lock()?.execute()?.lock_as_value(|v| v.clone())?;
        assert_eq!(next, jvalue!("Position"));
        Ok(())
    })
}

/// 型に合わない辞書で呼ぶと、Python の from_value のエラーになる
#[test]
fn calling_python_with_wrong_dictionary_is_error() -> JuizResult<()> {
    with_system(false, |system| {
        let origin = py(system, "py_typed_pose_origin")?;
        let err = origin
            .lock()?
            .call(vec![("pose", jvalue!({"position": {"x": "a"}}))].into())
            .unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.contains("ValueDoesNotMatchTypeError")
                || message.contains("value does not match type"),
            "{message}"
        );
        assert!(message.contains("pose.position.x"), "{message}");
        Ok(())
    })
}

/// 接続時の型検査 (Step 4) は Python の型参照でも働く
#[test]
fn incompatible_connection_with_python_is_error() -> JuizResult<()> {
    with_system(false, |system| {
        let make = py(system, "py_typed_make_pose")?;
        let mode = py(system, "py_typed_mode")?;
        let err = connect(system, &make, &mode, "mode")
            .unwrap_err()
            .downcast::<JuizError>()
            .expect("must be JuizError");
        match err {
            JuizError::TypeIncompatibleError { writer, reader, .. } => {
                assert!(writer.starts_with("robot.geometry.Pose2D"), "{writer}");
                assert!(reader.starts_with("robot.geometry.MotorMode"), "{reader}");
            }
            e => panic!("unexpected error: {e:?}"),
        }
        Ok(())
    })
}

/// 生成した client stub で HTTP 越しに呼ぶ (bindings/pyjuiz/test/typed_client_test.py)
#[test]
fn typed_client_over_http() -> JuizResult<()> {
    with_system(true, |_system| {
        let python = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_owned());
        let output = std::process::Command::new(python)
            .current_dir("../bindings/pyjuiz")
            .env("PYTHONPATH", ".")
            .args(["test/typed_client_test.py", PORT.to_string().as_str()])
            .output()?;
        assert!(
            output.status.success(),
            "typed_client_test.py failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    })
}
