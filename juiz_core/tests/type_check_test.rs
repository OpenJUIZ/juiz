//! core での型の扱い (データ型 Step 4、issue #193) のテスト
//!
//! - 接続時: source の戻り値の型と destination の引数の型が互換でなければ、接続しないでエラー
//! - Topic: publish / subscribe を始めるとき、既存の相手と型を比べる
//! - 型の登録所: type/list・type/schema (CoreBroker と HTTP)
//! - 呼び出し時の検査: conf の option.type_check.on_call で入れる (既定はオフ)
//!
//! 仕様は docs/spec/datatype.md の §8 と §19。プラグインは使わず、型付きの manifest の Factory をテストの中で登録する。
use std::sync::{Arc, Mutex};

use juiz_core::{prelude::*, types::TypeEntry};
use juiz_sdk::{
    connections::{ConnectionManifest, ConnectionProfile},
    manifests::TopicManifest,
    process_identifier::ProcessIdentifier,
    types::{DataType, RecordField, RecordType, TypeSchema},
};

const PORT: i64 = 8022;

fn point2d() -> DataType {
    DataType::Record(
        RecordType::new("robot.geometry", "Point2D")
            .field(RecordField::new("x", DataType::Double))
            .field(RecordField::new("y", DataType::Double)),
    )
}

fn pose2d_record() -> RecordType {
    RecordType::new("robot.geometry", "Pose2D")
        .field(RecordField::new("position", point2d()))
        .field(
            RecordField::new("theta", DataType::Double)
                .default_value(jvalue!(0.0))
                .unwrap(),
        )
}

/// Pose2D
fn pose2d() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(pose2d_record())).unwrap()
}

/// 既定値つきの frame を足した新しい Pose2D (古い Pose2D の値を読める)
fn pose2d_v2() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        pose2d_record().field(
            RecordField::new("frame", DataType::String)
                .default_value(jvalue!("map"))
                .unwrap(),
        ),
    ))
    .unwrap()
}

/// 既定値なしの frame を足した Pose2D (古い Pose2D の値を読めない)
fn pose2d_strict() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Pose2D")
            .field(RecordField::new("position", point2d()))
            .field(RecordField::new("frame", DataType::String)),
    ))
    .unwrap()
}

/// 名前の違う型
fn twist() -> TypeSchema {
    TypeSchema::from_data_type(&DataType::Record(
        RecordType::new("robot.geometry", "Twist")
            .field(RecordField::new("linear", DataType::Double))
            .field(RecordField::new("angular", DataType::Double)),
    ))
    .unwrap()
}

fn pose_value() -> Value {
    jvalue!({"position": {"x": 1.5, "y": 0.0}, "theta": 0.0})
}

fn make_pose(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(pose_value().into())
}

fn make_twist(_: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!({"linear": 1.0, "angular": 0.0}).into())
}

/// pose.position.x を返す。読めなければ null
fn read_x(args: CapsuleMap) -> JuizResult<Capsule> {
    let x = args
        .get("pose")?
        .lock_as_value(|v| v.pointer("/position/x").cloned().unwrap_or(Value::Null))?;
    Ok(x.into())
}

/// 型付きの Factory を登録する
fn register(system: &System) -> JuizResult<()> {
    let factories: Vec<(ProcessManifest, fn(CapsuleMap) -> JuizResult<Capsule>)> = vec![
        (
            ProcessManifest::new("make_pose").set_output_schema(Some(pose2d()))?,
            make_pose,
        ),
        (
            ProcessManifest::new("make_twist").set_output_schema(Some(twist()))?,
            make_twist,
        ),
        (
            ProcessManifest::new("read_pose").add_typed_arg("pose", "", pose2d(), pose_value())?,
            read_x,
        ),
        (
            ProcessManifest::new("read_pose_v2").add_typed_arg(
                "pose",
                "",
                pose2d_v2(),
                pose_value(),
            )?,
            read_x,
        ),
        (
            ProcessManifest::new("read_pose_strict").add_typed_arg(
                "pose",
                "",
                pose2d_strict(),
                Value::Null,
            )?,
            read_x,
        ),
        (
            ProcessManifest::new("read_untyped").add_object_arg("pose", "", pose_value()),
            read_x,
        ),
        (ProcessManifest::new("make_untyped"), make_twist),
    ];
    let mut cb = system.core_broker().lock_mut()?;
    for (manifest, function) in factories {
        let type_name = manifest.type_name.clone();
        let pf = process_factory_create(manifest, function)?;
        cb.worker_mut()
            .store_mut()
            .processes
            .register_factory(&type_name, pf)?;
    }
    Ok(())
}

