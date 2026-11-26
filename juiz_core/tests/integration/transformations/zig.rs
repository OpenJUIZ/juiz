//! Zig の ContainerProcess から ContainerPose を読み書きする (zigjuiz の juiz.pose など、issue #574) のテスト。
//! Haskell の transformations::haskell にそろえている
//!
//! 仕様は docs/spec/transformations.md §12.3。数値は §3.5 の例
//! (map → base0 は (1, 0, 0) と z 軸まわり 90°、base0 → camera0 は (0.1, 0, 0.3))。
//! Zig の例は examples/zig の zig_part と zig_part_* (Component は #575 なので、ContainerProcess ごとに別のライブラリ)。
//! scripts/test.sh (CI も同じ) で ../target にビルドされている前提。zig が無い環境では scripts/test.sh が
//! ビルドを飛ばすので、ライブラリが無ければテストも飛ばす。CI では JUIZ_REQUIRE_ZIG=1 にして失敗させる。

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::process_identifier::ProcessIdentifier;

const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

const PROCESSES: [&str; 9] = [
    "zig_part_pose", "zig_part_set_pose", "zig_part_try",
    "zig_part_peek", "zig_part_sleep", "zig_part_lookup", "zig_part_set_transform",
    "zig_part_set_model", "zig_part_meet",
];

fn part(name: &str) -> Value {
    jvalue!({
        "type_name": "zig_part",
        "name": name,
        "processes": PROCESSES.iter().map(|t| jvalue!({ "type_name": t, "name": format!("{name}_{t}") })).collect::<Vec<Value>>(),
    })
}

fn conf() -> Value {
    jvalue!({
        "name": "zig_transformations_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "container_factories": {
                "zig_part": {
                    "language": "zig",
                    "path": "../target",
                    "processes": PROCESSES.iter().map(|t| (t.to_string(), jvalue!({ "path": "../target" }))).collect::<std::collections::HashMap<String, Value>>(),
                }
            }
        },
        "containers": [
            { "type_name": "static_anchor", "name": "map" },
            part("base0"),
            part("camera0"),
            part("lonely0"),
        ],
        "transforms": [
            // lonely0 は端点ではないが set_transform で書いてよい (#410)
            { "source": "map", "destination": "base0",
              "translation": {"x": 1.0, "y": 0.0, "z": 0.0},
              "rotation": {"x": 0.0, "y": 0.0, "z": H, "w": H},
              "writers": ["lonely0"] },
            { "source": "base0", "destination": "camera0",
              "translation": {"x": 0.1, "y": 0.0, "z": 0.3} },
        ],
    })
}

/// Zig の例がビルドされていなければ true (テストを飛ばす)
fn skip_without_zig_example() -> bool {
    let mut names = vec!["zig_part"];
    names.extend(PROCESSES);
    crate::plugins::zig::skip_without_zig_examples(&names)
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

#[test]
fn zig_container_process_reads_and_writes_pose() -> JuizResult<()> {
    if skip_without_zig_example() {
        return Ok(());
    }
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        // 読む (§3.5)
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        // pose_camera0(base0) = Transform(base0→camera0)⁻¹
        let p = call(system, "base0", "zig_part_pose", jvalue!({"reference": "camera0"}))?;
        assert_pose(&p, "camera0", &pose(-0.1, 0.0, -0.3, Quaternion::identity()));

        // 書く: Transform(base0→camera0) が (0.2, 0, 0.3) に書き換わる (§3.5)
        let p = call(
            system,
            "camera0",
            "zig_part_set_pose",
            jvalue!({"reference": "map", "x": 1.0, "y": 0.2, "z": 0.3, "qz": H, "qw": H}),
        )?;
        assert_pose(&p, "map", &pose(1.0, 0.2, 0.3, z90()));
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.2, 0.0, 0.3, Quaternion::identity()));
        // ホストの登録所の値も同じ
        let t = system
            .core_broker()
            .lock()?
            .worker()
            .store()
            .transformations
            .pose_by_name("camera0", "base0", "")?;
        assert!(t.pose.approx_eq(&pose(0.2, 0.0, 0.3, Quaternion::identity()), EPS));
        Ok(())
    })
}

