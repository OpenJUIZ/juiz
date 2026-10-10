//! 識別子 (Identifier / ProcessIdentifier / ConnectionIdentifier など) のテスト

use juiz_sdk::connection_identifier::ConnectionIdentifier;
use juiz_sdk::identifier::{connection_identifier_split, identifier_from_manifest, identifier_new};
use juiz_sdk::manifests::{BrokerIdentifier, ContainerIdentifier};
use juiz_sdk::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;
use juiz_sdk::topic_identifier::TopicIdentifier;
use std::str::FromStr;

#[test]
fn identifier_new_lowercases_class_name() {
    let id = identifier_new("http", "localhost:8000", "Process", "increment", "inc0");
    assert_eq!(id, "http://localhost:8000/process/inc0::increment");
}

#[test]
fn connection_identifier_round_trip() -> JuizResult<()> {
    let src = "core://core/process/a0::a".to_owned();
    let dst = "core://core/process/b0::b".to_owned();
    let cid = connection_identifier_new(&src, &dst, "arg1");
    assert_eq!(cid, "connection://core://core/process/a0::a|arg1|core://core/process/b0::b");
    let (s, d, a) = connection_identifier_split(cid)?;
    assert_eq!(s, src);
    assert_eq!(d, dst);
    assert_eq!(a, "arg1");
    Ok(())
}

#[test]
fn connection_identifier_split_rejects_wrong_token_count() {
    assert!(connection_identifier_split("connection://a|b".to_owned()).is_err());
    assert!(connection_identifier_split("connection://a|b|c|d".to_owned()).is_err());
}

#[test]
fn identifier_from_manifest_prefers_identifier_key() -> JuizResult<()> {
    let m = jvalue!({"identifier": "core://core/process/x::y", "name": "ignored", "type_name": "ignored"});
    assert_eq!(identifier_from_manifest("core", "core", "Process", &m)?, "core://core/process/x::y");
    Ok(())
}

#[test]
fn identifier_from_manifest_uses_defaults_and_overrides() -> JuizResult<()> {
    let m = jvalue!({"name": "p0", "type_name": "p"});
    assert_eq!(identifier_from_manifest("core", "core", "Process", &m)?, "core://core/process/p0::p");

    let m = jvalue!({"name": "p0", "type_name": "p", "broker_name": "remote:8000", "broker_type_name": "http"});
    assert_eq!(identifier_from_manifest("core", "core", "Process", &m)?, "http://remote:8000/process/p0::p");
    Ok(())
}

#[test]
fn identifier_from_manifest_requires_name_and_type_name() {
    assert!(identifier_from_manifest("core", "core", "Process", &jvalue!({"name": "p0"})).is_err());
    assert!(identifier_from_manifest("core", "core", "Process", &jvalue!({"type_name": "p"})).is_err());
    assert!(identifier_from_manifest("core", "core", "Process", &jvalue!("not an object")).is_err());
}

#[test]
fn identifier_struct_parses_and_rebuilds() -> JuizResult<()> {
    let id = "http://localhost:8000/process/inc0::increment".to_owned();
    let s: IdentifierStruct = id.clone().try_into()?;
    assert_eq!(s.broker_type_name, "http");
    assert_eq!(s.broker_name, "localhost:8000");
    assert_eq!(s.class_name, "process");
    assert_eq!(s.object_name, "inc0");
    assert_eq!(s.type_name, "increment");
    assert_eq!(s.to_identifier(), id);

    assert_eq!(s.to_manifest(), jvalue!({
        "broker_type_name": "http",
        "broker_name": "localhost:8000",
        "type_name": "increment",
        "name": "inc0"
    }));
    assert_eq!(s.to_broker_manifest(), jvalue!({"type_name": "http", "name": "localhost:8000"}));

    let back: Identifier = s.into();
    assert_eq!(back, id);
    Ok(())
}

#[test]
fn identifier_struct_rejects_invalid() {
    let r: JuizResult<IdentifierStruct> = "not_an_identifier".to_owned().try_into();
    assert!(r.is_err());
}

#[test]
fn identifier_struct_set_class_name_and_new_object() {
    let mut s = IdentifierStruct::new_object("core", "core", "Process", "t", "o");
    assert_eq!(s.class_name, "process");
    s.set_class_name("container_process");
    assert_eq!(s.class_name, "container_process");
    assert_eq!(s.to_identifier(), "core://core/container_process/o::t");
}

