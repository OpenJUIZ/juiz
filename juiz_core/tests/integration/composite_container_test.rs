//! スロットを持つ Composite (CompositeContainer と CompositeContainerProcess、issue #168) のテスト
//!
//! 仕様は docs/spec/composition.md の §7。
//! conf を使わずに、コードから Container・ContainerProcess の Factory を登録し、
//! CompositeContainer と CompositeContainerProcess を作って動かす。

use std::collections::BTreeMap;

use juiz_core::anyhow;
use juiz_core::prelude::*;
use juiz_sdk::factory::{container_factory, container_process_factory};
use juiz_sdk::manifests::{
    CompositeManifest, CompositeNode, COMPOSITE_CONTAINER_PROCESS_FACTORY_NAME,
    COMPOSITE_CONTAINER_TYPE_NAME,
};

/// センサ。読むたびに reads が増える
struct Sensor {
    value: i64,
    reads: i64,
}

/// モータ。書いた指令を覚える
struct Motor {
    history: Vec<i64>,
}

/// 足し込むカウンタ
struct Counter {
    count: i64,
}

fn int_arg(args: &CapsuleMap, name: &str) -> JuizResult<i64> {
    args.get(name)?.lock_as_value(|v| v.as_i64().unwrap())
}

fn sensor_read(c: &mut ContainerImpl<Sensor>, _args: CapsuleMap) -> JuizResult<Capsule> {
    c.reads += 1;
    if c.value < 0 {
        return Err(anyhow::anyhow!(JuizError::InvalidArgumentError {
            message: format!("sensor broken ({})", c.value),
        }));
    }
    Ok(jvalue!(c.value).into())
}

fn motor_write(c: &mut ContainerImpl<Motor>, args: CapsuleMap) -> JuizResult<Capsule> {
    let command = int_arg(&args, "command")?;
    c.history.push(command);
    Ok(jvalue!(command).into())
}

fn counter_add(c: &mut ContainerImpl<Counter>, args: CapsuleMap) -> JuizResult<Capsule> {
    c.count += int_arg(&args, "arg1")?;
    Ok(jvalue!(c.count).into())
}

fn scale(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(int_arg(&args, "arg1")? * int_arg(&args, "factor")?).into())
}

fn increment(args: CapsuleMap) -> JuizResult<Capsule> {
    Ok(jvalue!(int_arg(&args, "arg1")? + 1).into())
}

fn container_process_manifest(type_name: &str, container_type: &str) -> ProcessManifest {
    ProcessManifest::new(type_name).container(ContainerManifest::new(container_type))
}

/// テスト用の Factory を System に登録する
fn register_factories(worker: &mut CoreWorker) -> JuizResult<()> {
    let store = worker.store_mut();
    let containers: Vec<juiz_sdk::factory::ContainerFactoryStruct> = vec![
        container_factory(
            ContainerManifest::new("sensor").add_int_arg("value", "value", 5),
            |args: CapsuleMap| {
                Ok(Box::new(Sensor {
                    value: int_arg(&args, "value")?,
                    reads: 0,
                }))
            },
        ),
        container_factory(ContainerManifest::new("motor"), |_args: CapsuleMap| {
            Ok(Box::new(Motor {
                history: Vec::new(),
            }))
        }),
        container_factory(ContainerManifest::new("counter"), |_args: CapsuleMap| {
            Ok(Box::new(Counter { count: 0 }))
        }),
    ];
    for juiz_sdk::factory::ContainerFactoryStruct(manifest, constructor) in containers {
        let type_name = manifest.type_name.clone();
        store
            .containers
            .register_factory(&type_name, container_factory_create(manifest, constructor)?)?;
    }
    let container_processes = vec![
        container_process_factory(
            container_process_manifest("sensor_read", "sensor"),
            sensor_read,
        ),
        container_process_factory(
            container_process_manifest("motor_write", "motor").add_int_arg("command", "command", 0),
            motor_write,
        ),
        container_process_factory(
            container_process_manifest("counter_add", "counter").add_int_arg("arg1", "amount", 1),
            counter_add,
        ),
    ];
    for juiz_sdk::factory::ContainerProcessFactoryStruct(manifest, function) in container_processes
    {
        let type_name = manifest.type_name.clone();
        store.container_processes.register_factory(
            &type_name,
            container_process_factory_create(manifest, function)?,
        )?;
    }
    for (manifest, function) in [
        (
            ProcessManifest::new("scale")
                .add_int_arg("arg1", "input", 1)
                .add_int_arg("factor", "factor", 2),
            scale as fn(CapsuleMap) -> JuizResult<Capsule>,
        ),
        (
            ProcessManifest::new("increment").add_int_arg("arg1", "input", 1),
            increment,
        ),
    ] {
        let type_name = manifest.type_name.clone();
        store
            .processes
            .register_factory(&type_name, process_factory_create(manifest, function)?)?;
    }
    Ok(())
}

