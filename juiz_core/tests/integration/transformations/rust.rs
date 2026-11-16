//! Rust の ContainerProcess から ContainerPose を読み書きする (Transformations Step 5、issue #372) のテスト
//!
//! juiz_sdk/test_packages/pose_part_component を、C ABI (既定) と今の Rust ABI (conf の `abi: rust`) の
//! 両方で読んで確かめる。C ABI では、ホストが引数のフレームに ContainerPose の口を載せ、グルーがプラグインの中の
//! ContainerImpl につなぐ (docs/spec/rust-plugin-abi.md §8.4、transformations.md §12.3)。
//! 数値は transformations.md §3.5 の例。

use juiz_core::prelude::*;
use juiz_sdk::geometry::{Pose, PoseStamped, Quaternion, TransformStamped, Vector3};
use juiz_sdk::process_identifier::ProcessIdentifier;

const EPS: f64 = 1e-9;
const H: f64 = std::f64::consts::FRAC_1_SQRT_2;
const ABIS: [&str; 2] = ["c", "rust"];

const PROCESSES: [&str; 8] = [
    "rust_part_pose",
    "rust_part_set_pose",
    "rust_part_try",
    "rust_part_peek",
    "rust_part_sleep",
    "rust_part_try_readonly",
    "rust_part_lookup",
    "rust_part_set_transform",
];

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