#[test]
fn identifier_struct_broker_constructors() -> JuizResult<()> {
    let b = IdentifierStruct::new_broker("http", "localhost:8000");
    assert_eq!(b.identifier, "http://localhost:8000");
    assert_eq!(b.broker_type_name, "http");
    assert_eq!(b.broker_name, "localhost:8000");
    assert_eq!(b.class_name, "");

    let b2 = IdentifierStruct::new_broker_id("http://localhost:8000".to_owned())?;
    assert_eq!(b, b2);
    assert!(IdentifierStruct::new_broker_id("no-scheme".to_owned()).is_err());

    let b3 = IdentifierStruct::from_broker_identifier(&"core://core/broker/localhost:8000::http".to_owned())?;
    assert_eq!(b3.class_name, "broker");
    assert_eq!(b3.broker_type_name, "http");
    assert_eq!(b3.broker_name, "localhost:8000");
    assert_eq!(b3.type_name, "http");
    assert_eq!(b3.object_name, "localhost:8000");
    assert!(IdentifierStruct::from_broker_identifier(&"invalid".to_owned()).is_err());
    Ok(())
}

#[test]
fn process_identifier_constructors() {
    let p = ProcessIdentifier::new("core".into(), "core".into(), "inc".into(), None, "inc0".into());
    assert_eq!(p.class_name, "process");
    assert_eq!(p, ProcessIdentifier::new_process_id("core".into(), "core".into(), "inc".into(), "inc0".into()));
    assert_eq!(p.to_string(), "core://core/process/inc0::inc");

    let cp = ProcessIdentifier::new_container_process_id("core".into(), "core".into(), "get".into(), "cont0".into(), "get0".into());
    assert_eq!(cp.class_name, "container_process");
    assert_eq!(cp.type_name, "get:cont0");
    assert_eq!(cp.container_name.as_deref(), Some("cont0"));
    assert_eq!(cp.to_string(), "core://core/container_process/get0::get:cont0");

    let with_container = ProcessIdentifier::new("core".into(), "core".into(), "get".into(), Some("cont0".into()), "get0".into());
    assert_eq!(with_container.class_name, "container_process");
    assert!(format!("{}", p).contains("core://core/process/inc0::inc"));
}

#[test]
fn process_identifier_string_round_trip() -> JuizResult<()> {
    let p: ProcessIdentifier = "http://localhost:8000/process/inc0::inc".try_into()?;
    assert_eq!(p.broker_type_name, "http");
    assert_eq!(p.broker_name, "localhost:8000");
    assert_eq!(p.name, "inc0");
    assert_eq!(p.type_name, "inc");
    assert_eq!(p.container_name, None);
    let s: String = p.clone().into();
    assert_eq!(s, "http://localhost:8000/process/inc0::inc");

    let cp: ProcessIdentifier = "core://core/container_process/get0::get:cont0".to_owned().try_into()?;
    assert_eq!(cp.type_name, "get:cont0");
    assert_eq!(cp.container_name.as_deref(), Some("cont0"));
    let s: String = cp.into();
    assert_eq!(s, "core://core/container_process/get0::get:cont0");
    Ok(())
}

#[test]
fn process_identifier_rejects_invalid_strings() {
    // クラス名が不正
    assert!(ProcessIdentifier::try_from("core://core/container/c0::c").is_err());
    // container_process なのにコンテナ名が無い
    assert!(ProcessIdentifier::try_from("core://core/container_process/get0::get").is_err());
    // 形式が不正
    assert!(ProcessIdentifier::try_from("garbage").is_err());
}

