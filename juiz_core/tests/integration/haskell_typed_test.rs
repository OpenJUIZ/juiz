//! Haskell のデータ型 (データ型 Step 7、issue #196) のテスト
//!
//! Haskell の Component examples/haskell/src/TypedComponent.hs (typed_component_hs。juiz-idl -g haskell が
//! juiz_idl/test_package/idl/geometry.jidl から作った型と、手で書いた handwritten.Odometry を使う) を ../target から読む。
//!
//! - manifest に型参照 (schema / output_schema) が載り、Rust で作った同じ型と指紋が一致する
//! - 辞書で渡した引数は型で読まれ (足りないフィールドは既定値)、いろいろな型が往復する
//! - Haskell ↔ Rust (ネイティブ値を返す Process) で受け渡せる
//! - 関数とマニフェストの型が合わない Process は読み込むときにエラーになる
//!
//! 仕様は docs/spec/datatype.md §22。Haskell の例は scripts/test.sh (CI も同じ) で ../target にビルドされている前提。
//! GHC が無い環境では scripts/test.sh がビルドを飛ばすので、ライブラリが無ければテストも飛ばす。
//! CI では JUIZ_REQUIRE_HASKELL=1 にして、飛ばさずに失敗させる。

use juiz_core::prelude::*;
use juiz_sdk::{
    process_identifier::ProcessIdentifier,
    types::{DataType, RecordField, RecordType, TypeSchema},
};

const HS_PROCESSES: [&str; 7] = [
    "hs_pose_make",
    "hs_pose_move",
    "hs_pose_x",
    "hs_pose_frame",
    "hs_everything_echo",
    "hs_mode_next",
    "hs_odometry_step",
];

fn plugin_file(name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    let file = format!("lib{name}.dylib");
    #[cfg(target_os = "linux")]
    let file = format!("lib{name}.so");
    #[cfg(target_os = "windows")]
    let file = format!("{name}.dll");
    std::path::PathBuf::from("../target").join(file)
}

/// Haskell の例がビルドされていなければ true (テストを飛ばす)
fn skip_without_haskell_examples() -> bool {
    let missing: Vec<_> = ["typed_component_hs", "broken_typed_process_hs"]
        .iter()
        .map(|n| plugin_file(n))
        .filter(|p| !p.exists())
        .collect();
    if missing.is_empty() {
        return false;
    }
    if std::env::var("JUIZ_REQUIRE_HASKELL").map_or(false, |v| v == "1") {
        panic!("Haskell の例がビルドされていません: {missing:?}");
    }
    eprintln!(
        "skip: Haskell の例がビルドされていません ({missing:?})。GHC と cabal を入れて scripts/test.sh を実行してください。"
    );
    true
}

fn conf() -> Value {
    let instance =
        |t: &str| jvalue!({ "type_name": t, "name": format!("{t}0"), "use_memo": false });
    let mut processes: Vec<Value> = HS_PROCESSES.iter().map(|t| instance(t)).collect();
    for t in [
        "native_pose_make",
        "native_pose_reader",
        "pose_hs_chain",
        "pose_hs_to_rust",
        "pose_rust_to_hs",
    ] {
        processes.push(instance(t));
    }
    let call = |process: &str, args: Value| jvalue!({ "process": process, "args": args });
    jvalue!({
        "name": "haskell_typed_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "components": {
                "typed_component_hs": { "language": "haskell", "path": "../target" },
                "native_pose_component": { "path": "../target/debug" }
            },
            "process_factories": { "native_pose_reader": { "path": "../target/debug" } }
        },
        "composites": [
            {
                "type_name": "pose_hs_chain",
                "body": call("hs_pose_x", jvalue!({
                    "pose": call("hs_pose_move", jvalue!({
                        "pose": { "process": "hs_pose_make" },
                        "dx": { "const": 1.0 }
                    })),
                    "offset": { "const": { "position": { "x": 0.0, "y": 0.0 } } }
                }))
            },
            {
                "type_name": "pose_hs_to_rust",
                "body": call("native_pose_reader", jvalue!({ "pose": { "process": "hs_pose_make" } }))
            },
            {
                // Rust のネイティブ値は辞書にして Haskell に渡る
                "type_name": "pose_rust_to_hs",
                "body": call("hs_pose_frame", jvalue!({ "pose": { "process": "native_pose_make" } }))
            }
        ],
        "processes": processes,
        "containers": [
            {
                "type_name": "hs_pose_holder",
                "name": "holder0",
                "initial": { "position": { "x": 9.0, "y": 0.0 } },
                "processes": [
                    { "type_name": "hs_pose_holder_scale", "name": "scale0" },
                    { "type_name": "hs_pose_holder_last", "name": "last0" }
                ]
            }
        ]
    })
}