/// lookup (#409): 自分以外の Container を、自分以外の基準から見て読む。呼び出し元はどの木にも入っていなくてよい
#[test]
fn rust_container_process_looks_up_other_containers() -> JuizResult<()> {
    for_each_abi(|system, abi| {
        let lookup = |caller: &str, target: &str, reference: &str| {
            call(system, caller, "rust_part_lookup", jvalue!({"target": target, "reference": reference}))
        };
        assert_pose(&lookup("lonely0", "camera0", "base0")?, "base0", &pose(0.1, 0.0, 0.3, Quaternion::identity()));
        assert_pose(&lookup("lonely0", "camera0", "map")?, "map", &pose(1.0, 0.1, 0.3, z90()));
        assert_pose(&lookup("camera0", "base0", "map")?, "map", &pose(1.0, 0.0, 0.0, z90()));
        // ID でも指定できる
        assert_pose(
            &lookup("lonely0", "core://core/container/camera0::rust_part", "base0")?,
            "base0",
            &pose(0.1, 0.0, 0.3, Quaternion::identity()),
        );
        // 木の外れ。Rust ABI ではホストの JuizError をプラグインで downcast できない (rust_pose_errors_have_kinds と同じ) ので、
        // エラーのまま返ってきたものを文で確かめる
        match lookup("camera0", "lonely0", "map") {
            Ok(v) => assert_eq!(v, jvalue!("NoCommonAnchorError"), "abi: {abi}"),
            Err(e) => {
                assert_eq!(abi, "rust", "{e:#}");
                assert!(format!("{e:#}").contains("No anchor contains"), "{e:#}");
            }
        }
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

/// readonly の ContainerProcess (#377) は Container を読み出しロックで呼ぶので、同じ Container の readonly 同士は
/// 並行に走る。readonly でないものとは並行に走らない
#[test]
fn rust_readonly_container_processes_run_concurrently() -> JuizResult<()> {
    for_each_abi(|system, abi| {
        let sleep = 300;
        // readonly 同士: 長いほう 1 回分の時間で終わる。短いほうが先に終わっても、長いほうは
        // 終わった後に ContainerPose を読める (C ABI で口を外すのは最後の呼び出しが終わったとき)
        let both_readonly = call_two_at_once(
            system,
            "base0",
            ("rust_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep})),
            ("rust_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep / 2})),
        )?;
        assert!(
            both_readonly.as_millis() < (sleep + sleep / 2 - 30) as u128,
            "abi={abi}: two readonly calls took {both_readonly:?}"
        );
        // readonly と書き込みのもの: 書き込みロックを待つので 2 回分かかる
        let mixed = call_two_at_once(
            system,
            "base0",
            ("rust_part_peek", jvalue!({"reference": "map", "sleep_ms": sleep})),
            ("rust_part_sleep", jvalue!({"sleep_ms": sleep})),
        )?;
        assert!(
            mixed.as_millis() >= (sleep * 2) as u128,
            "abi={abi}: a readonly and a writing call took {mixed:?}"
        );
        // readonly の中でも ContainerPose は読める
        let p = call(system, "camera0", "rust_part_peek", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        Ok(())
    })
}

/// readonly の ContainerProcess からの ContainerPose への書き込みは PoseNotWritableError (#377)
#[test]
fn rust_readonly_container_process_can_not_write_pose() -> JuizResult<()> {
    for_each_abi(|system, abi| {
        let try_op = |op: &str| -> JuizResult<String> {
            let v = call(system, "camera0", "rust_part_try_readonly", jvalue!({"op": op, "reference": "map"}))?;
            Ok(v.as_str().unwrap_or_default().to_owned())
        };
        assert_eq!(try_op("pose")?, "ok");
        let actual = try_op("set_pose")?;
        // Rust ABI ではホストの juiz_sdk の JuizError が渡るので、プラグインからは downcast できない (上と同じ)
        if abi == "c" {
            assert_eq!(actual, "PoseNotWritableError");
        } else {
            assert!(actual == "PoseNotWritableError" || actual.contains("readonly"), "{actual}");
        }
        // 書き込みは何も変えていない
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(1.0, 0.1, 0.3, z90()));
        // readonly の呼び出しが終われば、普通の ContainerProcess からは書ける
        let p = call(
            system,
            "camera0",
            "rust_part_set_pose",
            jvalue!({"reference": "map", "x": 1.0, "y": 0.2, "z": 0.3, "qz": H, "qw": H}),
        )?;
        assert_pose(&p, "map", &pose(1.0, 0.2, 0.3, z90()));
        Ok(())
    })
}

/// manifest と profile に readonly が載る (#377)
#[test]
fn rust_readonly_is_shown_in_profile() -> JuizResult<()> {
    for_each_abi(|system, abi| {
        let profile = |t: &str| -> JuizResult<ProcessProfile> {
            let id: ProcessIdentifier =
                format!("core://core/container_process/base0_{t}::{t}:base0").try_into()?;
            system.core_broker().lock()?.worker().store().container_processes.get(&id.to_string())?.lock()?.profile()
        };
        let p = profile("rust_part_peek")?;
        assert!(p.readonly, "abi={abi}: {p}");
        // JSON (API・CLI・管理画面が見るもの) にも載る
        assert_eq!(serde_json::to_value(&p)?["readonly"], jvalue!(true));
        let p = profile("rust_part_sleep")?;
        assert!(!p.readonly, "abi={abi}: {p}");
        assert!(serde_json::to_value(&p)?.get("readonly").is_none());
        Ok(())
    })
}

/// set_transform (#410): 端点か writers の Container だけが Transform の値を書ける。結果は transform/update と同じ
#[test]
fn rust_container_process_sets_transforms() -> JuizResult<()> {
    for_each_abi(|system, abi| {
        let set = |caller: &str, source: &str, destination: &str, v: Value| {
            let mut args = v;
            args["source"] = jvalue!(source);
            args["destination"] = jvalue!(destination);
            call(system, caller, "rust_part_set_transform", args)
        };
        // writers の lonely0 が map → base0 を書く。camera0 は base0 の子なので一緒に動く
        let t = set("lonely0", "map", "base0", jvalue!({"x": 2.0, "qz": H, "qw": H}))?;
        let t: TransformStamped = (&t).try_into()?;
        assert_eq!((t.source(), t.destination()), ("map", "base0"));
        assert!((t.transform.translation.x - 2.0).abs() < EPS);
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(2.0, 0.1, 0.3, z90()));
        // 端点の camera0 は base0 → camera0 を書ける。値は正規化される
        let t = set("camera0", "base0", "camera0", jvalue!({"x": 0.2, "z": 0.3, "qw": 2.0}))?;
        let t: TransformStamped = (&t).try_into()?;
        assert!((t.transform.rotation.w - 1.0).abs() < EPS);
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "base0"}))?;
        assert_pose(&p, "base0", &pose(0.2, 0.0, 0.3, Quaternion::identity()));

        // 端点でも writers でもない / 無い Transform。Rust ABI ではホストの JuizError を downcast できないので文で確かめる
        let expect = |r: JuizResult<Value>, kind: &str, text: &str| match r {
            Ok(v) => assert_eq!(v, jvalue!(kind), "abi: {abi}"),
            Err(e) => {
                assert_eq!(abi, "rust", "{e:#}");
                assert!(format!("{e:#}").contains(text), "{e:#}");
            }
        };
        expect(set("camera0", "map", "base0", jvalue!({})), "TransformNotWritableError", "can not be written by");
        expect(set("lonely0", "base0", "lonely0", jvalue!({})), "TransformNotFoundError", "is not found");
        // 書けなかった値は変わらない
        let p = call(system, "camera0", "rust_part_pose", jvalue!({"reference": "map"}))?;
        assert_pose(&p, "map", &pose(2.0, 0.2, 0.3, z90()));

        // readonly の中からは書けない (§14)
        let r = call(system, "base0", "rust_part_try_readonly", jvalue!({"op": "set_transform", "reference": "map"}))?;
        let r = r.as_str().unwrap_or_default();
        if abi == "c" {
            assert_eq!(r, "TransformNotWritableError");
        } else {
            assert!(r == "TransformNotWritableError" || r.contains("readonly"), "{r}");
        }
        Ok(())
    })
}
