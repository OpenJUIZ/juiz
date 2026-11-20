//! サブシステムの登録に関するテスト

use juiz_core::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;
use uuid::Uuid;

const MASTER_PORT: i64 = 8020;

fn system_manifest() -> Value {
    jvalue!({
        "name": "test_system",
        "option": {
            "http_broker": {
                "start": true,
                "port": MASTER_PORT
            }
        }
    })
}

/// increment_process を 1 つ持つ 2 台目の juiz。ポートが使われているので次のポートで待ち受ける
fn subsystem_manifest() -> Value {
    let mut manifest = system_manifest();
    manifest["plugins"] = jvalue!({
        "process_factories": {
            "increment_process": { "path": "../target/debug" }
        }
    });
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "inc0" }
    ]);
    manifest
}

fn subsystems(master: &System) -> JuizResult<Vec<Value>> {
    let profile = master.core_broker().lock()?.system_profile()?;
    Ok(profile["subsystems"]
        .as_array()
        .expect("subsystems must be array")
        .clone())
}

/// 2 台目の juiz を止めて同じアドレスで起動し直しても、1 台目の juiz に
/// サブシステムが二重に登録されない (issue #60)
#[cfg(test)]
#[test]
fn restarted_subsystem_replaces_old_one() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            let mut uuids = Vec::new();
            for _ in 0..2 {
                // 同じポートを指定すると次のポートで待ち受け、先の juiz のサブシステムとして登録される
                System::new(subsystem_manifest())?
                    .setup()?
                    .run_and_do_once(|sub| {
                        uuids.push(sub.core_broker().lock()?.system_uuid()?);
                        assert_eq!(subsystems(master)?.len(), 1);
                        // 先の juiz の再帰的な一覧に、2 台目の Process が 1 回だけ出る
                        let procs = master.core_broker().lock()?.process_list(true, None)?;
                        assert_eq!(procs.len(), 1, "{procs:?}");
                        // 止まるときに登録を外す (issue #303) と置き換えを確かめられないので、外さずに止まった場合を再現する
                        sub.core_broker().forget_connected_systems()?;
                        Ok(())
                    })?;
            }
            assert_ne!(uuids[0], uuids[1]);
            assert_eq!(subsystems(master)?.len(), 1);
            Ok(())
        })
}

/// 2 台目の juiz を止めると、1 台目の juiz の subsystems と Topic の転送先から外れる (issue #303)
#[cfg(test)]
#[test]
fn stopped_subsystem_is_removed_from_master() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(publisher_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            System::new(subscriber_manifest())?
                .setup()?
                .run_and_do_once(|sub| {
                    let uuid = Uuid::parse_str(
                        sub.core_broker().lock()?.system_uuid()?.as_str().unwrap(),
                    )?;
                    let mut cb = sub.core_broker().lock_mut()?;
                    let bp: &mut dyn BrokerProxy = &mut *cb;
                    bp.topic_request_publish("topic0", Some(uuid))?;
                    drop(cb);
                    assert_eq!(subsystems(master)?.len(), 1);
                    assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
                    Ok(())
                })?;
            assert_eq!(subsystems(master)?.len(), 0);
            assert!(mastersystems(master)?.is_empty());
            assert_eq!(num_topic_subscriber_subsystems(master)?, 0);
            // 再帰的な一覧も止まった juiz に問い合わせない
            let procs = master.core_broker().lock()?.process_list(true, None)?;
            assert_eq!(procs.len(), 1, "{procs:?}");
            // 登録されていない UUID を外そうとするとエラー
            let mut cb = master.core_broker().lock_mut()?;
            assert!(cb.system_remove_subsystem(Uuid::new_v4()).is_err());
            Ok(())
        })
}

/// 1 台目の juiz で Topic を publish し、2 台目の juiz で subscribe する
fn publisher_manifest() -> Value {
    let mut manifest = system_manifest();
    manifest["plugins"] = jvalue!({
        "process_factories": {
            "increment_process": { "path": "../target/debug" }
        }
    });
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "pub0", "publishes": ["topic0"] }
    ]);
    manifest
}

fn subscriber_manifest() -> Value {
    let mut manifest = subsystem_manifest();
    manifest["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "sub0", "subscribes": { "arg1": "topic0" } }
    ]);
    manifest
}

