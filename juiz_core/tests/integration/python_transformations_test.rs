//! Python の ContainerProcess から ContainerPose を読み書きする (Transformations Step 4、issue #371) のテスト
//!
//! 仕様は docs/spec/transformations.md §12.2。数値は §3.5 の例
//! (map → base0 は (1, 0, 0) と z 軸まわり 90°、base0 → camera0 は (0.1, 0, 0.3))。
//! Python のファイルは tests/python/py_part*.py。木と Pose 通知は conf の transforms: / pose_listeners: で作る。

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, Vector3};
use juiz_sdk::process_identifier::ProcessIdentifier;

const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;

const PROCESSES: [&str; 5] = [
    "py_part_pose",
    "py_part_set_pose",
    "py_part_try",
    "py_part_on_pose",
    "py_part_last",
];

fn part(name: &str) -> Value {
    jvalue!({
        "type_name": "py_part",
        "name": name,
        "processes": PROCESSES.iter().map(|t| jvalue!({ "type_name": t, "name": format!("{name}_{t}") })).collect::<Vec<Value>>(),
    })
}

fn conf() -> Value {
    let mut processes = serde_json::Map::new();
    for t in PROCESSES {
        processes.insert(t.to_owned(), jvalue!({ "path": "tests/python" }));
    }
    jvalue!({
        "name": "python_transformations_test",
        "option": {
            "pythonpath": ["../../../bindings/pyjuiz"]
        },
        "plugins": {
            "container_factories": {
                "py_part": { "language": "python", "path": "tests/python", "processes": processes }
            }
        },
        "containers": [
            { "type_name": "static_anchor", "name": "map" },
            part("base0"),
            part("camera0"),
            part("lonely0"),
            part("recorder0"),
        ],
        "transforms": [
            { "source": "map", "destination": "base0",
              "translation": {"x": 1.0, "y": 0.0, "z": 0.0},
              "rotation": {"x": 0.0, "y": 0.0, "z": H, "w": H} },
            { "source": "base0", "destination": "camera0",
              "translation": {"x": 0.1, "y": 0.0, "z": 0.3} },
        ],
        "pose_listeners": [
            { "container": "camera0", "process": "recorder0_py_part_on_pose", "arg": "pose", "reference": "map" }
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
fn python_container_process_reads_and_writes_pose() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        // 読む (§3.5)
        let p = call(
            system,
            "camera0",
            "py_part_pose",
            jvalue!({"reference": "map"}),
        )?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        let p = call(
            system,
            "camera0",
            "py_part_pose",
            jvalue!({"reference": "base0"}),
        )?;
        assert_pose(&p, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        let p = call(
            system,
            "base0",
            "py_part_pose",
            jvalue!({"reference": "camera0"}),
        )?;
        // pose_camera0(base0) = Transform(base0→camera0)⁻¹
        assert_pose(
            &p,
            "camera0",
            &pose(-0.1, 0.0, -0.3, Quaternion::identity()),
        );

        // 書く: Transform(base0→camera0) が (0.2, 0, 0.3) に書き換わる
        let p = call(
            system,
            "camera0",
            "py_part_set_pose",
            jvalue!({"reference": "map", "x": 1.0, "y": 0.2, "z": 0.3, "qz": H, "qw": H}),
        )?;
        assert_pose(&p, "map", &pose(1.0, 0.2, 0.3, z90()));
        let p = call(
            system,
            "camera0",
            "py_part_pose",
            jvalue!({"reference": "base0"}),
        )?;
        assert_pose(&p, "base0", &pose(0.2, 0.0, 0.3, Quaternion::identity()));

        // Pose 通知 (conf の pose_listeners:) が Python の ContainerProcess に PoseStamped で届く
        let last = call(system, "recorder0", "py_part_last", jvalue!({}))?;
        assert_pose(&last, "map", &pose(1.0, 0.2, 0.3, z90()));

        // PoseStamped の dict も受け付ける (pose だけを使う)
        let r = call(
            system,
            "camera0",
            "py_part_try",
            jvalue!({"op": "set_pose_stamped_dict", "reference": "base0"}),
        )?;
        assert_eq!(r, jvalue!("ok"));
        let p = call(
            system,
            "camera0",
            "py_part_pose",
            jvalue!({"reference": "base0"}),
        )?;
        assert_pose(&p, "base0", &pose(2.0, 0.0, 0.0, Quaternion::identity()));
        Ok(())
    })
}

#[test]
fn python_pose_errors_have_kinds() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(conf())?.setup()?.run_and_do_once(|system| {
        let try_op = |container: &str, op: &str, reference: &str| -> JuizResult<Value> {
            call(
                system,
                container,
                "py_part_try",
                jvalue!({"op": op, "reference": reference}),
            )
        };
        assert_eq!(try_op("camera0", "pose", "map")?, jvalue!("ok"));
        // どの木にも入っていない
        assert_eq!(
            try_op("lonely0", "pose", "map")?,
            jvalue!("NoCommonAnchorError")
        );
        assert_eq!(
            try_op("lonely0", "set_pose", "map")?,
            jvalue!("NoCommonAnchorError")
        );
        // 基準が子孫
        assert_eq!(
            try_op("base0", "set_pose", "camera0")?,
            jvalue!("PoseNotWritableError")
        );
        // 長さ 0 の四元数
        assert_eq!(
            try_op("camera0", "set_pose_zero", "map")?,
            jvalue!("InvalidPoseValueError")
        );
        // juiz が作った Container のオブジェクトでない
        assert_eq!(
            try_op("camera0", "not_container", "map")?,
            jvalue!("TransformationsNotAttachedError")
        );
        Ok(())
    })
}
