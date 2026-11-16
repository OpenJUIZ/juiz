//! BrokerProxy の API を一通り呼ぶ確認。どのブローカーでも同じ結果になるべきもの
//!
//! 各ブローカーのテストは、[`crate::fixture::Fixture`] の Process・Container を置いた System に
//! そのブローカーの BrokerProxy で繋ぎ、[`check_all`] (または個々の check_*) を呼ぶ。
//! 確認の途中で作ったものは消して戻すので、続けて別の確認を呼んでよい。

use std::time::{Duration, Instant};

use juiz_core::prelude::*;
use juiz_sdk::connection_identifier::ConnectionIdentifier;
use juiz_sdk::process_identifier::ProcessIdentifier;

use crate::fixture::{Fixture, TIMER_EC};
use crate::support::*;

/// 識別子に入るべきブローカー
pub struct Expect<'a> {
    /// BrokerProxy が返す識別子の broker_type_name (core・local・http・ipc・qmp・zenoh)
    pub broker_type_name: &'a str,
    /// BrokerProxy が返す識別子の broker_name。None なら見ない
    pub broker_name: Option<&'a str>,
}

/// この module の確認をすべて呼ぶ
pub fn check_all(bp: &mut dyn BrokerProxy, fx: &Fixture, expect: &Expect) -> JuizResult<()> {
    check_system(bp)?;
    check_processes(bp, fx, expect)?;
    check_unknown_process(bp, fx)?;
    check_containers(bp, fx)?;
    check_ec(bp)?;
    check_topics(bp)?;
    check_connections(bp, fx)?;
    check_create_destroy(bp)?;
    Ok(())
}

/// System とブローカーの情報が読める。解析できない識別子はエラーになり、その後も呼べる
pub fn check_system(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    assert!(bp.system_profile()?.is_object());
    // サブシステム登録では UUID を文字列として読むので、包まずに返すこと
    assert!(bp.system_uuid()?.as_str().is_some());
    bp.system_filesystem_list(std::path::PathBuf::from("."))?;

    // 応答をそのまま型に読み込めること (issue #19, #36)。
    // ブローカーの識別子は .../broker/{name}::{type_name} の形 (issue #35)
    let broker_ids = bp.broker_list(false)?;
    assert!(!broker_ids.is_empty());
    for id in broker_ids.iter() {
        let id_struct = IdentifierStruct::try_from(id.clone())?;
        assert_eq!(id_struct.class_name, "broker", "{id}");
        let profile = bp.broker_profile(id)?;
        assert_eq!(profile.name, id_struct.object_name, "{id}");
        assert_eq!(profile.type_name, id_struct.type_name, "{id}");
    }
    assert!(bp.broker_profile(&"invalid".to_owned()).is_err());
    assert!(bp
        .broker_profile(&"core://core/broker/none::none".to_owned())
        .is_err());
    assert!(!bp.broker_list(false)?.is_empty());
    Ok(())
}

/// arg1 の拘束値 (profile の value)
fn bound_arg1(bp: &mut dyn BrokerProxy, id: &ProcessIdentifier) -> JuizResult<Option<Value>> {
    Ok(bp
        .any_process_profile(id)?
        .argument_values
        .get("arg1")
        .cloned())
}