fn node(process: &str) -> CompositeNode {
    CompositeNode::new(process)
}

/// motor_write(motor, command = scale(arg1 = sensor_read(sensor), factor))
fn sense_scale_drive() -> CompositeManifest {
    CompositeManifest::new(
        "sense_scale_drive",
        node("motor_write").container("motor").arg_node(
            "command",
            node("scale").arg_node("arg1", node("sensor_read").container("sensor")),
        ),
    )
    .add_container("sensor", "sensor")
    .add_container("motor", "motor")
}

fn slots(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn create_container(
    worker: &mut CoreWorker,
    type_name: &str,
    name: &str,
    args: Vec<(&str, Value)>,
) -> JuizResult<ContainerPtr> {
    worker.create_container_ref(type_name, name, args.into())
}

fn call(p: &ProcessPtr, args: Vec<(&str, Value)>) -> JuizResult<i64> {
    p.lock()?
        .call(args.into())?
        .lock_as_value(|v| v.as_i64().unwrap())
}

fn run(name: &str, f: impl Fn(&System) -> JuizResult<()>) -> JuizResult<()> {
    crate::common::logger::init();
    System::new(jvalue!({
        "name": name,
        "option": { "http_broker": { "start": false } }
    }))?
    .setup()?
    .run_and_do_once(|system| {
        register_factories(system.core_broker().lock_mut()?.worker_mut())?;
        f(system)
    })
}

fn motor_history(motor: &ContainerPtr) -> Vec<i64> {
    motor
        .lock()
        .unwrap()
        .downcast_ref::<ContainerImpl<Motor>>()
        .unwrap()
        .history
        .clone()
}

fn sensor_reads(sensor: &ContainerPtr) -> i64 {
    sensor
        .lock()
        .unwrap()
        .downcast_ref::<ContainerImpl<Sensor>>()
        .unwrap()
        .reads
}

/// 2 つの Container をまたぐ CompositeContainerProcess (センサを読む → 変換 → モータに出す)
#[test]
fn composite_container_process_spans_two_containers() -> JuizResult<()> {
    run("composite_container_two", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        let sensor = create_container(worker, "sensor", "sensor0", vec![("value", jvalue!(5))])?;
        let motor = create_container(worker, "motor", "motor0", vec![])?;
        let cpf = worker.register_composite_container(&sense_scale_drive())?;
        let manifest = cpf.lock()?.manifest()?;
        assert_eq!(
            manifest.container_type.as_deref(),
            Some(COMPOSITE_CONTAINER_TYPE_NAME)
        );
        assert_eq!(manifest.factory, COMPOSITE_CONTAINER_PROCESS_FACTORY_NAME);
        let names: Vec<&str> = manifest.arguments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["factor"]);

        // スロットは名前でも ID でも指定できる
        let motor_id: String = motor.identifier().into();
        let cc = worker.create_composite_container(
            "robot0",
            "",
            &slots(&[("sensor", "sensor0"), ("motor", motor_id.as_str())]),
        )?;
        assert_eq!(
            cc.identifier().type_name,
            COMPOSITE_CONTAINER_TYPE_NAME.to_owned()
        );
        let p = worker.create_container_process_ref(
            cc.clone(),
            &ProcessManifest::new("sense_scale_drive").name("ssd0"),
        )?;
        assert_eq!(call(&p, vec![("factor", jvalue!(3))])?, 15);
        assert_eq!(call(&p, vec![("factor", jvalue!(2))])?, 10);
        assert_eq!(motor_history(&motor), vec![15, 10]);
        assert_eq!(sensor_reads(&sensor), 2);

        // ID と profile (仕様 §8)
        let pid = p.identifier();
        assert_eq!(
            Into::<String>::into(pid.clone()),
            "core://core/container_process/ssd0::sense_scale_drive:robot0"
        );
        let profile = bp.container_profile_full(&cc.identifier())?;
        let profile_slots = profile.slots.clone().expect("slots");
        assert_eq!(
            profile_slots.get("sensor").map(|s| s.as_str()),
            Some("core://core/container/sensor0::sensor")
        );
        assert_eq!(profile_slots.get("motor"), Some(&motor_id));
        assert_eq!(profile.processes.len(), 1);
        assert_eq!(profile.processes[0].name, "ssd0");
        let value = serde_json::to_value(&profile)?;
        assert_eq!(
            value["slots"]["sensor"],
            jvalue!("core://core/container/sensor0::sensor")
        );
        // 普通の Container の profile には slots を出さない
        let sensor_profile =
            serde_json::to_value(bp.container_profile_full(&sensor.identifier())?)?;
        assert!(sensor_profile.get("slots").is_none());

        // ブローカー越しにも呼べる
        let result = bp.container_process_call(&pid, vec![("factor", jvalue!(1))].into())?;
        assert_eq!(result.lock_as_value(|v| v.as_i64())?, Some(5));
        Ok(())
    })
}

