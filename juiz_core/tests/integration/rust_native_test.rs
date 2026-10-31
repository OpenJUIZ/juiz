//! 遅延評価 L2 (issue #176、#192) のテスト
//!
//! 仕様は docs/spec/lazy-evaluation.md §8・§9。Rust の Process が返す record (`#[derive(JuizType)]`) は
//! Native(rust) で包まれ、
//! - 同じプラグインの中の Composite では encode も clone もせずに移る
//! - 同じプラグインの中の接続では encode せずに clone して渡る
//! - 別のプラグインへは ABI タグが違うので辞書を通る
//! ことを確かめる。プラグイン (juiz_sdk/test_packages/native_pose_component、native_pose_reader) は
//! ../target/debug から読む。
//!
//! Native を C ABI 越しに運ぶのは Step 3 (#331)。それまでは C ABI で読むと Native は境界で encode されるので
//! (docs/spec/rust-plugin-abi.md §10、決めごと C)、L2 の性質を確かめるテストはプラグインを conf の
//! `abi: rust` で今の Rust ABI で読む。C ABI で読んだときに値が正しいことは `*_through_c_abi` で確かめる。

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

const COMPONENT_PROCESSES: [&str; 3] =
    ["native_pose_make", "native_pose_move", "native_pose_clones"];

/// abi: プラグインの conf の `abi` ("rust" か "c")
fn conf(abi: &str) -> Value {
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
    let mut processes: Vec<Value> = COMPONENT_PROCESSES.iter().map(|t| instance(t)).collect();
    for t in ["native_pose_reader", "pose_chain", "pose_cross"] {
        processes.push(instance(t));
    }
    jvalue!({
        "name": "rust_native_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "components": { "native_pose_component": { "path": "../target/debug", "abi": abi } },
            "process_factories": { "native_pose_reader": { "path": "../target/debug", "abi": abi } }
        },
        "composites": [
            {
                // move(move(make(x)))。中間値は受け取り手が一つなので移せる
                "type_name": "pose_chain",
                "body": {
                    "process": "native_pose_move",
                    "args": {
                        "dx": { "const": 1.0 },
                        "pose": {
                            "process": "native_pose_move",
                            "args": {
                                "dx": { "const": 1.0 },
                                "pose": { "process": "native_pose_make" }
                            }
                        }
                    }
                }
            },
            {
                // 別のプラグインの reader へ渡す
                "type_name": "pose_cross",
                "body": {
                    "process": "native_pose_reader",
                    "args": { "pose": { "process": "native_pose_make" } }
                }
            }
        ],
        "processes": processes,
        "connections": [
            pull("native_pose_make", "native_pose_move", "pose"),
            pull("native_pose_make", "native_pose_reader", "pose"),
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

fn call(system: &System, type_name: &str, args: Vec<(&str, Value)>) -> JuizResult<CapsulePtr> {
    process(system, type_name)?.lock()?.call(args.into())
}

/// プラグインの中で Pose2D を clone した回数 (読むと 0 に戻る)
fn clones(system: &System) -> JuizResult<i64> {
    call(system, "native_pose_clones", vec![])?.lock_as_value(|v| v.as_i64().unwrap())
}

fn x_of(result: &CapsulePtr) -> JuizResult<f64> {
    result.lock_as_value(|v| v["position"]["x"].as_f64().unwrap())
}

fn with_system(f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    with_system_abi("rust", f)
}

fn with_system_abi(abi: &str, f: impl FnOnce(&mut System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf(abi))?.setup()?.run_and_do_once(f)
}

#[test]
fn manifest_of_plugin_has_schemas() -> JuizResult<()> {
    with_system(|system| {
        let profile = process(system, "native_pose_move")?.lock()?.profile()?;
        let pose = profile.arguments.iter().find(|a| a.name == "pose").unwrap();
        assert_eq!(pose.schema.as_ref().unwrap().name, "robot.geometry.Pose2D");
        assert_eq!(
            profile.output_schema.as_ref().unwrap().name,
            "robot.geometry.Pose2D"
        );
        Ok(())
    })
}

/// 同じプラグインの Composite の中では、中間値を encode も clone もせずに移す
#[test]
fn composite_in_same_plugin_moves_native_values() -> JuizResult<()> {
    with_system(|system| {
        clones(system)?;
        let result = call(system, "pose_chain", vec![("x", jvalue!(1.0))])?;
        assert_eq!(clones(system)?, 0);
        // 戻り値は Native のまま返り、読むと encode される
        assert!(result.is_native()?);
        assert_eq!(
            result.native_cell()?.unwrap().describe(),
            "rust:robot.geometry.Pose2D"
        );
        assert_eq!(x_of(&result)?, 3.0);
        // ABI タグはプラグインごとに違う (このテストのバイナリの juiz_sdk とも違う)
        let abi = result.lock_as_native(|n| n.type_tag().unwrap().abi.clone())?;
        assert!(abi.starts_with("rust:"));
        assert_ne!(abi, juiz_sdk::types::rust_abi_tag());
        Ok(())
    })
}

/// 同じプラグインの接続では encode せずに渡る (source の戻り値を共有するので clone する)
#[test]
fn connection_in_same_plugin_passes_native_values() -> JuizResult<()> {
    with_system(|system| {
        clones(system)?;
        let result = process(system, "native_pose_move")?.lock()?.execute()?;
        assert_eq!(clones(system)?, 1);
        assert!(result.is_native()?);
        assert_eq!(x_of(&result)?, 1.0);
        Ok(())
    })
}

/// 別のプラグインへは辞書を通して渡る
#[test]
fn other_plugin_receives_through_dictionary() -> JuizResult<()> {
    with_system(|system| {
        let result = call(system, "pose_cross", vec![("x", jvalue!(2.5))])?;
        assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 2.5);
        let result = process(system, "native_pose_reader")?.lock()?.execute()?;
        assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 0.0);
        Ok(())
    })
}

/// C ABI で読んだとき (Step 3 #331 まで): Native は境界で encode されるが、値は同じになる
#[test]
fn values_are_the_same_through_c_abi() -> JuizResult<()> {
    with_system_abi("c", |system| {
        let result = call(system, "pose_chain", vec![("x", jvalue!(1.0))])?;
        assert_eq!(x_of(&result)?, 3.0);
        let result = process(system, "native_pose_move")?.lock()?.execute()?;
        assert_eq!(x_of(&result)?, 1.0);
        let result = call(system, "pose_cross", vec![("x", jvalue!(2.5))])?;
        assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 2.5);
        Ok(())
    })
}