fn system_manifest(on_call: bool, http: bool) -> Value {
    jvalue!({
        "name": "type_check_test",
        "option": {
            "http_broker": { "start": http, "port": PORT },
            "type_check": { "on_call": on_call }
        },
    })
}

fn with_system(
    on_call: bool,
    http: bool,
    f: impl FnOnce(&mut System) -> JuizResult<()>,
) -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(system_manifest(on_call, http))?
        .setup()?
        .run_and_do_once(|system| {
            register(system)?;
            f(system)
        })
}

fn create(system: &System, type_name: &str, name: &str) -> JuizResult<ProcessPtr> {
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .create_process_ref(&ProcessManifest::new(type_name).name(name))
}

fn connect(system: &System, src: &ProcessPtr, dst: &ProcessPtr) -> JuizResult<ConnectionProfile> {
    let manifest = ConnectionManifest::new(
        ConnectionType::Pull,
        src.identifier(),
        "pose".to_owned(),
        dst.identifier(),
    );
    system
        .core_broker()
        .lock_mut()?
        .connection_create(&manifest)
}

fn error_of<T: std::fmt::Debug>(r: JuizResult<T>) -> JuizError {
    r.expect_err("must fail")
        .downcast::<JuizError>()
        .expect("must be JuizError")
}

fn num_inputs(p: &ProcessPtr) -> usize {
    p.lock().unwrap().source_connections().unwrap().len()
}

/// 同じ型・互換な型・型なしの相手とはつながり、値が流れる
#[test]
fn connection_with_compatible_types() -> JuizResult<()> {
    with_system(false, false, |system| {
        let make = create(system, "make_pose", "mp0")?;
        let make_untyped = create(system, "make_untyped", "mu0")?;
        for (dst_type, src) in [
            ("read_pose", &make),         // 同じ
            ("read_pose_v2", &make),      // 互換 (既定値つきのフィールドが増えた)
            ("read_untyped", &make),      // reader が型なし
            ("read_pose", &make_untyped), // writer が型なし
        ] {
            let dst = create(
                system,
                dst_type,
                &format!("{dst_type}_{}", src.identifier().name),
            )?;
            connect(system, src, &dst)
                .unwrap_or_else(|e| panic!("{dst_type} must be connected: {e:#}"));
            assert_eq!(num_inputs(&dst), 1);
        }
        let read = system
            .core_broker()
            .lock()?
            .worker()
            .store()
            .processes
            .get(&"core://core/process/read_pose_v2_mp0::read_pose_v2".to_owned())?
            .clone();
        let x = read.lock()?.invoke()?.lock_as_value(|v| v.as_f64())?;
        assert_eq!(x, Some(1.5));
        Ok(())
    })
}

/// 互換でなければ TypeIncompatibleError で、接続は作らない
#[test]
fn connection_with_incompatible_types_is_error() -> JuizResult<()> {
    with_system(false, false, |system| {
        let twist_src = create(system, "make_twist", "mt0")?;
        let pose_src = create(system, "make_pose", "mp0")?;
        let read = create(system, "read_pose", "rp0")?;
        let strict = create(system, "read_pose_strict", "rs0")?;

        match error_of(connect(system, &twist_src, &read)) {
            JuizError::TypeIncompatibleError { writer, reader, .. } => {
                assert_eq!(writer, twist().identifier());
                assert_eq!(reader, pose2d().identifier());
            }
            e => panic!("unexpected error {e}"),
        }
        // 既定値のないフィールドが足りない
        match error_of(connect(system, &pose_src, &strict)) {
            JuizError::TypeIncompatibleError { path, .. } => {
                assert!(path.contains("frame"), "{path}")
            }
            e => panic!("unexpected error {e}"),
        }
        assert_eq!(num_inputs(&read), 0);
        assert_eq!(num_inputs(&strict), 0);
        assert!(twist_src.lock()?.destination_connections()?.is_empty());
        Ok(())
    })
}