/// 同じ Container を 2 回使う木 (同じスロットを 2 回、別のスロットに同じ Container) でもデッドロックしない
#[test]
fn composite_container_uses_same_container_twice() -> JuizResult<()> {
    run("composite_container_twice", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        create_container(worker, "counter", "counter0", vec![])?;
        // 同じスロット c を 2 回: counter_add(c, arg1 = counter_add(c, arg1))
        worker.register_composite_container(
            &CompositeManifest::new(
                "add_twice",
                node("counter_add")
                    .container("c")
                    .arg_node("arg1", node("counter_add").container("c")),
            )
            .add_container("c", "counter"),
        )?;
        // 別のスロット a と b に同じ Container: counter_add(a, arg1 = counter_add(b, arg1))
        worker.register_composite_container(
            &CompositeManifest::new(
                "add_two_slots",
                node("counter_add")
                    .container("a")
                    .arg_node("arg1", node("counter_add").container("b")),
            )
            .add_container("a", "counter")
            .add_container("b", "counter"),
        )?;
        let cc = worker.create_composite_container(
            "cc0",
            "",
            &slots(&[("c", "counter0"), ("a", "counter0"), ("b", "counter0")]),
        )?;
        // 余分なスロットがあってもよい (1 つの CompositeContainer に複数の Composite を載せる)
        let twice = worker.create_container_process_ref(
            cc.clone(),
            &ProcessManifest::new("add_twice").name("t0"),
        )?;
        let two_slots = worker.create_container_process_ref(
            cc.clone(),
            &ProcessManifest::new("add_two_slots").name("t1"),
        )?;
        // count: 0 -> 1 (内側) -> 2 (外側に 1 を足す)
        assert_eq!(call(&twice, vec![("arg1", jvalue!(1))])?, 2);
        // count: 2 -> 5 (内側) -> 10 (外側に 5 を足す)
        assert_eq!(call(&two_slots, vec![("arg1", jvalue!(3))])?, 10);
        Ok(())
    })
}

