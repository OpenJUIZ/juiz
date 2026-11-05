//! C++ の ContainerProcess から ContainerPose を読み書きする (Transformations Step 5、issue #372) のテスト
//!
//! 仕様は docs/spec/transformations.md §12.3。数値は §3.5 の例
//! (map → base0 は (1, 0, 0) と z 軸まわり 90°、base0 → camera0 は (0.1, 0, 0.3))。
//! C++ のコンポーネントは examples/cpp/component/cpp_pose_component。scripts/test.sh (CI も同じ) で
//! ../target/debug にビルドされている前提。

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::process_identifier::ProcessIdentifier;

const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

const PROCESSES: [&str; 8] = [
    "cpp_part_pose",
    "cpp_part_set_pose",
    "cpp_part_try",
    "cpp_part_pose_unchecked",
    "cpp_part_peek",
    "cpp_part_sleep",
    "cpp_part_try_readonly",
    "cpp_part_lookup",
];

fn part(name: &str) -> Value {
    jvalue!({
        "type_name": "cpp_part",
        "name": name,
        "processes": PROCESSES.iter().map(|t| jvalue!({ "type_name": t, "name": format!("{name}_{t}") })).collect::<Vec<Value>>(),
    })
}

fn conf() -> Value {
    jvalue!({
        "name": "cpp_transformations_test",
        "option": { "http_broker": { "start": false } },
        "plugins": {
            "components": {
                "cpp_pose_component": { "language": "c++", "path": "../target/debug" }
            }
        },
        "processes": [
            { "type_name": "cpp_pose_angles", "name": "angles0" },
        ],
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

#[test]
fn cpp_container_process_reads_and_writes_pose() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        // 読む (§3.5)
        let p = call(system, "camera0", "cpp_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        let p = call(system, "camera0", "cpp_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        // pose_camera0(base0) = Transform(base0→camera0)⁻¹
        let p = call(system, "base0", "cpp_part_pose", jvalue!({"reference": "camera0"}))?;
        assert_pose(&p, "camera0", &pose(-0.1, 0.0, -0.3, Quaternion::identity()));

        // 書く: Transform(base0→camera0) が (0.2, 0, 0.3) に書き換わる (§3.5)
        let p = call(
            system,
            "camera0",
            "cpp_part_set_pose",
            jvalue!({"reference": "map", "x": 1.0, "y": 0.2, "z": 0.3, "qz": H, "qw": H}),
        )?;
        assert_pose(&p, "map", &pose(1.0, 0.2, 0.3, z90()));
        let p = call(system, "camera0", "cpp_part_pose", jvalue!({"reference": "base0"}))?;
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

#[test]
fn cpp_pose_errors_have_kinds() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let try_op = |container: &str, op: &str, reference: &str| -> JuizResult<Value> {
            call(system, container, "cpp_part_try", jvalue!({"op": op, "reference": reference}))
        };
        assert_eq!(try_op("camera0", "pose", "map")?, jvalue!("ok"));
        // どの木にも入っていない
        assert_eq!(try_op("lonely0", "pose", "map")?, jvalue!("NoCommonAnchorError"));
        assert_eq!(try_op("lonely0", "set_pose", "map")?, jvalue!("NoCommonAnchorError"));
        // 基準が子孫
        assert_eq!(try_op("base0", "set_pose", "camera0")?, jvalue!("PoseNotWritableError"));
        // 長さ 0 の四元数
        assert_eq!(try_op("camera0", "set_pose_zero", "map")?, jvalue!("InvalidPoseValueError"));
        // 呼ばれている ContainerProcess の Container ではないオブジェクト
        assert_eq!(
            try_op("camera0", "other_object", "map")?,
            jvalue!("TransformationsNotAttachedError")
        );
        // 例外を外に出すと、entry point がコードにして返す
        let e = call(system, "lonely0", "cpp_part_pose_unchecked", jvalue!({"reference": "map"}))
            .unwrap_err();
        assert!(format!("{e:#}").contains("-902"), "{e:#}");
        Ok(())
    })
}

/// 同じ Container の 2 つの ContainerProcess を 2 つのスレッドから同時に呼び、かかった時間を返す
fn call_two_at_once(
    system: &System,
    container: &str,
    (t1, a1): (&str, Value),
    (t2, a2): (&str, Value),
) -> JuizResult<std::time::Duration> {
    let broker = system.core_broker().clone();
    let start = std::time::Instant::now();
    std::thread::scope(|scope| -> JuizResult<()> {
        let calls = [(t1, a1), (t2, a2)].map(|(t, a)| {
            let broker = broker.clone();
            let container = container.to_owned();
            let t = t.to_owned();
            scope.spawn(move || -> JuizResult<()> {
                let id: ProcessIdentifier =
                    format!("core://core/container_process/{container}_{t}::{t}:{container}").try_into()?;
                let args: CapsuleMap = a.try_into()?;
                broker.lock()?.container_process_call(&id, args)?;
                Ok(())
            })
        });
        for c in calls {
            c.join().expect("the calling thread panicked")?;
        }
        Ok(())
    })?;
    Ok(start.elapsed())
}

/// readonly() の C++ の ContainerProcess (#377): 同じ Container の readonly 同士は並行に走り、
/// 書き込みの口は JUIZ_POSE_NOT_WRITABLE_ERROR を返す
#[test]
fn cpp_readonly_container_process() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let sleep = 300;
        let both_readonly = call_two_at_once(
            system,
            "base0",
            ("cpp_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep})),
            ("cpp_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep / 2})),
        )?;
        assert!(
            both_readonly.as_millis() < (sleep + sleep / 2 - 30) as u128,
            "two readonly calls took {both_readonly:?}"
        );
        let mixed = call_two_at_once(
            system,
            "base0",
            ("cpp_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep})),
            ("cpp_part_sleep", jvalue!({"sleep_ms": sleep})),
        )?;
        assert!(
            mixed.as_millis() >= (sleep * 2) as u128,
            "a readonly and a writing call took {mixed:?}"
        );

        let p = call(system, "camera0", "cpp_part_peek", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        assert_eq!(
            call(system, "camera0", "cpp_part_try_readonly", jvalue!({"reference": "map"}))?,
            jvalue!("PoseNotWritableError")
        );
        let p = call(system, "camera0", "cpp_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        Ok(())
    })
}

/// juiz::lookup (#409): 自分以外の Container を、自分以外の基準から見て読む。呼び出し元はどの木にも入っていなくてよい
#[test]
fn cpp_container_process_looks_up_other_containers() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let lookup = |caller: &str, target: &str, reference: &str| {
            call(system, caller, "cpp_part_lookup", jvalue!({"target": target, "reference": reference}))
        };
        assert_pose(&lookup("lonely0", "camera0", "base0")?, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        assert_pose(&lookup("lonely0", "camera0", "map")?, "map", &pose(1.0, 0.1, 0.3, z90()));
        assert_pose(&lookup("camera0", "base0", "map")?, "map", &pose(1.0, 0.0, 0.0, z90()));
        assert_eq!(lookup("camera0", "lonely0", "map")?, jvalue!("NoCommonAnchorError"));
        Ok(())
    })
}

/// juiz/pose.h の四元数と角度の変換 (#412) が Rust の Quaternion::from_rpy / to_rpy と同じ値になる
#[test]
fn cpp_pose_angle_helpers_match_rust() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let id = system
            .core_broker()
            .lock()?
            .process_list(false, None)?
            .into_iter()
            .find(|id| id.name == "angles0")
            .expect("angles0 not found");
        let call_angles = |roll: f64, pitch: f64, yaw: f64, x: f64, y: f64| -> JuizResult<Value> {
            let args: CapsuleMap = jvalue!({"roll": roll, "pitch": pitch, "yaw": yaw, "x": x, "y": y}).try_into()?;
            system.core_broker().lock()?.process_call(&id, args)?.lock_as_value(|v| v.clone())
        };
        let f = |v: &Value, path: &[&str]| -> f64 {
            let mut v = v;
            for k in path {
                v = &v[*k];
            }
            v.as_f64().unwrap_or_else(|| panic!("{path:?} is not a number: {v}"))
        };
        for (roll, pitch, yaw) in [
            (0.0, 0.0, 0.0),
            (0.1, -0.2, 0.3),
            (0.0, 0.0, std::f64::consts::FRAC_PI_2),
            (-1.0, 0.5, 3.0),
            (0.3, 1.2, -2.5),
        ] {
            let r = call_angles(roll, pitch, yaw, 1.5, -2.0)?;
            let q = Quaternion::from_rpy(roll, pitch, yaw);
            for (k, expected) in [("x", q.x), ("y", q.y), ("z", q.z), ("w", q.w)] {
                assert!((f(&r, &["quaternion", k]) - expected).abs() < EPS, "{k}: {r}");
            }
            let (er, ep, ey) = q.to_rpy();
            assert!((f(&r, &["rpy", "roll"]) - er).abs() < EPS, "{r}");
            assert!((f(&r, &["rpy", "pitch"]) - ep).abs() < EPS, "{r}");
            assert!((f(&r, &["rpy", "yaw"]) - ey).abs() < EPS, "{r}");
            // 往復で元に戻る (pitch が ±90° でない範囲)
            assert!((er - roll).abs() < EPS && (ep - pitch).abs() < EPS && (ey - yaw).abs() < EPS, "{r}");
            assert!((f(&r, &["rpy_of_pose", "roll"]) - er).abs() < EPS, "{r}");

            // make_pose_2d(x, y, yaw) は z = 0、z 軸まわりの回転
            let q2 = Quaternion::from_rpy(0.0, 0.0, yaw);
            assert!((f(&r, &["pose_2d", "position", "x"]) - 1.5).abs() < EPS, "{r}");
            assert!((f(&r, &["pose_2d", "position", "y"]) + 2.0).abs() < EPS, "{r}");
            assert_eq!(f(&r, &["pose_2d", "position", "z"]), 0.0, "{r}");
            for (k, expected) in [("x", q2.x), ("y", q2.y), ("z", q2.z), ("w", q2.w)] {
                assert!((f(&r, &["pose_2d", "orientation", k]) - expected).abs() < EPS, "{k}: {r}");
            }
            assert!((f(&r, &["yaw_2d"]) - yaw).abs() < EPS, "{r}");
        }
        Ok(())
    })
}