fn publish(system: &System, p: &ProcessPtr, topic: &str) -> JuizResult<()> {
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .process_publish_topic(p.clone(), TopicManifest::new(topic))
}

fn subscribe(system: &System, p: &ProcessPtr, topic: &str) -> JuizResult<()> {
    system
        .core_broker()
        .lock_mut()?
        .worker_mut()
        .process_subscribe_topic(p.clone(), &"pose".to_owned(), TopicManifest::new(topic))
}

/// Topic: 後から来た側を、既存の相手それぞれと比べる
#[test]
fn topic_type_check() -> JuizResult<()> {
    with_system(false, false, |system| {
        let make = create(system, "make_pose", "mp0")?;
        let twist_src = create(system, "make_twist", "mt0")?;
        let read = create(system, "read_pose", "rp0")?;
        let read_v2 = create(system, "read_pose_v2", "rv0")?;
        let untyped = create(system, "read_untyped", "ru0")?;
        let strict = create(system, "read_pose_strict", "rs0")?;

        // publish が先
        publish(system, &make, "pose")?;
        subscribe(system, &read, "pose")?;
        subscribe(system, &read_v2, "pose")?;
        subscribe(system, &untyped, "pose")?;
        match error_of(subscribe(system, &strict, "pose")) {
            JuizError::TypeIncompatibleError { .. } => {}
            e => panic!("unexpected error {e}"),
        }
        assert_eq!(num_inputs(&strict), 0);
        // subscribe が先 (Twist は read_pose・read_pose_v2 で読めない)
        match error_of(publish(system, &twist_src, "pose")) {
            JuizError::TypeIncompatibleError { writer, .. } => {
                assert_eq!(writer, twist().identifier())
            }
            e => panic!("unexpected error {e}"),
        }
        assert!(twist_src.lock()?.destination_connections()?.is_empty());

        // 型なしの相手だけの Topic なら、どの型でもつながる
        let untyped2 = create(system, "read_untyped", "ru1")?;
        subscribe(system, &untyped2, "free")?;
        publish(system, &twist_src, "free")?;
        publish(system, &make, "free")?;

        // 値が届く
        make.lock()?.execute()?;
        // 同じ名前の引数の subscribe 側がいくつあっても、すべてに届く
        for p in [&read, &read_v2, &untyped] {
            let x = p.lock()?.invoke()?.lock_as_value(|v| v.as_f64())?;
            assert_eq!(x, Some(1.5));
        }
        Ok(())
    })
}

fn check_registry(bp: &dyn BrokerProxy) -> JuizResult<()> {
    let list: Vec<TypeEntry> = bp.type_list()?;
    let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
    // Pose2D は 3 つの版、入れ子の Point2D も載る。識別子の順
    assert_eq!(
        names
            .iter()
            .filter(|n| **n == "robot.geometry.Pose2D")
            .count(),
        3,
        "{list:?}"
    );
    assert!(names.contains(&"robot.geometry.Point2D"), "{list:?}");
    assert!(names.contains(&"robot.geometry.Twist"), "{list:?}");
    let mut sorted = list
        .iter()
        .map(|e| e.identifier.clone())
        .collect::<Vec<_>>();
    sorted.sort();
    assert_eq!(
        sorted,
        list.iter()
            .map(|e| e.identifier.clone())
            .collect::<Vec<_>>()
    );

    let pose = list
        .iter()
        .find(|e| e.identifier == pose2d().identifier())
        .expect("Pose2D");
    assert_eq!(pose.kind, "record");
    assert_eq!(pose.used_by, vec!["make_pose", "read_pose"]);
    assert!(pose.short_identifier.starts_with("Pose2D@"));
    let point = list
        .iter()
        .find(|e| e.name == "robot.geometry.Point2D")
        .expect("Point2D");
    assert_eq!(
        point.used_by,
        vec!["make_pose", "read_pose", "read_pose_strict", "read_pose_v2"]
    );

    // 識別子で引く
    assert_eq!(bp.type_schema(&pose2d().identifier())?, pose2d());
    // 完全名だけでも、1 つに決まれば引ける
    assert_eq!(bp.type_schema("robot.geometry.Twist")?, twist());
    // 決まらない・ない
    let e = format!(
        "{:#}",
        bp.type_schema("robot.geometry.Pose2D")
            .expect_err("ambiguous")
    );
    assert!(e.contains("ambiguous"), "{e}");
    let e = format!(
        "{:#}",
        bp.type_schema("robot.geometry.Nothing")
            .expect_err("not found")
    );
    assert!(e.contains("not found"), "{e}");
    Ok(())
}