/// Process の profile・call・execute・p_apply・call_partial・push_by
pub fn check_processes(bp: &mut dyn BrokerProxy, fx: &Fixture, expect: &Expect) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    for (name, type_name) in fx.processes {
        let id = find_process(&ids, name);
        // 一覧の識別子は、アクセスしたブローカーを指す
        assert_eq!(id.broker_type_name, expect.broker_type_name, "{id:?}");
        if let Some(broker_name) = expect.broker_name {
            assert_eq!(id.broker_name, broker_name, "{id:?}");
        }
        let profile = bp.process_profile(id)?;
        assert_eq!(
            (profile.instance_name(), profile.type_name.as_str()),
            (*name, *type_name)
        );
        assert_eq!(bp.any_process_profile(id)?.instance_name(), *name);

        // 返された識別子をそのまま渡しても、サーバーが自分宛てに再接続せずに処理できる
        assert_eq!(int_of(bp.process_call(id, args("arg1", 10))), 11, "{name}");
        assert_eq!(
            int_of(bp.any_process_call(id, args("arg1", 20))),
            21,
            "{name}"
        );
        // push_by は引数を設定して実行する
        assert_eq!(
            int_of(bp.process_push_by(id, "arg1".to_owned(), jvalue!(7).into())),
            8,
            "{name}"
        );
        // p_apply で設定した引数は、メモがあっても次の execute に反映される (issue #18)。
        // 送った値は __value__ で包まれずに引数へ入る (issue #36)
        bp.process_p_apply(id, "arg1", jvalue!(30).into())?;
        assert_eq!(int_of(bp.process_execute(id)), 31, "{name}");
        // 2回目以降の p_apply も反映される
        bp.process_p_apply(id, "arg1", jvalue!(40).into())?;
        assert_eq!(int_of(bp.process_execute(id)), 41, "{name}");
        // call_partial は省いた引数を拘束値で補い、渡した値で拘束値を書き換えない (issue #83)
        assert_eq!(
            int_of(bp.process_call_partial(id, CapsuleMap::new())),
            41,
            "{name}"
        );
        assert_eq!(
            int_of(bp.any_process_call_partial(id, args("arg1", 3))),
            4,
            "{name}"
        );
        assert_eq!(bound_arg1(bp, id)?, Some(jvalue!(40)), "{name}");
        // 存在しない引数は設定できない
        assert!(bp
            .process_p_apply(id, "no_such_arg", jvalue!(1).into())
            .is_err());
        // 結果の option は空 (#277)
        assert_no_options(bp.process_call(id, args("arg1", 1)));
        assert_no_options(bp.any_process_call(id, args("arg1", 1)));
        assert_no_options(bp.process_execute(id));
    }
    Ok(())
}

/// 存在しない Process はすぐにエラーになり、同じ BrokerProxy でその後も呼べる
pub fn check_unknown_process(bp: &mut dyn BrokerProxy, fx: &Fixture) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let (name, _) = fx.processes[0];
    let unknown = renamed(find_process(&ids, name), "nothing");
    let start = Instant::now();
    assert!(bp.process_call(&unknown, args("arg1", 1)).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(bp.process_profile(&unknown).is_err());
    assert!(bp.process_execute(&unknown).is_err());
    assert_eq!(bp.process_list(false, None)?.len(), ids.len());
    assert_eq!(
        int_of(bp.process_call(find_process(&ids, name), args("arg1", 1))),
        2
    );
    Ok(())
}

