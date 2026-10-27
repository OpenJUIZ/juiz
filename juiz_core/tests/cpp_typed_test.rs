//! C++ のデータ型 (データ型 Step 6、issue #195) と遅延評価 L3 (issue #177) のテスト
//!
//! C++ のコンポーネント examples/cpp/component/cpp_typed_component (juiz-idl -g cpp が
//! juiz_idl/test_package/idl/geometry.jidl から作った型と、JUIZ_TYPE で登録した counted.Odometry を使う) と、
//! 同じ型を ABI タグを変えて受け取る cpp_typed_component_other_abi を、../target/debug から読む。
//!
//! - manifest に型参照 (schema / output_schema) が載り、Rust で作った同じ型と指紋が一致する
//! - C++ → C++ (Composite・接続) では record が encode も decode もされずに渡る
//! - ABI タグが違う C++、C++ → Rust、Rust → C++ では辞書を通して渡る
//! - 辞書で渡した引数は型で読まれ (足りないフィールドは既定値)、いろいろな型が往復する
//!
//! 仕様は docs/spec/datatype.md §21 と docs/spec/lazy-evaluation.md §8・§9。
//! C++ の例は scripts/test.sh (CI も同じ) でビルドされている前提。

use juiz_core::prelude::*;
use juiz_sdk::{
    process_identifier::ProcessIdentifier,
    types::{DataType, RecordField, RecordType, TypeSchema},
};

const CPP_PROCESSES: [&str; 9] = [
    "cpp_pose_make",
    "cpp_pose_move",
    "cpp_pose_x",
    "cpp_pose_frame",
    "cpp_everything_echo",
    "cpp_mode_next",
    "cpp_odometry_make",
    "cpp_odometry_step",
    "cpp_odometry_stats",
];

const CPP_OTHER_PROCESSES: [&str; 3] = [
    "cpp_odometry_steps_other",
    "cpp_pose_x_other",
    "cpp_odometry_stats_other",
];

fn conf() -> Value {
    let instance =
        |t: &str| jvalue!({ "type_name": t, "name": format!("{t}0"), "use_memo": false });
    let mut processes: Vec<Value> = CPP_PROCESSES
        .iter()
        .chain(CPP_OTHER_PROCESSES.iter())
        .map(|t| instance(t))
        .collect();
    for t in [
        "native_pose_make",
        "native_pose_reader",
        "odometry_chain",
        "odometry_other_abi",
        "pose_cpp_chain",
        "pose_cpp_to_rust",
        "pose_rust_to_cpp",
        "pose_cpp_other_abi",
    ] {
        processes.push(instance(t));
    }
    let call = |process: &str, args: Value| jvalue!({ "process": process, "args": args });
    jvalue!({
        "name": "cpp_typed_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "components": {
                "cpp_typed_component": { "language": "c++", "path": "../target/debug" },
                "cpp_typed_component_other_abi": { "language": "c++", "path": "../target/debug" },
                "native_pose_component": { "path": "../target/debug" }
            },
            "process_factories": { "native_pose_reader": { "path": "../target/debug" } }
        },
        "composites": [
            {
                // step(step(make(distance)))。C++ どうしなので encode も decode もしない
                "type_name": "odometry_chain",
                "body": call("cpp_odometry_step", jvalue!({
                    "odometry": call("cpp_odometry_step", jvalue!({
                        "odometry": { "process": "cpp_odometry_make" }
                    }))
                }))
            },
            {
                // ABI タグの違う C++ へは辞書で渡る
                "type_name": "odometry_other_abi",
                "body": call("cpp_odometry_steps_other", jvalue!({
                    "odometry": { "process": "cpp_odometry_make" }
                }))
            },
            {
                "type_name": "pose_cpp_chain",
                "body": call("cpp_pose_x", jvalue!({
                    "pose": call("cpp_pose_move", jvalue!({
                        "pose": { "process": "cpp_pose_make" },
                        "dx": { "const": 1.0 }
                    })),
                    "offset": { "const": { "position": { "x": 0.0, "y": 0.0 } } }
                }))
            },
            {
                "type_name": "pose_cpp_to_rust",
                "body": call("native_pose_reader", jvalue!({ "pose": { "process": "cpp_pose_make" } }))
            },
            {
                "type_name": "pose_rust_to_cpp",
                "body": call("cpp_pose_x", jvalue!({
                    "pose": { "process": "native_pose_make" },
                    "offset": { "const": { "position": { "x": 0.0, "y": 0.0 } } }
                }))
            },
            {
                "type_name": "pose_cpp_other_abi",
                "body": call("cpp_pose_x_other", jvalue!({ "pose": { "process": "cpp_pose_make" } }))
            }
        ],
        "processes": processes,
        "containers": [
            {
                "type_name": "cpp_pose_holder",
                "name": "holder0",
                "processes": [ { "type_name": "cpp_pose_holder_scale", "name": "scale0" } ]
            }
        ],
        "connections": [
            {
                "connection_type": "Pull",
                "source": { "type_name": "cpp_odometry_make", "name": "cpp_odometry_make0" },
                "destination": { "type_name": "cpp_odometry_step", "name": "cpp_odometry_step0" },
                "arg_name": "odometry"
            }
        ]
    })
}