/// juiz.lookup (#409): 自分以外の Container を、自分以外の基準から見て読む。呼び出し元はどの木にも入っていなくてよい
#[test]
fn zig_container_process_looks_up_other_containers() -> JuizResult<()> {
    if skip_without_zig_example() {
        return Ok(());
    }
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let lookup = |caller: &str, target: &str, reference: &str| {
            call(system, caller, "zig_part_lookup", jvalue!({"target": target, "reference": reference}))
        };
        assert_pose(&lookup("lonely0", "camera0", "base0")?, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        assert_pose(&lookup("lonely0", "camera0", "map")?, "map", &pose(1.0, 0.1, 0.3, z90()));
        assert_pose(&lookup("camera0", "base0", "map")?, "map", &pose(1.0, 0.0, 0.0, z90()));
        assert_eq!(lookup("camera0", "lonely0", "map")?, jvalue!("NoCommonAnchor"));
        Ok(())
    })
}

#[test]
fn zig_pose_errors_have_kinds() -> JuizResult<()> {
    if skip_without_zig_example() {
        return Ok(());
    }
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let try_op = |container: &str, op: &str, reference: &str| -> JuizResult<Value> {
            call(system, container, "zig_part_try", jvalue!({"op": op, "reference": reference}))
        };
        assert_eq!(try_op("camera0", "pose", "map")?, jvalue!("ok"));
        // どの木にも入っていない
        assert_eq!(try_op("lonely0", "pose", "map")?, jvalue!("NoCommonAnchor"));
        assert_eq!(try_op("lonely0", "set_pose", "map")?, jvalue!("NoCommonAnchor"));
        // 基準が子孫
        assert_eq!(try_op("base0", "set_pose", "camera0")?, jvalue!("PoseNotWritable"));
        // 長さ 0 の四元数
        assert_eq!(try_op("camera0", "set_pose_zero", "map")?, jvalue!("InvalidPoseValue"));
        // エラーをそのまま返すと、zigjuiz が -1101 と、juiz が返した理由つきのエラー文にする
        let e = call(system, "lonely0", "zig_part_pose", jvalue!({"reference": "map"})).unwrap_err();
        match e.downcast_ref::<JuizError>() {
            Some(JuizError::ZigPluginError { return_value, message, .. }) => {
                assert_eq!(*return_value, -1101, "{message}");
                assert!(message.starts_with("zig_part_pose returned error.NoCommonAnchor: "), "{message}");
                assert!(message.len() > "zig_part_pose returned error.NoCommonAnchor: ".len(), "{message}");
            }
            _ => panic!("unexpected error: {e:#}"),
        }
        Ok(())
    })
}

/// 同じ Container の 2 つの ContainerProcess を 2 つのスレッドから同時に呼び、かかった時間と 2 つの結果を返す
fn call_two_at_once(
    system: &System,
    container: &str,
    (t1, a1): (&str, Value),
    (t2, a2): (&str, Value),
) -> JuizResult<(std::time::Duration, [Value; 2])> {
    let broker = system.core_broker().clone();
    let start = std::time::Instant::now();
    let results = std::thread::scope(|scope| -> JuizResult<[Value; 2]> {
        let calls = [(t1, a1), (t2, a2)].map(|(t, a)| {
            let broker = broker.clone();
            let container = container.to_owned();
            let t = t.to_owned();
            scope.spawn(move || -> JuizResult<Value> {
                let id: ProcessIdentifier =
                    format!("core://core/container_process/{container}_{t}::{t}:{container}").try_into()?;
                let args: CapsuleMap = a.try_into()?;
                let result = broker.lock()?.container_process_call(&id, args)?;
                result.lock_as_value(|v| v.clone())
            })
        });
        let [c1, c2] = calls;
        Ok([
            c1.join().expect("the calling thread panicked")?,
            c2.join().expect("the calling thread panicked")?,
        ])
    })?;
    Ok((start.elapsed(), results))
}