fn with_system(f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
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

fn call_value(system: &System, type_name: &str, args: Vec<(&str, Value)>) -> JuizResult<Value> {
    value_of(&call(system, type_name, args)?)
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
        let profile = process(system, "hs_pose_move")?.lock()?.profile()?;
        let pose = profile.arguments.iter().find(|a| a.name == "pose").unwrap();
        let schema = pose.schema.as_ref().unwrap();
        assert_eq!(schema.name, "robot.geometry.Pose2D");
        assert_eq!(schema.fingerprint, expected.fingerprint);
        assert_eq!(pose.type_name.as_str(), "Object");
        // 既定値は Haskell の値を辞書にしたもの
        assert_eq!(
            pose.default,
            jvalue!({"position": {"x": 0.0, "y": 0.0}, "theta": 0.0, "frame": "map", "comment": null})
        );
        assert_eq!(
            profile.output_schema.as_ref().unwrap().fingerprint,
            expected.fingerprint
        );
        schema.validate().unwrap();

        // 結果が型付きの型なら、returns を書かなくても output_schema が載る
        let profile = process(system, "hs_pose_make")?.lock()?.profile()?;
        assert_eq!(
            profile.output_schema.as_ref().unwrap().fingerprint,
            expected.fingerprint
        );

        // 列挙は type_name が String
        let profile = process(system, "hs_mode_next")?.lock()?.profile()?;
        assert_eq!(profile.arguments[0].type_name.as_str(), "String");
        assert_eq!(profile.arguments[0].default, jvalue!("Idle"));
        assert_eq!(
            profile.output_schema.as_ref().unwrap().name,
            "robot.geometry.MotorMode"
        );

        // 手で書いた型
        let profile = process(system, "hs_odometry_step")?.lock()?.profile()?;
        assert_eq!(
            profile.arguments[0].schema.as_ref().unwrap().name,
            "handwritten.Odometry"
        );
        // 結果が Double の Process は output_schema なし
        let profile = process(system, "hs_pose_x")?.lock()?.profile()?;
        assert!(profile.output_schema.is_none());
        Ok(())
    })
}