fn num_topic_subscriber_subsystems(system: &System) -> JuizResult<usize> {
    system
        .core_broker()
        .lock()?
        .worker()
        .store()
        .topics
        .get("topic0")
        .expect("topic0 must exist")
        .num_subscriber_subsystems()
}

/// 同じ問い合わせが 2 回来ても、また 2 台目の juiz を同じアドレスで起動し直しても、
/// Topic の転送先が二重に登録されない (issue #67)
#[cfg(test)]
#[test]
fn topic_subscriber_subsystem_is_not_duplicated() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(publisher_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            for _ in 0..2 {
                System::new(subscriber_manifest())?
                    .setup()?
                    .run_and_do_once(|sub| {
                        let uuid = Uuid::parse_str(
                            sub.core_broker().lock()?.system_uuid()?.as_str().unwrap(),
                        )?;
                        for _ in 0..2 {
                            let mut cb = sub.core_broker().lock_mut()?;
                            let bp: &mut dyn BrokerProxy = &mut *cb;
                            let r = bp.topic_request_publish("topic0", Some(uuid))?;
                            assert_eq!(r, jvalue!({"publish": true}));
                        }
                        assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
                        // 登録を外さずに止まった古いものが残っている状態で起動し直す
                        sub.core_broker().forget_connected_systems()?;
                        Ok(())
                    })?;
            }
            assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
            Ok(())
        })
}

/// 2 台目の juiz を止めた後、1 台目の Topic は push の失敗が続くとその転送先を外す (issue #301)
#[cfg(test)]
#[test]
fn stopped_subscriber_subsystem_is_dropped_from_topic() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(publisher_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            System::new(subscriber_manifest())?
                .setup()?
                .run_and_do_once(|sub| {
                    let uuid = Uuid::parse_str(
                        sub.core_broker().lock()?.system_uuid()?.as_str().unwrap(),
                    )?;
                    let mut cb = sub.core_broker().lock_mut()?;
                    let bp: &mut dyn BrokerProxy = &mut *cb;
                    bp.topic_request_publish("topic0", Some(uuid))?;
                    drop(cb);
                    // 終了処理で登録を外せずに止まった場合を再現する (issue #303 の後始末が無い場合)
                    sub.core_broker().forget_connected_systems()?;
                    Ok(())
                })?;
            assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
            let topic = master
                .core_broker()
                .lock()?
                .worker()
                .store()
                .topics
                .get("topic0")
                .expect("topic0 must exist")
                .clone();
            topic.set_push_failure_policy(std::time::Duration::ZERO, 2)?;
            // 止まった転送先への push は失敗しても Topic の push 自体はエラーにしない
            topic.push(jvalue!({"in1": 1}).into(), None)?;
            assert_eq!(num_topic_subscriber_subsystems(master)?, 1);
            topic.push(jvalue!({"in1": 1}).into(), None)?;
            assert_eq!(num_topic_subscriber_subsystems(master)?, 0);
            Ok(())
        })
}

/// 再帰的な system/profile で、サブシステムの中身と、経由する BrokerProxy が分かる (issue #116)
#[cfg(test)]
#[test]
fn recursive_profile_has_subsystem_contents() -> JuizResult<()> {
    crate::common::logger::init();
    System::new(system_manifest())?
        .setup()?
        .run_and_do_once(|master| {
            System::new(subsystem_manifest())?
                .setup()?
                .run_and_do_once(|sub| {
                    let uuid = sub.core_broker().lock()?.system_uuid()?;
                    // 再帰しない profile の subsystems は BrokerProxy の profile だけ
                    assert!(subsystems(master)?[0].get("core_store").is_none());

                    let profile = master
                        .core_broker()
                        .lock()?
                        .system_profile_recursive()?;
                    assert!(profile["connections"].is_array(), "{profile}");
                    let ss = profile["subsystems"]
                        .as_array()
                        .expect("subsystems must be array");
                    assert_eq!(ss.len(), 1);
                    assert_eq!(ss[0]["system_store"]["uuid"], uuid);
                    assert_eq!(ss[0]["broker_proxy"]["type_name"], jvalue!("http"));
                    assert_eq!(ss[0]["core_store"]["processes"][0]["name"], jvalue!("inc0"));
                    assert!(ss[0]["connections"].is_array(), "{}", ss[0]);
                    Ok(())
                })
        })
}

fn int_of(c: JuizResult<CapsulePtr>) -> i64 {
    c.expect("execute failed")
        .lock_as_value(|v| v.as_i64().expect("not an integer"))
        .unwrap()
}