fn with_system(f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    let _ = env_logger::try_init();
    System::new(conf())?.setup()?.run_and_do_once(f)
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

fn call(system: &System, type_name: &str, args: Vec<(&str, Value)>) -> JuizResult<CapsulePtr> {
    process(system, type_name)?.lock()?.call(args.into())
}

fn value_of(result: &CapsulePtr) -> JuizResult<Value> {
    result.lock_as_value(|v| v.clone())
}

/// C++ のプラグインの中で counted.Odometry を encode・decode した回数 (読むと 0 に戻る)
fn stats(system: &System, type_name: &str) -> JuizResult<Value> {
    value_of(&call(system, type_name, vec![])?)
}

fn main_stats(system: &System) -> JuizResult<Value> {
    stats(system, "cpp_odometry_stats")
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

#[test]
fn manifest_has_type_schemas() -> JuizResult<()> {
    with_system(|system| {
        let expected = rust_pose2d();
        let profile = process(system, "cpp_pose_move")?.lock()?.profile()?;
        let pose = profile.arguments.iter().find(|a| a.name == "pose").unwrap();
        let schema = pose.schema.as_ref().unwrap();
        assert_eq!(schema.name, "robot.geometry.Pose2D");
        assert_eq!(schema.fingerprint, expected.fingerprint);
        assert_eq!(pose.type_name.as_str(), "Object");
        // 既定値は Pose2D{} を辞書にしたもの
        assert_eq!(
            pose.default,
            jvalue!({"position": {"x": 0.0, "y": 0.0}, "theta": 0.0, "frame": "map", "comment": null})
        );
        assert_eq!(
            profile.output_schema.as_ref().unwrap().fingerprint,
            expected.fingerprint
        );
        // 型参照は libjuiz_sdk の検査を通る (他の言語から届いた型参照と同じ扱い)
        schema.validate().unwrap();

        // 列挙は type_name が String
        let profile = process(system, "cpp_mode_next")?.lock()?.profile()?;
        assert_eq!(profile.arguments[0].type_name.as_str(), "String");
        assert_eq!(profile.arguments[0].default, jvalue!("Idle"));
        assert_eq!(
            profile.output_schema.as_ref().unwrap().name,
            "robot.geometry.MotorMode"
        );

        // JUIZ_TYPE で登録した型
        let profile = process(system, "cpp_odometry_step")?.lock()?.profile()?;
        assert_eq!(
            profile.arguments[0].schema.as_ref().unwrap().name,
            "counted.Odometry"
        );
        // 型なしの Process は今のまま
        let profile = process(system, "cpp_odometry_stats")?.lock()?.profile()?;
        assert!(profile.output_schema.is_none());
        Ok(())
    })
}

/// C++ どうしの Composite では、record を encode も decode もせずに渡す
#[test]
fn composite_between_cpp_passes_native_values() -> JuizResult<()> {
    with_system(|system| {
        main_stats(system)?;
        let result = call(system, "odometry_chain", vec![("distance", jvalue!(1.0))])?;
        assert!(result.is_native()?);
        assert_eq!(
            result.native_cell()?.unwrap().describe(),
            "cpp:counted.Odometry"
        );
        let abi = result.lock_as_native(|n| n.type_tag().unwrap().abi.clone())?;
        assert!(abi.starts_with("cpp:"), "{abi}");
        assert_eq!(main_stats(system)?, jvalue!({"encodes": 0, "decodes": 0}));
        // 読むと encode される (1 回だけ)
        assert_eq!(value_of(&result)?, jvalue!({"distance": 3.0, "steps": 2}));
        assert_eq!(value_of(&result)?, jvalue!({"distance": 3.0, "steps": 2}));
        assert_eq!(main_stats(system)?, jvalue!({"encodes": 1, "decodes": 0}));
        Ok(())
    })
}

/// C++ どうしの接続でも encode しない
#[test]
fn connection_between_cpp_passes_native_values() -> JuizResult<()> {
    with_system(|system| {
        main_stats(system)?;
        let result = process(system, "cpp_odometry_step")?.lock()?.execute()?;
        assert!(result.is_native()?);
        assert_eq!(main_stats(system)?, jvalue!({"encodes": 0, "decodes": 0}));
        assert_eq!(value_of(&result)?["steps"], jvalue!(1));
        Ok(())
    })
}

/// ABI タグが違う C++ へは辞書を通して渡る
#[test]
fn other_abi_receives_through_dictionary() -> JuizResult<()> {
    with_system(|system| {
        main_stats(system)?;
        stats(system, "cpp_odometry_stats_other")?;
        let result = call(
            system,
            "odometry_other_abi",
            vec![("distance", jvalue!(2.0))],
        )?;
        assert_eq!(value_of(&result)?, jvalue!(0));
        assert_eq!(main_stats(system)?, jvalue!({"encodes": 1, "decodes": 0}));
        assert_eq!(
            stats(system, "cpp_odometry_stats_other")?,
            jvalue!({"encodes": 0, "decodes": 1})
        );
        // IDL から作った型も同じ
        let result = call(system, "pose_cpp_other_abi", vec![("x", jvalue!(4.0))])?;
        assert_eq!(value_of(&result)?, jvalue!(4.0));
        Ok(())
    })
}

/// C++ の record の値を Rust の Process が辞書で受け取り、Rust の値を C++ が辞書で受け取る
#[test]
fn cpp_and_rust_exchange_through_dictionary() -> JuizResult<()> {
    with_system(|system| {
        let result = call(system, "pose_cpp_to_rust", vec![("x", jvalue!(2.5))])?;
        assert_eq!(value_of(&result)?, jvalue!(2.5));
        // Rust の Pose2D には frame・comment が無いが、C++ 側で既定値になる
        let result = call(system, "pose_rust_to_cpp", vec![("x", jvalue!(1.5))])?;
        assert_eq!(value_of(&result)?, jvalue!(1.5));
        let result = call(system, "pose_cpp_chain", vec![("x", jvalue!(0.5))])?;
        assert_eq!(value_of(&result)?, jvalue!(1.5));
        Ok(())
    })
}

/// 辞書で渡した引数は型で読まれ、足りないフィールドは既定値になる
#[test]
fn dictionary_arguments_are_read_with_type() -> JuizResult<()> {
    with_system(|system| {
        let frame = |pose: Value| -> JuizResult<Value> {
            value_of(&call(system, "cpp_pose_frame", vec![("pose", pose)])?)
        };
        assert_eq!(
            frame(jvalue!({"position": {"x": 1, "y": 2}}))?,
            jvalue!("map/none")
        );
        assert_eq!(
            frame(
                jvalue!({"position": {"x": 1, "y": 2}, "frame": "odom", "comment": "c", "extra": 1})
            )?,
            jvalue!("odom/c")
        );
        // 型に合わない値はエラー
        assert!(call(
            system,
            "cpp_pose_frame",
            vec![("pose", jvalue!({"position": "x"}))]
        )
        .is_err());

        let x = call(
            system,
            "cpp_pose_x",
            vec![
                ("pose", jvalue!({"position": {"x": 1.0, "y": 0.0}})),
                ("offset", jvalue!({"position": {"x": 0.25, "y": 0.0}})),
            ],
        )?;
        assert_eq!(value_of(&x)?, jvalue!(1.25));

        let mode = call(system, "cpp_mode_next", vec![("mode", jvalue!("Velocity"))])?;
        assert!(!mode.is_native()?);
        assert_eq!(value_of(&mode)?, jvalue!("Position"));
        Ok(())
    })
}

/// いろいろな型のフィールドが C++ の値と辞書の間を往復する
#[test]
fn every_field_type_round_trips() -> JuizResult<()> {
    with_system(|system| {
        let result = call(
            system,
            "cpp_everything_echo",
            vec![(
                "value",
                jvalue!({
                    "header": {"stamp": {"sec": 5}},
                    "data": "AAEC",
                    "extra": {"nested": [true, null, "s"]},
                    "self": 7
                }),
            )],
        )?;
        assert!(result.is_native()?);
        assert_eq!(
            value_of(&result)?,
            jvalue!({
                "flag": true,
                "i": 0,
                "l": 9007199254740994i64,
                "f": 1.5,
                "data": "AAEC/w==",
                "list": [1, 2, 2],
                "table": {"a": 1.0, "b": 2.5},
                "maybe": null,
                "mode": "Position",
                "extra": {"nested": [true, null, "s"]},
                "header": {"frame_id": "map!", "stamp": {"sec": 5, "nanosec": 0}},
                "type": "t?",
                "self": 7
            })
        );
        Ok(())
    })
}

/// ContainerProcess も型付きの引数・戻り値を持てる
#[test]
fn container_process_takes_typed_arguments() -> JuizResult<()> {
    with_system(|system| {
        let cb = system.core_broker().lock()?;
        let id = cb
            .container_process_list(false, None)?
            .into_iter()
            .find(|id| id.name == "scale0")
            .expect("container process scale0 not found");
        let args: CapsuleMap = vec![
            ("pose", jvalue!({"position": {"x": 1.5, "y": 0.0}})),
            ("scale", jvalue!(2.0)),
        ]
        .into();
        let result = cb.container_process_call(&id, args)?;
        assert!(result.is_native()?);
        assert_eq!(value_of(&result)?["position"]["x"], jvalue!(3.0));
        Ok(())
    })
}
