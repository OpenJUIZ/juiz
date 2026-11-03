//! Rust の ContainerProcess から ContainerPose を読み書きする (Transformations Step 5、issue #372) のテスト
//!
//! juiz_sdk/test_packages/pose_part_component を、C ABI (既定) と今の Rust ABI (conf の `abi: rust`) の
//! 両方で読んで確かめる。C ABI では、ホストが引数のフレームに ContainerPose の口を載せ、グルーがプラグインの中の
//! ContainerImpl につなぐ (docs/spec/rust-plugin-abi.md §8.4、transformations.md §12.3)。
//! 数値は transformations.md §3.5 の例。

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::process_identifier::ProcessIdentifier;

const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;
const ABIS: [&str; 2] = ["c", "rust"];

const PROCESSES: [&str; 3] = ["rust_part_pose", "rust_part_set_pose", "rust_part_try"];

fn part(name: &str) -> Value {
    jvalue!({
        "type_name": "rust_part",
        "name": name,
        "processes": PROCESSES.iter().map(|t| jvalue!({ "type_name": t, "name": format!("{name}_{t}") })).collect::<Vec<Value>>(),
    })
}

fn conf(abi: &str) -> Value {
    jvalue!({
        "name": "rust_transformations_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "components": {
                "pose_part_component": { "path": "../target/debug", "abi": abi }
            }
        },
        "containers": [
            { "type_name": "static_anchor", "name": "map" },
            part("base0"),
            part("camera0"),
            part("lonely0"),
        ],
        "transforms": [
            { "source": "map", "destination": "base0",
              "translation": {"x": 1.0, "y": 0.0, "z": 0.0},
              "rotation": {"x": 0.0, "y": 0.0, "z": H, "w": H} },
            { "source": "base0", "destination": "camera0",
              "translation": {"x": 0.1, "y": 0.0, "z": 0.3} },
        ],
    })
}

fn call(system: &System, container: &str, t: &str, args: Value) -> JuizResult<Value> {
    let id: ProcessIdentifier =
        format!("core://core/container_process/{container}_{t}::{t}:{container}").try_into()?;
    let args: CapsuleMap = args.try_into()?;
    let out = system
        .core_broker()
        .lock()?
        .container_process_call(&id, args)?;
    out.lock_as_value(|v| v.clone())
}

fn pose(x: f64, y: f64, z: f64, q: Quaternion) -> Pose {
    Pose::new(Vector3::new(x, y, z), q)
}

fn z90() -> Quaternion {
    Quaternion::new(0.0, 0.0, H, H)
}

fn assert_pose(actual: &Value, reference: &str, expected: &Pose) {
    let p: PoseStamped = actual.try_into().unwrap();
    assert_eq!(p.reference(), reference, "{actual}");
    assert!(
        p.pose.approx_eq(expected, EPS),
        "actual={actual} expected={expected:?}"
    );
}

fn for_each_abi(f: impl Fn(&System, &str) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    for abi in ABIS {
        System::new(conf(abi))?
            .setup()?
            .run_and_do_once(|system| f(system, abi))
            .map_err(|e| juiz_sdk::anyhow::anyhow!("abi: {abi}: {e:#}"))?;
    }
    Ok(())
}

#[test]
fn rust_container_process_reads_and_writes_pose() -> JuizResult<()> {
    for_each_abi(|system, _abi| {
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        let p = call(system, "base0", "rust_part_pose", jvalue!({"reference": "camera0"}))?;
        assert_pose(&p, "camera0", &pose(-0.1, 0.0, -0.3, Quaternion::identity()));

        // 書く: Transform(base0→camera0) が (0.2, 0, 0.3) に書き換わる (§3.5)
        let p = call(
            system,
            "camera0",
            "rust_part_set_pose",
            jvalue!({"reference": "map", "x": 1.0, "y": 0.2, "z": 0.3, "qz": H, "qw": H}),
        )?;
        assert_pose(&p, "map", &pose(1.0, 0.2, 0.3, z90()));
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.2, 0.0, 0.3, Quaternion::identity()));
        Ok(())
    })
}

#[test]
fn rust_pose_errors_have_kinds() -> JuizResult<()> {
    for_each_abi(|system, abi| {
        let try_op = |container: &str, op: &str, reference: &str| -> JuizResult<String> {
            let v = call(system, container, "rust_part_try", jvalue!({"op": op, "reference": reference}))?;
            Ok(v.as_str().unwrap_or_default().to_owned())
        };
        // C ABI ではエラーの種類がコードで運ばれ、プラグインの juiz_sdk の JuizError になる。
        // Rust ABI ではホストの juiz_sdk の JuizError がそのまま渡るが、プラグインの juiz_sdk とは別の型
        // (別にビルドした複製) なので downcast できない (今までと同じ)。そのときは文で確かめる
        let expect = |actual: String, kind: &str, text: &str| {
            if abi == "c" || actual == "ok" {
                assert_eq!(actual, kind);
            } else {
                assert!(actual.starts_with("other: ") && actual.contains(text), "{actual}");
            }
        };
        assert_eq!(try_op("camera0", "pose", "map")?, "ok");
        expect(try_op("lonely0", "pose", "map")?, "NoCommonAnchorError", "No anchor contains");
        expect(try_op("lonely0", "set_pose", "map")?, "NoCommonAnchorError", "No anchor contains");
        expect(try_op("base0", "set_pose", "camera0")?, "PoseNotWritableError", "can not be written");
        // 長さ 0 の四元数はプラグインの中で弾く (どちらの ABI でもプラグインの JuizError)
        assert_eq!(try_op("camera0", "set_pose_zero", "map")?, "InvalidPoseValueError");
        // エラーを返すとホストの呼び出しのエラーになる
        let e = call(system, "lonely0", "rust_part_pose", jvalue!({"reference": "map"})).unwrap_err();
        assert!(format!("{e:#}").contains("lonely0"), "{e:#}");
        Ok(())
    })
}