/// サブシステム越しの publish / subscribe は、ネームスペースで解決した完全な名前でやり取りする (issue #149)
#[cfg(test)]
#[test]
fn namespaced_topic_over_subsystem() -> JuizResult<()> {
    crate::common::logger::init();
    let mut publisher = publisher_manifest();
    publisher["namespace"] = jvalue!("robot1");
    publisher["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "pub0", "publishes": ["value_topic"], "p_apply": { "arg1": 10 } }
    ]);
    let mut subscriber = subsystem_manifest();
    subscriber["namespace"] = jvalue!("robot1");
    subscriber["processes"] = jvalue!([
        // => robot1/value_topic (1 台目の juiz の pub0 とつながる)
        { "type_name": "increment_process", "name": "sub0", "subscribes": { "arg1": "value_topic" } },
        // => robot2/value_topic (つながらない)
        { "type_name": "increment_process", "name": "sub0", "namespace": "/robot2", "subscribes": { "arg1": "value_topic" } }
    ]);
    System::new(publisher)?.setup()?.run_and_do_once(|master| {
        System::new(subscriber)?.setup()?.run_and_do_once(|sub| {
            let topic_subsystems = |name: &str| -> JuizResult<usize> {
                match master
                    .core_broker()
                    .lock()?
                    .worker()
                    .store()
                    .topics
                    .get(name)
                {
                    Some(t) => t.num_subscriber_subsystems(),
                    None => Ok(0),
                }
            };
            assert_eq!(topic_subsystems("robot1/value_topic")?, 1);
            assert_eq!(topic_subsystems("robot2/value_topic")?, 0);
            assert_eq!(topic_subsystems("value_topic")?, 0);

            let pub0: ProcessIdentifier =
                "core://core/process/robot1/pub0::increment_process".try_into()?;
            let r1_sub0: ProcessIdentifier =
                "core://core/process/robot1/sub0::increment_process".try_into()?;
            let r2_sub0: ProcessIdentifier =
                "core://core/process/robot2/sub0::increment_process".try_into()?;
            assert_eq!(
                int_of(master.core_broker().lock_mut()?.process_execute(&pub0)),
                11
            );
            assert_eq!(
                int_of(sub.core_broker().lock_mut()?.process_execute(&r1_sub0)),
                12
            );
            assert_eq!(
                int_of(sub.core_broker().lock_mut()?.process_execute(&r2_sub0)),
                1
            );
            Ok(())
        })
    })
}

/// マスターシステムの一覧。複数持てるので配列 (issue #478)
fn mastersystems(system: &System) -> JuizResult<Vec<Value>> {
    let profile = system.core_broker().lock()?.system_profile()?;
    Ok(profile["mastersystem"]
        .as_array()
        .expect("mastersystem must be array")
        .clone())
}

/// juiz run のアプリのように、conf の subsystems に基礎を書いた側を止めると、
/// 基礎の subsystems・mastersystem・Topic の転送先から外れる (issue #303)
fn check_stopped_app_is_removed_from_base(base: Value, app: Value) -> JuizResult<()> {
    System::new(base)?.setup()?.run_and_do_once(|base| {
        System::new(app)?.setup()?.run_and_do_once(|app| {
            let uuid = Uuid::parse_str(app.core_broker().lock()?.system_uuid()?.as_str().unwrap())?;
            let mut cb = app.core_broker().lock_mut()?;
            let bp: &mut dyn BrokerProxy = &mut *cb;
            bp.topic_request_publish("topic0", Some(uuid))?;
            drop(cb);
            // http ではお互いを subsystems と mastersystem の両方に、ipc では基礎はアプリを mastersystem にだけ登録する
            assert!(subsystems(base)?.len() == 1 || mastersystems(base)?.len() == 1);
            assert_eq!(num_topic_subscriber_subsystems(base)?, 1);
            Ok(())
        })?;
        assert_eq!(subsystems(base)?.len(), 0);
        assert!(mastersystems(base)?.is_empty());
        assert_eq!(num_topic_subscriber_subsystems(base)?, 0);
        let bps = base
            .core_broker()
            .lock()?
            .worker()
            .store()
            .broker_proxies
            .list_ids()?;
        assert!(
            !bps.as_array().unwrap().iter().any(|id| {
                let id = id.as_str().unwrap();
                id.ends_with("::http") || id.ends_with("::ipc")
            }),
            "{bps}"
        );
        Ok(())
    })
}