/// 型の登録所。Factory の manifest にある型 (入れ子も含む) が載る
#[test]
fn type_registry_api() -> JuizResult<()> {
    with_system(false, true, |system| {
        check_registry(&*system.core_broker().lock()?)?;

        // HTTP の BrokerProxy から
        let host = format!("localhost:{PORT}");
        let bp: Arc<Mutex<dyn BrokerProxy>> =
            system
                .core_broker()
                .lock()?
                .worker()
                .broker_proxy("http", host.as_str(), true)?;
        check_registry(&*bp.lock().unwrap())?;

        // JSON そのもの
        let schema: Value = reqwest::blocking::get(format!(
            "http://localhost:{PORT}/api/type/schema?identifier={}",
            twist().identifier()
        ))?
        .json()?;
        assert_eq!(schema, serde_json::to_value(twist())?);
        Ok(())
    })
}

fn call_read_pose(system: &System, pose: Value) -> JuizResult<CapsulePtr> {
    let id: ProcessIdentifier = "core://core/process/rp0::read_pose".try_into()?;
    let args: CapsuleMap = vec![("pose", pose)].into();
    system.core_broker().lock()?.process_call(&id, args)
}

fn p_apply_read_pose(system: &System, pose: Value) -> JuizResult<CapsulePtr> {
    let id: ProcessIdentifier = "core://core/process/rp0::read_pose".try_into()?;
    system
        .core_broker()
        .lock_mut()?
        .process_p_apply(&id, "pose", pose.into())
}

/// 呼び出し時の検査を入れると、型に合わない辞書は関数を呼ばずにエラー
#[test]
fn call_time_check_when_enabled() -> JuizResult<()> {
    with_system(true, false, |system| {
        create(system, "read_pose", "rp0")?;
        let x = call_read_pose(system, pose_value())?.lock_as_value(|v| v.as_f64())?;
        assert_eq!(x, Some(1.5));
        // 足りないフィールドに既定値があれば通る
        let x = call_read_pose(system, jvalue!({"position": {"x": 2.0, "y": 0.0}}))?
            .lock_as_value(|v| v.as_f64())?;
        assert_eq!(x, Some(2.0));

        let bad = jvalue!({"position": {"x": "one", "y": 0.0}});
        match error_of(call_read_pose(system, bad.clone())) {
            JuizError::ValueDoesNotMatchTypeError { path, .. } => {
                assert!(path.contains("position.x"), "{path}")
            }
            e => panic!("unexpected error {e}"),
        }
        match error_of(p_apply_read_pose(system, bad)) {
            JuizError::ValueDoesNotMatchTypeError { .. } => {}
            e => panic!("unexpected error {e}"),
        }
        Ok(())
    })
}

/// 既定 (オフ) では検査しないで関数に渡す
#[test]
fn call_time_check_is_off_by_default() -> JuizResult<()> {
    let _ = env_logger::try_init();
    let manifest = jvalue!({
        "name": "type_check_test_default",
        "option": { "http_broker": { "start": false } },
    });
    System::new(manifest)?.setup()?.run_and_do_once(|system| {
        register(system)?;
        create(system, "read_pose", "rp0")?;
        let x = call_read_pose(system, jvalue!({"position": {"x": "one"}}))?
            .lock_as_value(|v| v.clone())?;
        assert_eq!(x, jvalue!("one"));
        p_apply_read_pose(system, jvalue!({"position": {"x": "two"}}))?;
        Ok(())
    })
}
