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
//! どのテストも、プラグインを C ABI (既定) と今の Rust ABI (conf の `abi: rust`) の両方で読んで確かめる。
//! C ABI では Native はプラグインの中に置いたまま、ハンドルと表で運ぶ (docs/spec/rust-plugin-abi.md §10、#331)。

use juiz_core::anyhow;
use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;

const COMPONENT_PROCESSES: [&str; 3] =
    ["native_pose_make", "native_pose_move", "native_pose_clones"];

/// プラグインの conf の `abi` ("rust" か "c")。component はコンポーネントの、reader は native_pose_reader の
fn conf(component: &str, reader: &str) -> Value {
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
            "components": { "native_pose_component": { "path": "../target/debug", "abi": component } },
            "process_factories": { "native_pose_reader": { "path": "../target/debug", "abi": reader } }
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

const ABIS: [&str; 2] = ["c", "rust"];

/// C ABI と Rust ABI の両方で f を呼ぶ
/// f には読んだ abi も渡す
fn with_system(f: impl Fn(&mut System, &str) -> JuizResult<()>) -> JuizResult<()> {
    for abi in ABIS {
        with_system_abi(abi, abi, |system| f(system, abi)).map_err(|e| anyhow::anyhow!("abi: {abi}: {e:#}"))?;
    }
    Ok(())
}

fn with_system_abi(
    component: &str,
    reader: &str,
    f: impl Fn(&mut System) -> JuizResult<()>,
) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf(component, reader))?
        .setup()?
        .run_and_do_once(|system| f(system))
}

#[test]
fn manifest_of_plugin_has_schemas() -> JuizResult<()> {
    with_system(|system, _abi| {
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
    with_system(|system, abi| {
        clones(system)?;
        let encodes = juiz_core::rust_cabi_native_encode_count();
        let result = call(system, "pose_chain", vec![("x", jvalue!(1.0))])?;
        assert_eq!(clones(system)?, 0);
        // C ABI でも中間値を encode しない (ハンドルのまま移す)
        assert_eq!(juiz_core::rust_cabi_native_encode_count(), encodes);
        // 戻り値は Native のまま返り、読むと encode される
        assert!(result.is_native()?);
        assert!(!result.native_cell()?.unwrap().is_encoded());
        assert_eq!(
            result.native_cell()?.unwrap().describe(),
            "rust:robot.geometry.Pose2D"
        );
        assert_eq!(x_of(&result)?, 3.0);
        // ABI タグはプラグインごとに違う (このテストのバイナリの juiz_sdk とも違う)
        let tag_abi = result.lock_as_native(|n| n.type_tag().unwrap().abi.clone())?;
        assert!(tag_abi.starts_with("rust:"));
        assert_ne!(tag_abi, juiz_sdk::types::rust_abi_tag());
        // C ABI ではプラグインの中に置いたまま (ホストはハンドルを持つ)
        let in_plugin = result.lock_as_native(|n| n.as_any().is::<juiz_core::RustCabiNative>())?;
        assert_eq!(in_plugin, abi == "c");
        Ok(())
    })
}

/// 同じプラグインの接続では encode せずに渡る (source の戻り値を共有するので clone する)
#[test]
fn connection_in_same_plugin_passes_native_values() -> JuizResult<()> {
    with_system(|system, _abi| {
        clones(system)?;
        let encodes = juiz_core::rust_cabi_native_encode_count();
        let result = process(system, "native_pose_move")?.lock()?.execute()?;
        assert_eq!(clones(system)?, 1);
        assert_eq!(juiz_core::rust_cabi_native_encode_count(), encodes);
        assert!(result.is_native()?);
        assert_eq!(x_of(&result)?, 1.0);
        Ok(())
    })
}

/// 別のプラグインへは辞書を通して渡る
#[test]
fn other_plugin_receives_through_dictionary() -> JuizResult<()> {
    with_system(|system, _abi| {
        let result = call(system, "pose_cross", vec![("x", jvalue!(2.5))])?;
        assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 2.5);
        let result = process(system, "native_pose_reader")?.lock()?.execute()?;
        assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 0.0);
        Ok(())
    })
}

/// C ABI のコンポーネントの Native を Rust ABI の reader へ (と、その逆): 表の encode で辞書を通る
#[test]
fn mixed_abis_pass_through_dictionary() -> JuizResult<()> {
    for (component, reader) in [("c", "rust"), ("rust", "c")] {
        with_system_abi(component, reader, |system| {
            let result = call(system, "pose_cross", vec![("x", jvalue!(2.5))])?;
            assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 2.5);
            let result = process(system, "native_pose_reader")?.lock()?.execute()?;
            assert_eq!(result.lock_as_value(|v| v.as_f64().unwrap())?, 0.0);
            // 同じプラグインの中は、ABI が混ざっても L2 の性質を保つ
            clones(system)?;
            let result = call(system, "pose_chain", vec![("x", jvalue!(1.0))])?;
            assert_eq!(clones(system)?, 0);
            assert_eq!(x_of(&result)?, 3.0);
            Ok(())
        })
        .map_err(|e| anyhow::anyhow!("component: {component}, reader: {reader}: {e:#}"))?;
    }
    Ok(())
}

/// C ABI で返った Native は、encode の結果を覚える (2 度目は表の encode を呼ばない) し、
/// 読んだ後でも同じプラグインへはハンドルのまま渡る
#[test]
fn c_abi_native_is_encoded_once_and_still_passes_as_handle() -> JuizResult<()> {
    with_system_abi("c", "c", |system| {
        let made = call(system, "native_pose_make", vec![("x", jvalue!(4.0))])?;
        assert!(made.is_native()?);
        let encodes = juiz_core::rust_cabi_native_encode_count();
        assert_eq!(x_of(&made)?, 4.0);
        assert_eq!(x_of(&made)?, 4.0);
        assert_eq!(juiz_core::rust_cabi_native_encode_count(), encodes + 1);
        assert!(made.native_cell()?.unwrap().is_encoded());
        clones(system)?;
        // made を持ったまま渡すので共有 (owned = 0)。プラグインの中で 1 回 clone する
        let mut args = CapsuleMap::new();
        args.insert("pose".to_owned(), made.clone());
        args.insert("dx".to_owned(), jvalue!(1.0).into());
        let moved = process(system, "native_pose_move")?.lock()?.call(args)?;
        assert_eq!(clones(system)?, 1);
        assert_eq!(x_of(&moved)?, 5.0);
        assert_eq!(x_of(&made)?, 4.0);
        Ok(())
    })
}