#[cfg(test)]
#[test]
fn stopped_app_is_removed_from_base_http() -> JuizResult<()> {
    crate::common::logger::init();
    let mut app = subscriber_manifest();
    app["subsystems"] =
        jvalue!([{ "type_name": "http", "name": format!("127.0.0.1:{MASTER_PORT}") }]);
    check_stopped_app_is_removed_from_base(publisher_manifest(), app)
}

#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn stopped_app_is_removed_from_base_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let base_ipc = format!("subsystem_test_base-{}", std::process::id());
    let app_ipc = format!("subsystem_test_app-{}", std::process::id());
    let mut base = publisher_manifest();
    base["option"]["http_broker"]["start"] = jvalue!(false);
    base["brokers"] = jvalue!([{ "type_name": "ipc", "name": base_ipc }]);
    let mut app = subscriber_manifest();
    app["option"]["http_broker"]["start"] = jvalue!(false);
    app["brokers"] = jvalue!([{ "type_name": "ipc", "name": app_ipc }]);
    app["subsystems"] = jvalue!([{ "type_name": "ipc", "name": base_ipc }]);
    check_stopped_app_is_removed_from_base(base, app)
}

/// サブシステムのつなぎ方が輪 (A の subsystems = [B, C]、C の subsystems = [B]) でも、
/// topic/request_subscribe・request_publish が A に戻ってこず起動でき、C の publish が A に届く (issue #449)。
/// A も publish する Topic を持つと、起動時に A から request_subscribe が出て A → C → B → A と回る
fn check_topic_over_subsystem_cycle(b: Value, mut c: Value, mut a: Value) -> JuizResult<()> {
    c["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "pub0", "publishes": ["topic0"], "p_apply": { "arg1": 10 } }
    ]);
    a["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "sub0", "subscribes": { "arg1": "topic0" } },
        { "type_name": "increment_process", "name": "pub1", "publishes": ["topic1"] }
    ]);
    System::new(b)?.setup()?.run_and_do_once(|_b| {
        System::new(c)?.setup()?.run_and_do_once(|c| {
            System::new(a)?.setup()?.run_and_do_once(|a| {
                let pub0: ProcessIdentifier =
                    "core://core/process/pub0::increment_process".try_into()?;
                let sub0: ProcessIdentifier =
                    "core://core/process/sub0::increment_process".try_into()?;
                assert_eq!(
                    int_of(c.core_broker().lock_mut()?.process_execute(&pub0)),
                    11
                );
                assert_eq!(
                    int_of(a.core_broker().lock_mut()?.process_execute(&sub0)),
                    12
                );
                Ok(())
            })
        })
    })
}

#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn topic_over_subsystem_cycle_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let pid = std::process::id();
    let ipc = |n: &str| format!("subsystem_cycle_{n}-{pid}");
    let ipc_system = |n: &str, subs: &[&str]| -> Value {
        let mut m = subsystem_manifest();
        m["option"]["http_broker"]["start"] = jvalue!(false);
        m["brokers"] = jvalue!([{ "type_name": "ipc", "name": ipc(n) }]);
        m["subsystems"] = Value::Array(
            subs.iter()
                .map(|s| jvalue!({ "type_name": "ipc", "name": ipc(s) }))
                .collect(),
        );
        m
    };
    check_topic_over_subsystem_cycle(
        ipc_system("b", &[]),
        ipc_system("c", &["b"]),
        ipc_system("a", &["b", "c"]),
    )
}

/// HTTP ではサブシステムの登録がお互い向きになるので、輪がもっと多い
#[cfg(test)]
#[test]
fn topic_over_subsystem_cycle_http() -> JuizResult<()> {
    crate::common::logger::init();
    let http_system = |port: i64, subs: &[i64]| -> Value {
        let mut m = subsystem_manifest();
        m["option"]["http_broker"]["port"] = jvalue!(port);
        m["subsystems"] = Value::Array(
            subs.iter()
                .map(|p| jvalue!({ "type_name": "http", "name": format!("127.0.0.1:{p}") }))
                .collect(),
        );
        m
    };
    let (b, c, a) = (MASTER_PORT, MASTER_PORT + 1, MASTER_PORT + 2);
    check_topic_over_subsystem_cycle(
        http_system(b, &[]),
        http_system(c, &[b]),
        http_system(a, &[b, c]),
    )
}