/// ContainerProcess の call・execute・p_apply・call_partial と Container の一覧
pub fn check_containers(bp: &mut dyn BrokerProxy, fx: &Fixture) -> JuizResult<()> {
    let ids = bp.container_process_list(false, None)?;
    for (container_name, add_name, arg_name, get_name) in fx.containers {
        let add = find_process(&ids, add_name);
        let get = find_process(&ids, get_name);
        assert_eq!(
            bp.container_process_profile(add)?.instance_name(),
            *add_name
        );
        assert_eq!(
            bp.container_process_profile(get)?.instance_name(),
            *get_name
        );
        assert_eq!(bp.any_process_profile(get)?.instance_name(), *get_name);

        let before = int_of(bp.container_process_call(get, CapsuleMap::new()));
        assert_eq!(
            int_of(bp.container_process_call(add, args(arg_name, 3))),
            before + 3,
            "{container_name}"
        );
        assert_eq!(
            int_of(bp.any_process_call(get, CapsuleMap::new())),
            before + 3,
            "{container_name}"
        );
        bp.container_process_p_apply(add, arg_name, jvalue!(2).into())?;
        assert!(
            try_int_of(&bp.container_process_execute(add)?).is_some(),
            "{container_name}"
        );
        assert!(
            try_int_of(&bp.container_process_execute(get)?).is_some(),
            "{container_name}"
        );
        // call_partial は省いた引数を拘束値で補う (issue #83)
        let before2 = int_of(bp.container_process_call(get, CapsuleMap::new()));
        bp.container_process_call_partial(add, CapsuleMap::new())?;
        assert_eq!(
            int_of(bp.container_process_call(get, CapsuleMap::new())),
            before2 + 2,
            "{container_name}"
        );
        bp.any_process_call_partial(add, args(arg_name, 5))?;
        assert_eq!(
            int_of(bp.container_process_call(get, CapsuleMap::new())),
            before2 + 7,
            "{container_name}"
        );
        // get には引数がない
        assert!(bp
            .container_process_p_apply(get, arg_name, jvalue!(1).into())
            .is_err());
    }

    let mut names = vec![];
    for id in bp.container_list(false, None)? {
        let profile = bp.container_profile(&id)?;
        // 識別子は <name>::<type_name> の順に並ぶ (issue #15)
        let id_struct = IdentifierStruct::try_from(id.to_string())?;
        assert_eq!(id_struct.object_name, profile.instance_name(), "{id}");
        assert_eq!(id_struct.type_name, profile.type_name, "{id}");
        names.push(profile.instance_name().to_owned());
    }
    names.sort();
    let mut expected: Vec<_> = fx.containers.iter().map(|c| c.0.to_owned()).collect();
    expected.sort();
    assert_eq!(names, expected);
    Ok(())
}

fn ec_ids(bp: &mut dyn BrokerProxy) -> JuizResult<Vec<String>> {
    Ok(unwrap_value(bp.ec_list(false)?)
        .as_array()
        .expect("ec_list is not an array")
        .iter()
        .map(|v| {
            v.as_str()
                .expect("EC identifier is not a string")
                .to_owned()
        })
        .collect())
}

fn ec_state(bp: &mut dyn BrokerProxy, id: &Identifier) -> JuizResult<Value> {
    Ok(unwrap_value(bp.ec_get_state(id)?))
}

/// EC の状態はスレッド側で切り替わるので、しばらく待つ
fn wait_ec_state(bp: &mut dyn BrokerProxy, id: &Identifier, state: &str) -> JuizResult<bool> {
    wait_until(Duration::from_secs(2), || {
        Ok(ec_state(bp, id)? == jvalue!(state))
    })
}

/// EC の profile・状態・start・stop・作成・削除
pub fn check_ec(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let ids = ec_ids(bp)?;
    let n = ids.len();
    let id = ids
        .iter()
        .find(|id| id.contains(TIMER_EC))
        .expect("timer0 not listed")
        .clone();
    assert_eq!(unwrap_value(bp.ec_profile(&id)?)["name"], jvalue!(TIMER_EC));
    assert_eq!(ec_state(bp, &id)?, jvalue!("STOPPED"));
    bp.ec_start(&id)?;
    assert!(wait_ec_state(bp, &id, "STARTED")?, "{id} did not start");
    bp.ec_stop(&id)?;
    assert!(wait_ec_state(bp, &id, "STOPPED")?, "{id} did not stop");

    let created = unwrap_value(
        bp.ec_create(&jvalue!({"type_name": "TimerEC", "name": "timer1", "rate": 100.0}))?,
    );
    assert_eq!(created["name"], jvalue!("timer1"));
    assert_eq!(ec_ids(bp)?.len(), n + 1);
    assert!(bp
        .ec_create(&jvalue!({"type_name": "NoSuchEC", "name": "x"}))
        .is_err());
    assert!(ec_state(bp, &id.replace(TIMER_EC, "nothing")).is_err());

    // 動いている EC も削除できる (issue #34)。
    // メッセンジャー系のブローカーは自分のスレッドの中で EC を drop する。
    // EC が持つ tokio のランタイムをそこで落としても panic しないこと (issue #231)
    let timer1 = ec_ids(bp)?
        .into_iter()
        .find(|id| id.contains("timer1"))
        .expect("timer1 not listed");
    bp.ec_start(&timer1)?;
    assert!(wait_ec_state(bp, &timer1, "STARTED")?);
    assert_eq!(
        unwrap_value(bp.ec_destroy(&timer1)?)["name"],
        jvalue!("timer1")
    );
    assert_eq!(ec_ids(bp)?.len(), n);
    assert!(bp.ec_get_state(&timer1).is_err());
    assert!(bp.ec_destroy(&timer1).is_err());
    Ok(())
}