/// スロットが参照している Container は削除を拒否する。CompositeContainer を先に消せば消せる
#[test]
fn container_in_use_cannot_be_destroyed() -> JuizResult<()> {
    run("composite_container_in_use", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        let sensor = create_container(worker, "sensor", "sensor0", vec![])?;
        let motor = create_container(worker, "motor", "motor0", vec![])?;
        worker.register_composite_container(&sense_scale_drive())?;
        let cc = worker.create_composite_container(
            "robot0",
            "",
            &slots(&[("sensor", "sensor0"), ("motor", "motor0")]),
        )?;
        let p = worker.create_container_process_ref(
            cc.clone(),
            &ProcessManifest::new("sense_scale_drive").name("ssd0"),
        )?;
        let pid = p.identifier();
        drop(p);

        let err = bp
            .container_destroy(&sensor.identifier())
            .expect_err("sensor0 is in use");
        let cc_id: String = cc.identifier().into();
        match err.downcast_ref::<JuizError>() {
            Some(JuizError::ContainerInUseError { id, referenced_by }) => {
                assert_eq!(id, "core://core/container/sensor0::sensor");
                assert_eq!(referenced_by, &vec![cc_id.clone()]);
            }
            _ => panic!("unexpected error: {err:?}"),
        }
        assert!(err.to_string().contains(&cc_id), "{err}");
        // 拒否したので、Container も CompositeContainerProcess も残っている
        assert!(bp.container_profile_full(&sensor.identifier()).is_ok());
        assert!(bp.container_process_profile_full(&pid).is_ok());

        // CompositeContainer を消すと、その CompositeContainerProcess も消え、参照先は残る
        let profile = bp.container_destroy(&cc.identifier())?;
        assert_eq!(profile.type_name, COMPOSITE_CONTAINER_TYPE_NAME);
        assert!(bp.container_process_profile_full(&pid).is_err());
        assert!(bp.container_profile_full(&cc.identifier()).is_err());
        assert!(bp.container_profile_full(&motor.identifier()).is_ok());
        drop(cc);
        bp.container_destroy(&sensor.identifier())?;
        bp.container_destroy(&motor.identifier())?;
        Ok(())
    })
}

/// スロットの名前は CompositeContainer のネームスペースから解決する (先頭 / は絶対)
#[test]
fn composite_container_resolves_slot_names_in_namespace() -> JuizResult<()> {
    run("composite_container_namespace", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        let sensor = create_container(
            worker,
            "sensor",
            "sensor0",
            vec![("namespace", jvalue!("robot1")), ("value", jvalue!(7))],
        )?;
        create_container(worker, "motor", "motor0", vec![])?;
        worker.register_composite_container(&sense_scale_drive())?;
        let cc = worker.create_composite_container(
            "cc0",
            "robot1",
            &slots(&[("sensor", "sensor0"), ("motor", "/motor0")]),
        )?;
        assert_eq!(cc.identifier().namespace, "robot1");
        // ContainerProcess は CompositeContainer のネームスペースに置く
        let p = worker.create_container_process_ref(
            cc.clone(),
            &ProcessManifest::new("sense_scale_drive").name("ssd0"),
        )?;
        assert_eq!(p.identifier().namespace, "robot1");
        assert_eq!(call(&p, vec![("factor", jvalue!(2))])?, 14);
        assert_eq!(sensor_reads(&sensor), 1);

        // ルートから見ると robot1 の sensor0 は "robot1/sensor0"、"sensor0" では見つからない
        assert!(worker
            .create_composite_container(
                "cc1",
                "",
                &slots(&[("sensor", "sensor0"), ("motor", "motor0")]),
            )
            .is_err());
        worker.create_composite_container(
            "cc1",
            "",
            &slots(&[("sensor", "robot1/sensor0"), ("motor", "motor0")]),
        )?;
        Ok(())
    })
}

/// スロットの不足・型の食い違い・参照先の誤りはエラーにする
#[test]
fn composite_container_rejects_invalid_slots() -> JuizResult<()> {
    run("composite_container_invalid", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        let sensor = create_container(worker, "sensor", "sensor0", vec![])?;
        create_container(worker, "motor", "motor0", vec![])?;
        create_container(worker, "counter", "counter0", vec![])?;
        worker.register_composite_container(&sense_scale_drive())?;
        let manifest = ProcessManifest::new("sense_scale_drive").name("ssd0");

        // 参照先が無い
        assert!(worker
            .create_composite_container("bad0", "", &slots(&[("sensor", "nothing0")]))
            .is_err());
        // スロット名が不正
        assert!(worker
            .create_composite_container("bad1", "", &slots(&[("a:b", "sensor0")]))
            .is_err());

        // motor スロットが無い
        let missing =
            worker.create_composite_container("cc0", "", &slots(&[("sensor", "sensor0")]))?;
        let err = worker
            .create_container_process_ref(missing.clone(), &manifest)
            .expect_err("motor slot is missing");
        assert!(err.to_string().contains("'motor'"), "{err}");

        // motor スロットの型が違う
        let wrong = worker.create_composite_container(
            "cc1",
            "",
            &slots(&[("sensor", "sensor0"), ("motor", "counter0")]),
        )?;
        let err = worker
            .create_container_process_ref(wrong.clone(), &manifest)
            .expect_err("motor slot has a wrong type");
        assert!(err.to_string().contains("'motor'"), "{err}");

        // CompositeContainer ではない Container には作れない
        assert!(worker
            .create_container_process_ref(sensor.clone(), &manifest)
            .is_err());

        // CompositeContainer はスロットの参照先にできない
        let cc0_id: String = missing.identifier().into();
        assert!(worker
            .create_composite_container("cc2", "", &slots(&[("x", cc0_id.as_str())]))
            .is_err());

        // 同じ ID の CompositeContainer は作れない
        assert!(worker
            .create_composite_container("cc0", "", &slots(&[("sensor", "sensor0")]))
            .is_err());
        Ok(())
    })
}