/// IPC で、subsystems に書いた相手 (`subs`) につなぐ System
#[cfg(feature = "ipc-broker")]
fn ipc_system(prefix: &str, n: &str, subs: &[&str]) -> Value {
    let ipc = |n: &str| format!("{prefix}_{n}-{}", std::process::id());
    let mut m = subsystem_manifest();
    m["option"]["http_broker"]["start"] = jvalue!(false);
    m["brokers"] = jvalue!([{ "type_name": "ipc", "name": ipc(n) }]);
    m["subsystems"] = Value::Array(
        subs.iter()
            .map(|s| jvalue!({ "type_name": "ipc", "name": ipc(s) }))
            .collect(),
    );
    m
}

/// マスターシステムへのブローカーの名前の一覧
fn master_names(system: &System) -> JuizResult<Vec<String>> {
    Ok(mastersystems(system)?
        .iter()
        .map(|m| m["name"].as_str().unwrap_or_default().to_owned())
        .collect())
}

/// 星形 (A の subsystems = [B]、C の subsystems = [B]) では、B は A と C の両方をマスターシステムとして覚え、
/// C の publish が B を通って A に届く。A・C を止めると B のマスターシステムから外れる (issue #478)。
/// A と C はどちらも topic0 を publish も subscribe もするので、B は A と C の両方を転送先にし、A・C も B を転送先にする。
/// 受け取ったデータを送ってきた System へ送り返すと C → B → C と回り続けるので、送り返さないことも確かめる
#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn topic_over_star_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let b = ipc_system("subsystem_star", "b", &[]);
    let mut a = ipc_system("subsystem_star", "a", &["b"]);
    let mut c = ipc_system("subsystem_star", "c", &["b"]);
    a["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "sub0", "subscribes": { "arg1": "topic0" } },
        { "type_name": "increment_process", "name": "pub1", "publishes": ["topic0"], "p_apply": { "arg1": 20 } }
    ]);
    c["processes"] = jvalue!([
        { "type_name": "increment_process", "name": "pub0", "publishes": ["topic0"], "p_apply": { "arg1": 10 } },
        { "type_name": "increment_process", "name": "sub1", "subscribes": { "arg1": "topic0" } }
    ]);
    let a_ipc = a["brokers"][0]["name"].as_str().unwrap().to_owned();
    let c_ipc = c["brokers"][0]["name"].as_str().unwrap().to_owned();
    System::new(b)?.setup()?.run_and_do_once(|b| {
        System::new(a)?.setup()?.run_and_do_once(|a| {
            System::new(c)?.setup()?.run_and_do_once(|c| {
                let mut masters = master_names(b)?;
                masters.sort();
                assert_eq!(masters, vec![a_ipc.clone(), c_ipc.clone()]);
                let pub0: ProcessIdentifier =
                    "core://core/process/pub0::increment_process".try_into()?;
                let sub0: ProcessIdentifier =
                    "core://core/process/sub0::increment_process".try_into()?;
                let sub1: ProcessIdentifier =
                    "core://core/process/sub1::increment_process".try_into()?;
                // 送り返すと、IPC の応答待ちの時間切れまで C → B → C と回るので数十秒かかる
                let started = std::time::Instant::now();
                assert_eq!(
                    int_of(c.core_broker().lock_mut()?.process_execute(&pub0)),
                    11
                );
                assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
                assert_eq!(
                    int_of(a.core_broker().lock_mut()?.process_execute(&sub0)),
                    12
                );
                assert_eq!(
                    int_of(c.core_broker().lock_mut()?.process_execute(&sub1)),
                    12
                );
                Ok(())
            })?;
            assert_eq!(master_names(b)?, vec![a_ipc.clone()]);
            Ok(())
        })?;
        assert!(mastersystems(b)?.is_empty());
        Ok(())
    })
}