/// 購読者も配信者もいない Topic
pub fn check_topics(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let topic = "broker_suite_unused_topic";
    assert!(!bp.topic_list()?.iter().any(|t| t.name.contains(topic)));
    let uuid = Some(uuid::Uuid::new_v4());
    // ローカルにサブスクライバもパブリッシャもいないので購読・配信は要求されない
    assert_eq!(
        unwrap_value(bp.topic_request_subscribe(topic, uuid)?),
        jvalue!({"subscribe": false})
    );
    assert_eq!(
        unwrap_value(bp.topic_request_publish(topic, uuid)?),
        jvalue!({"publish": false})
    );
    // 存在しないトピックには push できない
    assert!(bp.topic_push(topic, jvalue!(1).into(), None).is_err());
    Ok(())
}

/// Pull と Push のコネクションの作成・一覧・削除 (issue #31, #34)
pub fn check_connections(bp: &mut dyn BrokerProxy, fx: &Fixture) -> JuizResult<()> {
    let ids = bp.process_list(false, None)?;
    let (src_name, dst_name) = fx.connect;
    let src = find_process(&ids, src_name).clone();
    let dst = find_process(&ids, dst_name).clone();
    let cm = ConnectionManifest::new(
        ConnectionType::Pull,
        src.clone(),
        "arg1".to_owned(),
        dst.clone(),
    );
    assert_eq!(bp.connection_create(&cm)?.arg_name, "arg1");
    // Topic のコネクションも並ぶことがあるので、src -> dst のものを探す
    let is_ours = |c: &ConnectionIdentifier| {
        c.source_identifier.name == src_name && c.destination_identifier.name == dst_name
    };
    let con = bp
        .connection_list(false)?
        .into_iter()
        .find(|c| is_ours(c))
        .expect("connection not listed");
    assert_eq!(con.arg_name, "arg1");
    assert_eq!(bp.connection_profile(&con)?.arg_name, "arg1");

    // dst を実行すると src の出力を pull して使う
    let src_out = int_of(bp.process_execute(&src));
    assert_eq!(int_of(bp.process_execute(&dst)), src_out + 1);

    // コネクションを削除すると、dst は pull せずに引数の値を使う
    assert_eq!(bp.connection_destroy(&con)?.arg_name, "arg1");
    assert!(!bp.connection_list(false)?.iter().any(|c| is_ours(c)));
    assert!(bp.connection_profile(&con).is_err());
    assert!(bp.connection_destroy(&con).is_err());
    bp.process_p_apply(&dst, "arg1", jvalue!(100).into())?;
    assert_eq!(int_of(bp.process_execute(&dst)), 101);

    // Push で繋ぐと、src を実行したときに出力が dst の arg1 に入り、dst も実行される
    let cm = ConnectionManifest::new(
        ConnectionType::Push,
        src.clone(),
        "arg1".to_owned(),
        dst.clone(),
    );
    assert_eq!(bp.connection_create(&cm)?.arg_name, "arg1");
    let con = bp
        .connection_list(false)?
        .into_iter()
        .find(|c| is_ours(c))
        .expect("push connection not listed");
    let src_out = int_of(bp.process_execute(&src));
    assert_eq!(bound_arg1(bp, &dst)?, Some(jvalue!(src_out)));
    assert_eq!(int_of(bp.process_execute(&dst)), src_out + 1);
    bp.connection_destroy(&con)?;
    assert!(!bp.connection_list(false)?.iter().any(|c| is_ours(c)));

    // 削除のエラーの後も呼べる
    assert_eq!(bp.process_list(false, None)?.len(), ids.len());
    Ok(())
}