/// 登録の入口の取り違えと、スロットを持たない Composite を子にする入れ子
#[test]
fn register_composite_container_rules() -> JuizResult<()> {
    run("composite_container_register", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        // スロットを持つ Composite は register_composite では登録できない (逆も同じ)
        assert!(worker.register_composite(&sense_scale_drive()).is_err());
        let pure = CompositeManifest::new(
            "inc_scale",
            node("scale").arg_node("arg1", node("increment")),
        );
        assert!(worker.register_composite_container(&pure).is_err());

        // スロットを持たない Composite を子にできる
        worker.register_composite(&pure)?;
        worker.register_composite_container(
            &CompositeManifest::new(
                "sense_inc_scale_drive",
                node("motor_write").container("motor").arg_node(
                    "command",
                    node("inc_scale").arg_node("arg1", node("sensor_read").container("sensor")),
                ),
            )
            .add_container("sensor", "sensor")
            .add_container("motor", "motor"),
        )?;
        // 同じ type_name は二重に登録できない (Process 側に同名があってもエラー)
        worker.register_composite_container(&sense_scale_drive())?;
        assert!(worker
            .register_composite_container(&sense_scale_drive())
            .is_err());
        assert!(worker
            .register_composite_container(
                &CompositeManifest::new("inc_scale", node("counter_add").container("c"),)
                    .add_container("c", "counter"),
            )
            .is_err());

        create_container(worker, "sensor", "sensor0", vec![("value", jvalue!(4))])?;
        let motor = create_container(worker, "motor", "motor0", vec![])?;
        let cc = worker.create_composite_container(
            "cc0",
            "",
            &slots(&[("sensor", "sensor0"), ("motor", "motor0")]),
        )?;
        let p = worker.create_container_process_ref(
            cc,
            &ProcessManifest::new("sense_inc_scale_drive").name("p0"),
        )?;
        // (4 + 1) * 3
        assert_eq!(call(&p, vec![("factor", jvalue!(3))])?, 15);
        assert_eq!(motor_history(&motor), vec![15]);
        Ok(())
    })
}

/// ContainerProcess の子で起きたエラーにも、式の木の中の位置が付く。後ろのノードは呼ばない
#[test]
fn composite_container_error_has_node_path() -> JuizResult<()> {
    run("composite_container_error", |system| {
        let mut bp = system.core_broker().lock_mut()?;
        let worker = bp.worker_mut();
        create_container(worker, "sensor", "sensor0", vec![("value", jvalue!(-1))])?;
        let motor = create_container(worker, "motor", "motor0", vec![])?;
        worker.register_composite_container(&sense_scale_drive())?;
        let cc = worker.create_composite_container(
            "cc0",
            "",
            &slots(&[("sensor", "sensor0"), ("motor", "motor0")]),
        )?;
        let p = worker.create_container_process_ref(
            cc,
            &ProcessManifest::new("sense_scale_drive").name("p0"),
        )?;
        let err = call(&p, vec![("factor", jvalue!(2))]).expect_err("sensor is broken");
        assert!(
            err.to_string()
                .contains("Composite(sense_scale_drive) failed at body.command.arg1 (sensor_read)"),
            "{err}"
        );
        assert!(motor_history(&motor).is_empty());
        Ok(())
    })
}