/// 星形で、後からつないだ C があっても、先につないだ A から B の Container へ Transform を張れる。
/// 終点の B は A を source_system の UUID で探すので、A がマスターシステムに残っている必要がある (issue #478)
#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn transform_over_star_ipc() -> JuizResult<()> {
    use juiz_sdk::geometry::Vector3;
    use juiz_sdk::manifests::TransformManifest;
    struct Part {}
    crate::common::logger::init();
    let b_conf = ipc_system("subsystem_star_tf", "b", &[]);
    let b_ipc = b_conf["brokers"][0]["name"].as_str().unwrap().to_owned();
    System::new(b_conf)?.setup()?.run_and_do_once(|b| {
        {
            let mut cb = b.core_broker().lock_mut()?;
            let juiz_sdk::factory::ContainerFactoryStruct(manifest, constructor) =
                juiz_sdk::factory::container_factory(
                    ContainerManifest::new("part"),
                    |_args: CapsuleMap| Ok(Box::new(Part {})),
                );
            cb.worker_mut()
                .store_mut()
                .containers
                .register_factory("part", container_factory_create(manifest, constructor)?)?;
            cb.worker_mut().create_container_ref(
                "part",
                "base0",
                Vec::<(&str, Value)>::new().into(),
            )?;
        }
        System::new(ipc_system("subsystem_star_tf", "a", &["b"]))?
            .setup()?
            .run_and_do_once(|a| {
                System::new(ipc_system("subsystem_star_tf", "c", &["b"]))?
                    .setup()?
                    .run_and_do_once(|_c| {
                        let mut cb = a.core_broker().lock_mut()?;
                        let bp: &mut dyn BrokerProxy = &mut *cb;
                        bp.container_create(
                            &ContainerManifest::new("static_anchor").name("map"),
                            CapsuleMap::new(),
                        )?;
                        let p = bp.transform_create(
                            &TransformManifest::new(
                                "map",
                                &format!("ipc://{b_ipc}/container/base0::part"),
                            )
                            .translation(Vector3::new(1.0, 0.0, 0.0)),
                        )?;
                        assert!(
                            p["identifier"]
                                .as_str()
                                .is_some_and(|id| id.ends_with("|core://core/container/base0::part")),
                            "{p}"
                        );
                        Ok(())
                    })
            })
    })
}

/// 一本の木 (A の subsystems = [C]、C の subsystems = [B]) で、A の再帰的な一覧に B の Process も出る (issue #449・#478)
#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn recursive_list_over_two_levels_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let b = ipc_system("subsystem_tree", "b", &[]);
    let c = ipc_system("subsystem_tree", "c", &["b"]);
    let a = ipc_system("subsystem_tree", "a", &["c"]);
    System::new(b)?.setup()?.run_and_do_once(|_b| {
        System::new(c)?.setup()?.run_and_do_once(|_c| {
            System::new(a)?.setup()?.run_and_do_once(|a| {
                let procs = a.core_broker().lock()?.process_list(true, None)?;
                assert_eq!(procs.len(), 3, "{procs:?}");
                Ok(())
            })
        })
    })
}

/// system/profile?recursive=true の答えに入っている System の UUID (重複も数える)。
/// visited (ほかの所で中身を出した System) は数えない
fn walked_uuids(profile: &Value, out: &mut Vec<String>) {
    if let Some(u) = profile["system_store"]["uuid"].as_str() {
        out.push(u.to_owned());
    }
    for key in ["mastersystem", "subsystems"] {
        for c in profile[key].as_array().into_iter().flatten() {
            assert!(c.get("error").is_none(), "{c}");
            walked_uuids(c, out);
        }
    }
}

/// 再帰の一覧と profile が、どこから問い合わせても 3 つの System を 1 度ずつ出すことを確かめる
fn check_walks_all(systems: &[&System], count: usize) -> JuizResult<()> {
    for s in systems {
        let started = std::time::Instant::now();
        let procs = s.core_broker().lock()?.process_list(true, None)?;
        assert_eq!(procs.len(), count, "{procs:?}");
        let profile = s.core_broker().lock()?.system_profile_recursive()?;
        let mut uuids = Vec::new();
        walked_uuids(&profile, &mut uuids);
        assert_eq!(uuids.len(), count, "{profile}");
        uuids.sort();
        uuids.dedup();
        assert_eq!(uuids.len(), count, "{profile}");
        // 輪で戻ると IPC の応答待ちの時間切れ (約 10 秒) まで止まる
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    }
    Ok(())
}