/// Process・Container・ContainerProcess の作成と削除 (issue #15, #17, #19, #46)
pub fn check_create_destroy(bp: &mut dyn BrokerProxy) -> JuizResult<()> {
    let n_processes = bp.process_list(false, None)?.len();
    let created = bp.process_create(&ProcessManifest::new("increment_process").name("inc_new"))?;
    assert_eq!(created.instance_name(), "inc_new");
    let ids = bp.process_list(false, None)?;
    assert_eq!(ids.len(), n_processes + 1);
    let inc_new = find_process(&ids, "inc_new").clone();
    assert_eq!(int_of(bp.process_call(&inc_new, args("arg1", 1))), 2);
    assert!(bp
        .process_create(&ProcessManifest::new("no_such_type").name("x"))
        .is_err());
    assert_eq!(
        bp.process_destroy(&created.identifier())?.instance_name(),
        "inc_new"
    );
    assert_eq!(bp.process_list(false, None)?.len(), n_processes);
    assert!(bp.process_call(&inc_new, args("arg1", 1)).is_err());

    // コンテナを作り、そこにコンテナプロセスを追加する
    let n_containers = bp.container_list(false, None)?.len();
    let n_cprocs = bp.container_process_list(false, None)?.len();
    let cprofile = bp.container_create(
        &ContainerManifest::new("example_container").name("c_new"),
        vec![("name", jvalue!("c_new")), ("initial_value", jvalue!(5))].into(),
    )?;
    assert_eq!(cprofile.instance_name(), "c_new");
    let cid = cprofile.identifier();
    assert_eq!(bp.container_list(false, None)?.len(), n_containers + 1);
    assert_eq!(bp.container_profile(&cid)?.instance_name(), "c_new");
    let pprofile = bp.container_process_create(
        &cid,
        &ProcessManifest::new("example_container_get").name("c_new_get"),
    )?;
    assert_eq!(pprofile.instance_name(), "c_new_get");
    // マニフェストに Container を書かなくても、結びつけた Container が ID に入り、
    // 返ってきたプロファイルの ID でそのまま呼び出せる (issue #46)
    assert_eq!(pprofile.container_name.as_deref(), Some("c_new"));
    assert_eq!(
        int_of(bp.container_process_call(&pprofile.identifier(), CapsuleMap::new())),
        5
    );
    let cprocs = bp.container_process_list(false, None)?;
    assert_eq!(cprocs.len(), n_cprocs + 1);
    let pid = find_process(&cprocs, "c_new_get").clone();
    assert_eq!(
        int_of(bp.container_process_call(&pid, CapsuleMap::new())),
        5
    );
    assert!(bp
        .container_process_create(&cid, &ProcessManifest::new("no_such_type").name("x"))
        .is_err());

    // コンテナプロセスを削除できる (issue #17)
    assert_eq!(
        bp.container_process_destroy(&pid)?.instance_name(),
        "c_new_get"
    );
    assert_eq!(bp.container_process_list(false, None)?.len(), n_cprocs);
    assert!(bp.container_process_call(&pid, CapsuleMap::new()).is_err());

    // コンテナプロセスを持つコンテナも削除できる (issue #15, #17)
    bp.container_process_create(
        &cid,
        &ProcessManifest::new("example_container_get").name("c_new_get2"),
    )?;
    assert_eq!(bp.container_process_list(false, None)?.len(), n_cprocs + 1);
    assert_eq!(bp.container_destroy(&cid)?.instance_name(), "c_new");
    assert_eq!(bp.container_list(false, None)?.len(), n_containers);
    assert_eq!(bp.container_process_list(false, None)?.len(), n_cprocs);
    assert!(bp.container_profile(&cid).is_err());
    Ok(())
}