/// 状態を *const で受ける Zig の ContainerProcess は readonly (#377): 同じ Container の readonly 同士は並行に走る。
/// readonly から juiz.setPose / juiz.setTransform を呼ぶのはコンパイルエラー (bindings/zigjuiz/tests/compile_errors/check.sh)
#[test]
fn zig_readonly_container_process() -> JuizResult<()> {
    if skip_without_zig_example() {
        return Ok(());
    }
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let sleep = 300;
        // readonly 同士: 2 つが同時に走っていれば、zig_part_meet の待ち合わせがどちらも成り立つ。
        // かかった時間ではなく重なったかどうかで確かめる (#625、macOS の CI で時間の余裕が足りず落ちた。Rust の #534 と同じ)。
        // 短いほうが先に終わっても、長いほうは終わった後に ContainerPose を読める
        let (_, met) = call_two_at_once(
            system,
            "base0",
            ("zig_part_meet", jvalue!({"reference": "map", "linger_ms": sleep})),
            ("zig_part_meet", jvalue!({"reference": "map"})),
        )?;
        assert_eq!(met, [jvalue!(true), jvalue!(true)], "two readonly calls did not run concurrently");
        // readonly と書き込みのもの: 書き込みロックを待つので 2 回分かかる
        // (直列なら必ず 2 回分以上かかるので、この下限は混んだランナーでも揺れない)
        let (mixed, _) = call_two_at_once(
            system,
            "base0",
            ("zig_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep})),
            ("zig_part_sleep", jvalue!({"sleep_ms": sleep})),
        )?;
        assert!(
            mixed.as_millis() >= (sleep * 2) as u128,
            "a readonly and a writing call took {mixed:?}"
        );

        let p = call(system, "camera0", "zig_part_peek", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        Ok(())
    })
}

/// juiz.setTransform (#410): 端点か writers の Container だけが Transform の値を書ける (平面の x, y, yaw で書く)
#[test]
fn zig_container_process_sets_transforms() -> JuizResult<()> {
    if skip_without_zig_example() {
        return Ok(());
    }
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let set = |caller: &str, source: &str, destination: &str, x: f64, y: f64, yaw: f64| {
            call(
                system,
                caller,
                "zig_part_set_transform",
                jvalue!({"source": source, "destination": destination, "x": x, "y": y, "yaw": yaw}),
            )
        };
        let t = set("lonely0", "map", "base0", 2.0, 0.0, std::f64::consts::FRAC_PI_2)?;
        let t: juiz_sdk::geometry::TransformStamped = (&t).try_into()?;
        assert_eq!((t.source(), t.destination()), ("map", "base0"));
        assert!((t.transform.rotation.z - H).abs() < EPS, "{t:?}");
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(2.0, 0.1, 0.3, z90()));
        set("camera0", "base0", "camera0", 0.2, 0.0, 0.0)?;
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.2, 0.0, 0.0, Quaternion::identity()));
        assert_eq!(set("camera0", "map", "base0", 0.0, 0.0, 0.0)?, jvalue!("TransformNotWritable"));
        assert_eq!(set("lonely0", "base0", "lonely0", 0.0, 0.0, 0.0)?, jvalue!("TransformNotFound"));
        let p = call(system, "camera0", "zig_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(2.0, 0.2, 0.0, z90()));
        Ok(())
    })
}

/// juiz.setModel / juiz.clearModel (#564、container-model.md §13.2): Zig の ContainerProcess から 3D モデルを差し替える・外す。
/// readonly (self が *const) からはコンパイルエラーになるので、ここでは確かめない
#[test]
fn zig_container_process_replaces_model() -> JuizResult<()> {
    if skip_without_zig_example() {
        return Ok(());
    }
    crate::common::logger::init();
    const SDF: &str = r#"<sdf version="1.9"><model name="m"><link name="l"><visual name="v">
<geometry><box><size>0.2 0.1 0.1</size></box></geometry></visual></link></model></sdf>"#;
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let info = call(system, "base0", "zig_part_set_model", jvalue!({"text": SDF, "format": "sdf"}))?;
        assert_eq!(info["format"], "sdf", "{info}");
        assert_eq!(info["file_name"], "zig.sdf");
        assert_eq!(info["color"], "#ffff00");
        assert_eq!(info["sha256"], juiz_core::models::sha256_hex(SDF.as_bytes()));
        assert_eq!(system.core_broker().lock()?.container_model_info("base0")?, info);
        // 値の誤りは error.ContainerModelInvalid で、今のモデルは変えない
        assert_eq!(
            call(system, "base0", "zig_part_set_model", jvalue!({"text": "not a glb", "format": "glb"}))?,
            jvalue!("ContainerModelInvalid")
        );
        assert_eq!(system.core_broker().lock()?.container_model_info("base0")?["sha256"], info["sha256"]);
        // 外す
        assert_eq!(call(system, "base0", "zig_part_set_model", jvalue!({"text": ""}))?, jvalue!({"cleared": true}));
        assert_eq!(system.core_broker().lock()?.container_model_info("base0")?, Value::Null);
        Ok(())
    })
}