#[test]
fn process_identifier_from_value() -> JuizResult<()> {
    let p = ProcessIdentifier::from_value(&jvalue!("core://core/process/inc0::inc"))?;
    assert_eq!(p.name, "inc0");

    let p = ProcessIdentifier::from_value(&jvalue!({"identifier": "core://core/process/inc0::inc"}))?;
    assert_eq!(p.name, "inc0");

    // 名前を省略すると type_name + "0" になり、ブローカーは core になる
    let p = ProcessIdentifier::from_value(&jvalue!({"type_name": "inc"}))?;
    assert_eq!(p.name, "inc0");
    assert_eq!(p.broker_name, "core");
    assert_eq!(p.broker_type_name, "core");
    assert_eq!(p.class_name, "process");

    let p = ProcessIdentifier::from_value(&jvalue!({
        "type_name": "get", "name": "g", "broker_type_name": "http", "broker_name": "h", "container_name": "c0"
    }))?;
    assert_eq!(p.class_name, "container_process");
    assert_eq!(p.container_name.as_deref(), Some("c0"));
    assert_eq!(p.broker_type_name, "http");

    assert!(ProcessIdentifier::from_value(&jvalue!({"name": "no_type"})).is_err());
    assert!(ProcessIdentifier::from_value(&jvalue!(1)).is_err());
    Ok(())
}

#[test]
fn connection_identifier_from_string_and_value() -> JuizResult<()> {
    let s = "connection://core://core/process/a0::a|arg1|core://core/process/b0::b".to_owned();
    let c: ConnectionIdentifier = s.clone().try_into()?;
    assert_eq!(c.source_identifier.name, "a0");
    assert_eq!(c.destination_identifier.name, "b0");
    assert_eq!(c.arg_name, "arg1");

    let c2: ConnectionIdentifier = jvalue!(s).try_into()?;
    assert_eq!(c, c2);

    let c3: ConnectionIdentifier = jvalue!({
        "source_identifier": "core://core/process/a0::a",
        "destination_identifier": "core://core/process/b0::b",
        "arg_name": "arg1"
    }).try_into()?;
    assert_eq!(c, c3);

    let c4 = ConnectionIdentifier::new(c.source_identifier.clone(), "arg1", c.destination_identifier.clone());
    assert_eq!(c, c4);
    assert!(format!("{}", c4).contains("arg=arg1"));
    Ok(())
}

#[test]
fn connection_identifier_rejects_invalid() {
    let r: JuizResult<ConnectionIdentifier> = "connection://a|b".to_owned().try_into();
    assert!(r.is_err());
    let r: JuizResult<ConnectionIdentifier> = jvalue!(1).try_into();
    assert!(r.is_err());
    let r: JuizResult<ConnectionIdentifier> = jvalue!({"arg_name": "x"}).try_into();
    assert!(r.is_err());
}

#[test]
fn topic_identifier_conversions() -> JuizResult<()> {
    let t = TopicIdentifier::new("topic1".to_owned());
    assert_eq!(TopicIdentifier::from("topic1"), t);
    assert_eq!(TopicIdentifier::from_str("topic1")?, t);
    assert_eq!(TopicIdentifier::try_from("topic1".to_owned())?, t);
    let s: String = t.clone().try_into()?;
    assert_eq!(s, "topic1");
    assert_eq!(format!("{}", t), "TopicID(name=topic1)");
    Ok(())
}

#[test]
fn broker_identifier_from_value_defaults() -> JuizResult<()> {
    let b: BrokerIdentifier = (&jvalue!({"type_name": "http"})).try_into()?;
    assert_eq!(b.name, "http0");
    assert_eq!(b.broker_type_name, "core");
    assert_eq!(b.broker_name, "core");
    assert_eq!(b.to_string(), "core://core/http0::http");

    let b: BrokerIdentifier = (&jvalue!("core://core/http0::http")).try_into()?;
    assert_eq!(b.name, "http0");
    let b: BrokerIdentifier = (&jvalue!({"identifier": "core://core/http0::http"})).try_into()?;
    assert_eq!(b.type_name, "http");

    assert!(BrokerIdentifier::try_from(&jvalue!({"name": "x"})).is_err());
    assert!(BrokerIdentifier::try_from(&jvalue!(1)).is_err());
    assert!(BrokerIdentifier::try_from("invalid").is_err());
    Ok(())
}

#[test]
fn container_identifier_string_round_trip() -> JuizResult<()> {
    let s = "core://core/container/c0::example_container".to_owned();
    let c: ContainerIdentifier = s.clone().try_into()?;
    assert_eq!(c.class_name, "container");
    assert_eq!(c.name, "c0");
    assert_eq!(c.type_name, "example_container");
    assert_eq!(c.to_string(), s);
    assert!(format!("{}", c).starts_with("ContainerIdentifier("));
    assert!(ContainerIdentifier::try_from("invalid".to_owned()).is_err());
    Ok(())
}