/// 辞書で渡した引数は型で読まれ、足りないフィールドは既定値になる
#[test]
fn dictionary_arguments_are_read_with_type() -> JuizResult<()> {
    with_system(|system| {
        let frame = |pose: Value| -> JuizResult<Value> {
            call_value(system, "hs_pose_frame", vec![("pose", pose)])
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
        // 型に合わない値はエラー (プロセスは落ちない)
        assert!(call(
            system,
            "hs_pose_frame",
            vec![("pose", jvalue!({"position": "x"}))]
        )
        .is_err());

        assert_eq!(
            call_value(
                system,
                "hs_pose_x",
                vec![
                    ("pose", jvalue!({"position": {"x": 1.0, "y": 0.0}})),
                    ("offset", jvalue!({"position": {"x": 0.25, "y": 0.0}})),
                ],
            )?,
            jvalue!(1.25)
        );
        // 戻り値の record は辞書で返る
        let moved = call(
            system,
            "hs_pose_move",
            vec![
                (
                    "pose",
                    jvalue!({"position": {"x": 1.0, "y": 2.0}, "theta": 0.5}),
                ),
                ("dx", jvalue!(2.0)),
            ],
        )?;
        assert!(!moved.is_native()?);
        assert_eq!(
            value_of(&moved)?,
            jvalue!({"position": {"x": 3.0, "y": 2.0}, "theta": 0.5, "frame": "map", "comment": null})
        );

        assert_eq!(
            call_value(system, "hs_mode_next", vec![("mode", jvalue!("Velocity"))])?,
            jvalue!("Position")
        );
        // 知らない記号は列挙の既定値 (Idle) として読む (仕様 §4)
        assert_eq!(
            call_value(system, "hs_mode_next", vec![("mode", jvalue!("Unknown"))])?,
            jvalue!("Velocity")
        );

        assert_eq!(
            call_value(
                system,
                "hs_odometry_step",
                vec![("odometry", jvalue!({"distance": 1.5}))]
            )?,
            jvalue!({"distance": 2.5, "steps": 1})
        );
        Ok(())
    })
}

/// いろいろな型のフィールドが Haskell の値と辞書の間を往復する (C++ の cpp_everything_echo と同じ結果)
#[test]
fn every_field_type_round_trips() -> JuizResult<()> {
    with_system(|system| {
        let result = call_value(
            system,
            "hs_everything_echo",
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
        assert_eq!(
            result,
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

/// Haskell の Process どうし、Haskell ↔ Rust の Process の間で型付きの値を受け渡せる
#[test]
fn haskell_and_rust_exchange_typed_values() -> JuizResult<()> {
    with_system(|system| {
        assert_eq!(
            call_value(system, "pose_hs_chain", vec![("x", jvalue!(0.5))])?,
            jvalue!(1.5)
        );
        assert_eq!(
            call_value(system, "pose_hs_to_rust", vec![("x", jvalue!(2.5))])?,
            jvalue!(2.5)
        );
        // Rust の Pose2D には frame・comment が無いが、Haskell 側で既定値になる
        assert_eq!(
            call_value(system, "pose_rust_to_hs", vec![("x", jvalue!(1.5))])?,
            jvalue!("map/none")
        );
        Ok(())
    })
}

/// Container を作る関数と ContainerProcess も型付きの引数を持てる
#[test]
fn container_takes_typed_arguments() -> JuizResult<()> {
    with_system(|system| {
        let cb = system.core_broker().lock()?;
        let find = |name: &str| {
            cb.container_process_list(false, None)
                .unwrap()
                .into_iter()
                .find(|id| id.name == name)
                .unwrap_or_else(|| panic!("container process {name} not found"))
        };
        let (scale0, last0) = (find("scale0"), find("last0"));
        let last = || -> JuizResult<Value> {
            cb.container_process_call(&last0, CapsuleMap::new())?
                .lock_as_value(|v| v.clone())
        };
        // 設定ファイルの initial (足りないフィールドは既定値で読む)
        assert_eq!(last()?, jvalue!("9.0"));
        let args: CapsuleMap = vec![
            ("pose", jvalue!({"position": {"x": 1.5, "y": 0.0}})),
            ("scale", jvalue!(2.0)),
        ]
        .into();
        let result = cb.container_process_call(&scale0, args)?;
        assert_eq!(value_of(&result)?["position"]["x"], jvalue!(3.0));
        assert_eq!(last()?, jvalue!("1.5"));
        let profile = cb.container_process_profile_full(&scale0)?;
        assert_eq!(
            profile.output_schema.as_ref().unwrap().name,
            "robot.geometry.Pose2D"
        );
        Ok(())
    })
}

/// 関数は Pose2D を受け取るのにマニフェストが型なしの引数の Process は、読み込むときにエラーになる
#[test]
fn mismatched_typed_process_is_rejected() -> JuizResult<()> {
    crate::common::logger::init();
    if skip_without_haskell_examples() {
        return Ok(());
    }
    let r = System::new(jvalue!({
        "name": "haskell_broken_typed_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "process_factories": {
                "broken_typed_process_hs": { "language": "haskell", "path": "../target" }
            }
        }
    }))?
    .setup();
    assert!(r.is_err());
    Ok(())
}