/// 星形 (A の subsystems = [B]、C の subsystems = [B]) では、B から見ると A と C はマスターシステム。
/// 再帰の一覧と system/profile?recursive=true はマスターシステムもたどるので、A・B・C のどこから
/// 問い合わせても 3 つの System が全部出る (issue #559。juiz run の基礎とアプリの形)
#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn recursive_walk_over_mastersystems_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let b = ipc_system("subsystem_walk_star", "b", &[]);
    let a = ipc_system("subsystem_walk_star", "a", &["b"]);
    let c = ipc_system("subsystem_walk_star", "c", &["b"]);
    System::new(b)?.setup()?.run_and_do_once(|b| {
        System::new(a)?.setup()?.run_and_do_once(|a| {
            System::new(c)?.setup()?.run_and_do_once(|c| {
                check_walks_all(&[a, b, c], 3)?;
                // B から見た profile では、A と C はマスターシステムに中身ごと入る
                let profile = b.core_broker().lock()?.system_profile_recursive()?;
                let masters = profile["mastersystem"].as_array().unwrap();
                assert_eq!(masters.len(), 2, "{profile}");
                assert!(masters.iter().all(|m| m.get("core_store").is_some()), "{profile}");
                // A から見ると B はサブシステムで、B のマスターシステムのうち A 自身は visited になる
                let profile = a.core_broker().lock()?.system_profile_recursive()?;
                let b_masters = profile["subsystems"][0]["mastersystem"].as_array().unwrap();
                assert_eq!(
                    b_masters.iter().filter(|m| m["visited"] == jvalue!(true)).count(),
                    1,
                    "{profile}"
                );
                Ok(())
            })
        })
    })
}

/// 互いに subsystem の輪 (A の subsystems = [B] に加えて、B に add_subsystem で A を足す) でも、
/// 再帰の一覧と profile が戻って止まらない (issue #559)
#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn recursive_walk_over_mutual_subsystems_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let b = ipc_system("subsystem_walk_ring", "b", &[]);
    let a = ipc_system("subsystem_walk_ring", "a", &["b"]);
    let c = ipc_system("subsystem_walk_ring", "c", &["a"]);
    let a_ipc = a["brokers"][0]["name"].as_str().unwrap().to_owned();
    let c_ipc = c["brokers"][0]["name"].as_str().unwrap().to_owned();
    System::new(b)?.setup()?.run_and_do_once(|b| {
        System::new(a)?.setup()?.run_and_do_once(|a| {
            b.core_broker()
                .system_add_subsystem(jvalue!({ "type_name": "ipc", "name": a_ipc }))?;
            check_walks_all(&[a, b], 2)?;
            // 3 つで輪 (C → A → B → C) になっても同じ
            System::new(c)?.setup()?.run_and_do_once(|c| {
                b.core_broker()
                    .system_add_subsystem(jvalue!({ "type_name": "ipc", "name": c_ipc }))?;
                check_walks_all(&[a, b, c], 3)
            })
        })
    })
}

/// 4 つの輪 (A → B → C → D → A) と、さらに対角 (A → C) を足した形でも、
/// 再帰の一覧に同じ System の Process が 2 度出ない (issue #559)。
/// 直接の相手を via に入れるだけでは、A から B 経由と D 経由の両方で C が数えられてしまう
#[cfg(all(test, feature = "ipc-broker"))]
#[test]
fn recursive_walk_over_ring_of_four_ipc() -> JuizResult<()> {
    crate::common::logger::init();
    let d = ipc_system("subsystem_walk_ring4", "d", &[]);
    let c = ipc_system("subsystem_walk_ring4", "c", &["d"]);
    let b = ipc_system("subsystem_walk_ring4", "b", &["c"]);
    let a = ipc_system("subsystem_walk_ring4", "a", &["b"]);
    let a_ipc = a["brokers"][0]["name"].as_str().unwrap().to_owned();
    let c_ipc = c["brokers"][0]["name"].as_str().unwrap().to_owned();
    System::new(d)?.setup()?.run_and_do_once(|d| {
        System::new(c)?.setup()?.run_and_do_once(|c| {
            System::new(b)?.setup()?.run_and_do_once(|b| {
                System::new(a)?.setup()?.run_and_do_once(|a| {
                    d.core_broker()
                        .system_add_subsystem(jvalue!({ "type_name": "ipc", "name": a_ipc }))?;
                    check_walks_all(&[a, b, c, d], 4)?;
                    a.core_broker()
                        .system_add_subsystem(jvalue!({ "type_name": "ipc", "name": c_ipc }))?;
                    check_walks_all(&[a, b, c, d], 4)
                })
            })
        })
    })
}
