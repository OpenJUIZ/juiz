//! マニフェスト / プロファイル / 接続定義のテスト

use juiz_sdk::connection_identifier::ConnectionIdentifier;
use juiz_sdk::connections::ConnectionProfile;
use juiz_sdk::manifests::{ArgumentProfile, OutputProfile, PrimitiveProfile, StructProfile};
use juiz_sdk::prelude::*;
use juiz_sdk::process_identifier::ProcessIdentifier;
use std::collections::HashMap;

#[test]
fn argument_type_string_conversions() -> JuizResult<()> {
    for name in ["Bool", "Int", "Float", "String", "Array", "Object", "Image"] {
        let t: ArgumentType = name.try_into()?;
        assert_eq!(t.as_str(), name);
        assert_eq!(format!("{}", t), name);
    }
    assert!(ArgumentType::try_from("int").is_err());
    Ok(())
}

#[test]
fn argument_manifest_constructors_and_type_check() -> JuizResult<()> {
    let a = ArgumentManifest::new_bool("b", true).description("flag");
    assert_eq!(a.type_name, ArgumentType::Bool);
    assert_eq!(a.description.as_str(), "flag");
    assert_eq!(ArgumentManifest::new_int("i", 1).default, jvalue!(1));
    assert_eq!(
        ArgumentManifest::new_float("f", 1.5).type_name,
        ArgumentType::Float
    );
    assert_eq!(ArgumentManifest::new_string("s", "x").default, jvalue!("x"));
    assert_eq!(
        ArgumentManifest::new_array("a", vec![jvalue!(1)]).default,
        jvalue!([1])
    );
    assert_eq!(
        ArgumentManifest::new_object("o", jvalue!({"k": 1})).type_name,
        ArgumentType::Object
    );
    assert_eq!(ArgumentManifest::new_image("img").default, jvalue!({}));
    assert!(format!("{}", ArgumentManifest::new_int("i", 1)).contains("i::Int"));

    let cases = [
        (ArgumentType::Bool, jvalue!(true), jvalue!(1)),
        (ArgumentType::Int, jvalue!(1), jvalue!(1.5)),
        (ArgumentType::Float, jvalue!(1.5), jvalue!("x")),
        (ArgumentType::String, jvalue!("x"), jvalue!(1)),
        (ArgumentType::Array, jvalue!([]), jvalue!({})),
        (ArgumentType::Object, jvalue!({}), jvalue!([])),
        (ArgumentType::Image, Value::Null, jvalue!(1)),
    ];
    for (t, ok, ng) in cases {
        assert!(
            ArgumentManifest::new_with_check(t.clone(), "a", "".into(), ok.clone()).is_ok(),
            "{t} should accept {ok}"
        );
        assert!(
            ArgumentManifest::new_with_check(t.clone(), "a", "".into(), ng.clone()).is_err(),
            "{t} should reject {ng}"
        );
        assert!(ArgumentProfile::new_with_check(t.clone(), "a", "".into(), ok).is_ok());
        assert!(ArgumentProfile::new_with_check(t, "a", "".into(), ng).is_err());
    }
    // Image は空オブジェクトも受け付ける
    assert!(
        ArgumentManifest::new_with_check(ArgumentType::Image, "a", "".into(), jvalue!({})).is_ok()
    );
    Ok(())
}

#[test]
fn argument_manifest_deserializes_with_default_description() -> JuizResult<()> {
    let a: ArgumentManifest =
        serde_json::from_value(jvalue!({"type_name": "Int", "name": "x", "default": 3}))?;
    assert_eq!(a.description.as_str(), "");
    assert_eq!(a.default, jvalue!(3));
    let a: ArgumentManifest = serde_json::from_value(
        jvalue!({"type_name": "Int", "name": "x", "default": 3, "description": "d"}),
    )?;
    assert_eq!(a.description.as_str(), "d");
    Ok(())
}

#[test]
fn argument_profile_constructors() {
    let p: ArgumentProfile = ArgumentManifest::new_int("i", 2).description("int").into();
    assert_eq!(p.name, "i");
    assert_eq!(p.description.as_str(), "int");
    assert_eq!(
        ArgumentProfile::new_bool("b", false).default,
        jvalue!(false)
    );
    assert_eq!(
        ArgumentProfile::new_float("f", 0.5).type_name,
        ArgumentType::Float
    );
    assert_eq!(ArgumentProfile::new_string("s", "v").default, jvalue!("v"));
    assert_eq!(ArgumentProfile::new_array("a", vec![]).default, jvalue!([]));
    assert_eq!(
        ArgumentProfile::new_object("o", jvalue!({})).default,
        jvalue!({})
    );
    assert_eq!(ArgumentProfile::new_image("img").name, "img");
    assert!(format!("{}", p).starts_with("ArgumentProfile(i::Int"));
}

#[test]
fn description_conversions() -> JuizResult<()> {
    let d = Description::new("text");
    assert_eq!(d.as_str(), "text");
    assert_eq!(format!("{}", d), "Description(\"text\")");
    let v: Value = d.clone().into();
    assert_eq!(v, jvalue!("text"));
    assert_eq!(Description::try_from(jvalue!("t2"))?.to_str(), "t2");
    assert!(Description::try_from(jvalue!(1)).is_err());
    let d: Description = serde_json::from_value(jvalue!("from json"))?;
    assert_eq!(d.as_str(), "from json");
    Ok(())
}

#[test]
fn topic_manifest_and_profile() -> JuizResult<()> {
    let t = TopicManifest::new("topic");
    assert_eq!(format!("{}", t), "TopicManifest(topic)");
    let v: Value = t.clone().into();
    assert_eq!(v, jvalue!("topic"));
    assert_eq!(TopicManifest::try_from(jvalue!("x"))?.name, "x");
    assert!(TopicManifest::try_from(jvalue!(1)).is_err());

    let p: TopicProfile = t.into();
    assert_eq!(p.name, "topic");
    assert_eq!(format!("{}", p), "TopicProfile(topic)");
    let v: Value = p.into();
    assert_eq!(v, jvalue!("topic"));
    assert_eq!(TopicProfile::try_from(jvalue!("y"))?.name, "y");
    assert!(TopicProfile::try_from(jvalue!(1)).is_err());
    Ok(())
}

#[test]
fn output_profile_variants() -> JuizResult<()> {
    let d = OutputProfile::default();
    assert!(d.is_primitive());
    assert!(!d.is_struct());
    assert!(d.as_struct().is_none());
    assert_eq!(d.as_primitive().unwrap().type_name, "int");
    assert_eq!(format!("{}", d), "OutputProfile(int=0)");

    let mut members = HashMap::new();
    members.insert(
        "x".to_owned(),
        OutputProfile::Primitive(PrimitiveProfile {
            type_name: "float".to_owned(),
            default_value: None,
        }),
    );
    let s = OutputProfile::Struct(StructProfile {
        type_name: "Point".to_owned(),
        members,
    });
    assert!(s.is_struct());
    assert!(s.as_primitive().is_none());
    assert_eq!(s.as_struct().unwrap().type_name, "Point");
    assert_eq!(format!("{}", s), "OutputProfile(struct Point {x::float})");

    // serde のタグ付き表現で往復できる
    let v = serde_json::to_value(&s)?;
    assert_eq!(v["enum_type"], jvalue!("Struct"));
    let back: OutputProfile = serde_json::from_value(v)?;
    assert!(back.is_struct());
    Ok(())
}

fn sample_process_manifest() -> ProcessManifest {
    ProcessManifest::new("adder")
        .name("adder0")
        .description("adds")
        .factory("custom_factory")
        .language("python")
        .use_memo(true)
        .add_int_arg("a", "first", 1)
        .add_float_arg("b", "second", 2.0)
        .add_bool_arg("c", "flag", false)
        .add_string_arg("d", "text", "x")
        .add_array_arg("e", "list", vec![jvalue!(1)])
        .add_object_arg("f", "obj", jvalue!({}))
        .add_image_arg("g", "image")
        .publishes("out_topic")
        .subscribes("a", "in_topic")
        .set_outputs(Some(OutputProfile::default()))
}

#[test]
fn process_manifest_builder() -> JuizResult<()> {
    let m = sample_process_manifest();
    assert_eq!(m.name.as_deref(), Some("adder0"));
    assert_eq!(m.factory, "custom_factory");
    assert_eq!(m.language, "python");
    assert!(m.use_memo);
    assert_eq!(m.arguments.len(), 7);
    assert_eq!(m.publishes[0].name, "out_topic");
    assert_eq!(m.subscribes["a"].name, "in_topic");
    assert!(m.outputs().unwrap().is_primitive());

    let s = format!("{}", m);
    assert!(s.starts_with("ProcessManifest(adder0::adder"));
    assert!(s.contains("TopicManifest(out_topic)"));
    assert!(s.contains("a:TopicManifest(in_topic)"));
    assert!(format!("{}", ProcessManifest::new("t")).starts_with("ProcessManifest(None::t"));

    let id = m.identifier()?;
    assert_eq!(id.to_string(), "core://core/process/adder0::adder");
    Ok(())
}

#[test]
fn process_manifest_container_settings() -> JuizResult<()> {
    let c = ContainerManifest::new("counter").name("counter0");
    let m = ProcessManifest::new("get").name("get0").container(c);
    assert_eq!(m.container_type.as_deref(), Some("counter"));
    assert_eq!(m.container_name.as_deref(), Some("counter0"));
    let id = m.identifier()?;
    assert_eq!(id.class_name, "container_process");
    assert_eq!(
        id.to_string(),
        "core://core/container_process/get0::get:counter0"
    );

    let m = ProcessManifest::new("get")
        .container_type(Some("t".to_owned()))
        .container_name(None);
    assert_eq!(m.container_type.as_deref(), Some("t"));
    assert!(m.container_name.is_none());
    Ok(())
}

#[test]
fn process_manifest_build_instance_manifest_overrides_defaults() -> JuizResult<()> {
    let factory_manifest = ProcessManifest::new("adder")
        .description("adds")
        .use_memo(true)
        .add_int_arg("a", "first", 1)
        .add_int_arg("b", "second", 2)
        .set_outputs(Some(OutputProfile::default()));
    // インスタンス側は名前と一部の引数のデフォルト値だけ指定する
    let partial = ProcessManifest::new("ignored")
        .name("adder1")
        .add_int_arg("b", "", 20);
    let inst = factory_manifest.build_instance_manifest(partial)?;
    assert_eq!(inst.type_name, "adder");
    assert_eq!(inst.name.as_deref(), Some("adder1"));
    assert_eq!(inst.description.as_str(), "adds");
    assert!(inst.use_memo);
    assert_eq!(inst.arguments.len(), 2);
    assert_eq!(inst.arguments[0].default, jvalue!(1));
    assert_eq!(inst.arguments[1].default, jvalue!(20));
    // 説明はファクトリ側のものが使われる
    assert_eq!(inst.arguments[1].description.as_str(), "second");
    assert!(inst.outputs.is_some());
    Ok(())
}

#[test]
fn process_manifest_from_value() -> JuizResult<()> {
    let m: ProcessManifest = jvalue!({
        "type_name": "adder",
        "name": "adder0",
        "description": "adds",
        "arguments": [{"name": "a", "type_name": "Int", "default": 1, "description": "first"}],
        "factory": "f",
        "use_memo": true,
        "container_type": "ct",
        "container_name": "cn",
        "publishes": ["p1", "p2"],
        "subscribes": {"a": "s1"},
        "outputs": {"enum_type": "Primitive", "type_name": "int", "default_value": null}
    })
    .try_into()?;
    assert_eq!(m.type_name, "adder");
    assert_eq!(m.name.as_deref(), Some("adder0"));
    assert_eq!(m.description.as_str(), "adds");
    assert_eq!(m.arguments[0].name, "a");
    assert_eq!(m.factory, "f");
    assert!(m.use_memo);
    assert_eq!(m.container_type.as_deref(), Some("ct"));
    assert_eq!(m.container_name.as_deref(), Some("cn"));
    assert_eq!(m.publishes.len(), 2);
    assert_eq!(m.subscribes["a"].name, "s1");
    assert!(m.outputs.unwrap().is_primitive());

    // 省略時のデフォルト
    let m: ProcessManifest = jvalue!({"type_name": "t"}).try_into()?;
    assert_eq!(m.description.as_str(), "");
    assert_eq!(m.language, "rust");
    assert_eq!(m.factory, "process_factory");
    assert!(!m.use_memo);
    assert!(m.name.is_none());

    assert!(ProcessManifest::try_from(jvalue!({"name": "no_type"})).is_err());
    Ok(())
}

#[test]
fn process_manifest_serde_defaults() -> JuizResult<()> {
    let m: ProcessManifest = serde_json::from_value(jvalue!({"name": null, "type_name": "t"}))?;
    assert_eq!(m.factory, "process_factory");
    assert_eq!(m.language, "rust");
    assert_eq!(m.broker_type_name, "core");
    assert_eq!(m.broker_name, "core");
    assert!(m.outputs.is_none());
    Ok(())
}

#[test]
fn process_profile_from_manifest() -> JuizResult<()> {
    let p: ProcessProfile = sample_process_manifest().try_into()?;
    assert_eq!(p.name, "adder0");
    assert_eq!(p.language, "python");
    assert_eq!(p.arguments.len(), 7);
    assert_eq!(p.publishes[0].name, "out_topic");
    assert_eq!(p.subscribes["a"].name, "in_topic");
    assert_eq!(
        p.identifier().to_string(),
        "core://core/process/adder0::adder"
    );
    let s = format!("{}", p);
    assert!(s.starts_with("ProcessProfile(adder0::adder"));
    assert!(s.contains("outputs=OutputProfile(int=0)"));

    // 名前が無いマニフェストはプロファイルにできない
    assert!(ProcessProfile::try_from(ProcessManifest::new("t")).is_err());
    Ok(())
}

#[test]
fn process_profile_builder_and_instance_manifest() -> JuizResult<()> {
    let p = ProcessProfile::new("p0", "p")
        .name("p1")
        .language("cpp")
        .use_memo(true)
        .factory("fac")
        .description("desc")
        .add_bool_arg("b", "", true)
        .add_int_arg("i", "", 1)
        .add_float_arg("f", "", 1.0)
        .add_array_arg("a", "", vec![])
        .add_object_arg("o", "", jvalue!({}))
        .add_image_arg("img", "")
        .add_string_arg("s", "", "x")
        .publishes("t1")
        .subscribes("i", "t2")
        .container(ContainerManifest::new("ct").name("cn"));
    assert_eq!(p.name, "p1");
    assert_eq!(p.arguments.len(), 7);
    assert_eq!(p.container_type.as_deref(), Some("ct"));
    let id = p.identifier();
    assert_eq!(id.class_name, "container_process");
    assert_eq!(id.container_name.as_deref(), Some("cn"));
    assert!(format!("{}", p).contains("outputs=null"));

    let p = p
        .container_type(None)
        .container_name(None)
        .set_outputs(Some(OutputProfile::default()));
    assert_eq!(p.identifier().class_name, "process");
    assert!(p.outputs().is_some());

    let partial = ProcessProfile::new("inst", "ignored").add_int_arg("i", "", 100);
    let inst = p.build_instance_manifest(partial)?;
    assert_eq!(inst.type_name, "p");
    assert_eq!(inst.name, "inst");
    assert_eq!(inst.arguments.len(), 7);
    assert_eq!(
        inst.arguments
            .iter()
            .find(|a| a.name == "i")
            .unwrap()
            .default,
        jvalue!(100)
    );
    assert_eq!(
        inst.arguments
            .iter()
            .find(|a| a.name == "s")
            .unwrap()
            .default,
        jvalue!("x")
    );
    Ok(())
}

#[test]
fn container_manifest_builder_and_processes() -> JuizResult<()> {
    let c = ContainerManifest::new("counter")
        .name("counter0")
        .language("cpp")
        .description("counts")
        .factory("fac")
        .parent_type_name("parent_t")
        .parent_name("parent0")
        .add_int_arg("init", "", 0)
        .add_float_arg("f", "", 0.0)
        .add_bool_arg("b", "", false)
        .add_object_arg("o", "", jvalue!({}))
        .add_string_arg("s", "", "")
        .add_image_arg("img", "")
        .add_array_arg("a", "", vec![])
        .add_process(ProcessManifest::new("get").name("get0"));
    assert_eq!(c.arguments.len(), 7);
    assert_eq!(c.processes.len(), 1);
    // add_process でコンテナの情報がプロセスに書き込まれる
    assert_eq!(c.processes[0].container_type.as_deref(), Some("counter"));
    assert_eq!(c.processes[0].container_name.as_deref(), Some("counter0"));
    let s = format!("{}", c);
    assert!(s.starts_with("ContainerManifest(\"counter\", \"cpp\""));
    assert!(s.contains("ProcessManifest(get0::get"));

    let parent = c.parent_container_manifest();
    assert_eq!(parent.type_name, "parent_t");
    assert_eq!(parent.name.as_deref(), Some("parent0"));
    assert_eq!(parent.factory, "component_factory");

    let inst = c.build_instance_manifest(ContainerManifest::new("ignored").name("counter1"))?;
    assert_eq!(inst.type_name, "counter");
    assert_eq!(inst.name.as_deref(), Some("counter1"));
    assert_eq!(inst.description.as_str(), "counts");

    assert_eq!(c.identifier()?.class_name, "container");
    Ok(())
}

#[test]
fn container_profile_from_manifest() -> JuizResult<()> {
    let c = ContainerManifest::new("counter")
        .name("counter0")
        .add_int_arg("init", "", 0)
        .add_process(ProcessManifest::new("get").name("get0"));
    let p: ContainerProfile = c.try_into()?;
    assert_eq!(p.name, "counter0");
    assert_eq!(p.type_name, "counter");
    assert_eq!(p.arguments.len(), 1);
    assert_eq!(p.processes.len(), 1);
    assert!(format!("{}", p).contains("ProcessProfile(get0::get"));

    // 名前が無いとプロファイルにできない
    assert!(ContainerProfile::try_from(ContainerManifest::new("counter")).is_err());
    // プロセスに名前が無い場合もエラー
    let c = ContainerManifest::new("counter")
        .name("c0")
        .add_process(ProcessManifest::new("get"));
    let err = ContainerProfile::try_from(c).unwrap_err();
    // エラーの原因にコンテナとプロセスの型名が含まれる
    let msg = format!("{:#}", err);
    assert!(msg.contains("type_name=counter, name=c0"), "{msg}");
    assert!(msg.contains("type_name=get"), "{msg}");

    let p2 = ContainerProfile::new("counter", "c2");
    let inst = p.build_instance_manifest(p2)?;
    assert_eq!(inst.name, "c2");
    Ok(())
}

#[test]
fn component_manifest_routes_processes_to_containers() {
    let comp = ComponentManifest::new("comp")
        .language("cpp")
        .description("component")
        .add_container(ContainerManifest::new("counter").name("counter0"))
        .add_container(ContainerManifest::new("other"))
        .add_process(ProcessManifest::new("get").container_type(Some("counter".to_owned())))
        .add_process(ProcessManifest::new("standalone"));
    assert_eq!(comp.language, "cpp");
    assert_eq!(comp.processes.len(), 1);
    assert_eq!(comp.processes[0].type_name, "standalone");
    assert_eq!(comp.containers[0].processes.len(), 1);
    assert_eq!(comp.containers[0].processes[0].type_name, "get");
    assert_eq!(comp.containers[1].processes.len(), 0);
    let s = format!("{}", comp);
    assert!(s.starts_with("ComponentManifest(\"comp\", \"cpp\""));
    assert!(s.contains("ProcessManifest(None::standalone"));
}

#[test]
fn broker_manifest_conversions() -> JuizResult<()> {
    let b = BrokerManifest::new("http0", "http", "core", "core", "desc");
    assert_eq!(
        format!("{}", b),
        "BrokerManifest(http://http0,desc=Description(\"desc\"))"
    );

    let b: BrokerManifest = "core://core/http0::http".try_into()?;
    assert_eq!(b.name, "http0");
    assert_eq!(b.type_name, "http");
    let b: BrokerManifest = "core://core/http0::http".to_owned().try_into()?;
    assert_eq!(b.broker_type_name, "core");

    let b: BrokerManifest =
        (&jvalue!({"identifier": "core://core/http0::http", "description": "d"})).try_into()?;
    assert_eq!(b.description.as_str(), "d");
    let b: BrokerManifest = (&jvalue!("core://core/http0::http")).try_into()?;
    assert_eq!(b.description.as_str(), "");
    let b: BrokerManifest = (&jvalue!({"type_name": "http"})).try_into()?;
    assert_eq!(b.name, "http0");
    assert_eq!(b.broker_name, "core");

    assert!(BrokerManifest::try_from(&jvalue!({})).is_err());
    assert!(BrokerManifest::try_from(&jvalue!(1)).is_err());
    assert!(BrokerManifest::try_from("bad").is_err());
    Ok(())
}

#[test]
fn broker_profile_conversions() -> JuizResult<()> {
    let b = BrokerProfile::new("http0", "http", "core", "core", "desc");
    assert!(format!("{}", b).starts_with("BrokerProfile(http://http0"));
    let id: Identifier = b.clone().into();
    assert_eq!(id, "core://core/broker/http0::http");
    let v: Value = b.into();
    assert_eq!(
        v,
        jvalue!({
            "identifier": "core://core/broker/http0::http",
            "type_name": "http", "name": "http0", "broker_type_name": "core", "broker_name": "core", "description": "desc"
        })
    );

    let b: BrokerProfile = v.try_into()?;
    assert_eq!(b.name, "http0");
    assert_eq!(b.description.as_str(), "desc");
    let b: BrokerProfile =
        jvalue!({"identifier": "core://core/http0::http", "description": "d"}).try_into()?;
    assert_eq!(b.description.as_str(), "d");
    let b: BrokerProfile = jvalue!("core://core/http0::http").try_into()?;
    assert_eq!(b.type_name, "http");
    let b: BrokerProfile = "core://core/http0::http".to_owned().try_into()?;
    assert_eq!(b.name, "http0");

    assert!(BrokerProfile::try_from(jvalue!({})).is_err());
    assert!(BrokerProfile::try_from(jvalue!(1)).is_err());
    assert!(BrokerProfile::try_from("bad").is_err());
    Ok(())
}

#[test]
fn connection_type_parsing() {
    for s in ["pull", "PULL", "Pull"] {
        assert_eq!(ConnectionType::from(s), ConnectionType::Pull);
    }
    for s in ["push", "PUSH", "Push"] {
        assert_eq!(ConnectionType::from(s), ConnectionType::Push);
    }
    assert_eq!(ConnectionType::from("other"), ConnectionType::Unknown);
    assert_eq!(ConnectionType::Unknown.to_string(), "Unknown");
    assert_eq!(format!("{}", ConnectionType::Pull), "ConnectionType(Pull)");
}

fn pid(name: &str) -> ProcessIdentifier {
    ProcessIdentifier::new_process_id("core".into(), "core".into(), "t".into(), name.into())
}

#[test]
fn connection_manifest_conversions() -> JuizResult<()> {
    let m = ConnectionManifest::new(ConnectionType::Pull, pid("a"), "arg".to_owned(), pid("b"));
    let v: Value = m.clone().into();
    assert_eq!(
        v,
        jvalue!({
            "connection_type": "Pull",
            "source_process_id": "core://core/process/a::t",
            "destination_process_id": "core://core/process/b::t",
            "arg_name": "arg"
        })
    );
    // Value から戻せる
    let back: ConnectionManifest = v.try_into()?;
    assert_eq!(back.connection_type, ConnectionType::Pull);
    assert_eq!(back.source_process_id, pid("a"));
    assert_eq!(back.destination_process_id, pid("b"));

    // connection_type 省略時は push、識別子はオブジェクトでもよい
    let m2: ConnectionManifest = jvalue!({
        "source_process_id": {"identifier": "core://core/process/a::t"},
        "destination_process_id": "core://core/process/b::t",
        "arg_name": "arg"
    })
    .try_into()?;
    assert_eq!(m2.connection_type, ConnectionType::Push);
    assert_eq!(m2.source_process_id, pid("a"));

    assert!(ConnectionManifest::try_from(jvalue!({"source_process_id": "core://core/process/a::t", "destination_process_id": "core://core/process/b::t"})).is_err());
    assert!(ConnectionManifest::try_from(
        jvalue!({"source_process_id": {}, "destination_process_id": "x", "arg_name": "a"})
    )
    .is_err());
    assert!(ConnectionManifest::try_from(
        jvalue!({"source_process_id": 1, "destination_process_id": "x", "arg_name": "a"})
    )
    .is_err());
    assert!(ConnectionManifest::try_from(
        jvalue!({"destination_process_id": "x", "arg_name": "a"})
    )
    .is_err());

    let cid: ConnectionIdentifier = m.clone().into();
    assert_eq!(cid.arg_name, "arg");
    assert!(format!("{}", m).starts_with("ConnectionManifest(ConnectionType(Pull)"));
    Ok(())
}

#[test]
fn connection_manifest_from_capsule_map() -> JuizResult<()> {
    let cm: CapsuleMap = vec![
        ("connection_type", jvalue!("push")),
        ("source_process_id", jvalue!("core://core/process/a::t")),
        (
            "destination_process_id",
            jvalue!({"type_name": "t", "name": "b"}),
        ),
        ("arg_name", jvalue!("arg")),
    ]
    .into();
    let m: ConnectionManifest = cm.try_into()?;
    assert_eq!(m.connection_type, ConnectionType::Push);
    assert_eq!(m.source_process_id, pid("a"));
    assert_eq!(m.destination_process_id, pid("b"));
    assert_eq!(m.arg_name, "arg");

    let missing: CapsuleMap = vec![("connection_type", jvalue!("push"))].into();
    assert!(ConnectionManifest::try_from(missing).is_err());
    Ok(())
}

#[test]
fn connection_profile_conversions() -> JuizResult<()> {
    let m = ConnectionManifest::new(ConnectionType::Push, pid("a"), "arg".to_owned(), pid("b"));
    let p: ConnectionProfile = m.into();
    let v: Value = p.clone().into();
    assert_eq!(
        v,
        jvalue!({
            "type": "Push",
            "source": "core://core/process/a::t",
            "destination": "core://core/process/b::t",
            "arg_name": "arg"
        })
    );
    let back: ConnectionProfile = v.try_into()?;
    assert_eq!(back.source_process_id, pid("a"));
    assert_eq!(back.connection_type, ConnectionType::Push);

    let p2: ConnectionProfile = jvalue!({
        "source": {"identifier": "core://core/process/a::t"},
        "destination": "core://core/process/b::t",
        "arg_name": "arg"
    })
    .try_into()?;
    assert_eq!(p2.connection_type, ConnectionType::Push);
    assert!(ConnectionProfile::try_from(
        jvalue!({"source": {}, "destination": "x", "arg_name": "a"})
    )
    .is_err());
    assert!(ConnectionProfile::try_from(
        jvalue!({"source": 1, "destination": "x", "arg_name": "a"})
    )
    .is_err());
    assert!(ConnectionProfile::try_from(jvalue!({"destination": "x", "arg_name": "a"})).is_err());
    assert!(ConnectionProfile::try_from(
        jvalue!({"source": "core://core/process/a::t", "destination": "core://core/process/b::t"})
    )
    .is_err());

    let cm: CapsuleMap = vec![
        ("type", jvalue!("pull")),
        ("source", jvalue!("core://core/process/a::t")),
        ("destination", jvalue!("core://core/process/b::t")),
        ("arg_name", jvalue!("arg")),
    ]
    .into();
    let p3: ConnectionProfile = cm.try_into()?;
    assert_eq!(p3.connection_type, ConnectionType::Pull);

    let p4 = ConnectionProfile::new(ConnectionType::Pull, pid("a"), "x".to_owned(), pid("b"));
    let cid: ConnectionIdentifier = p4.clone().into();
    assert_eq!(cid.arg_name, "x");
    assert!(format!("{}", p4).starts_with("ConnectionProfile(ConnectionType(Pull)"));
    Ok(())
}

#[test]
fn object_core_and_class_names() -> JuizResult<()> {
    let classes = [
        (JuizObjectClass::Process(""), "process"),
        (JuizObjectClass::ProcessFactory(""), "process_factory"),
        (JuizObjectClass::Container(""), "container"),
        (JuizObjectClass::ContainerFactory(""), "container_factory"),
        (JuizObjectClass::ContainerProcess(""), "container_process"),
        (
            JuizObjectClass::ContainerProcessFactory(""),
            "container_process_factory",
        ),
        (JuizObjectClass::Connection(""), "connection"),
        (JuizObjectClass::ExecutionContext(""), "execution_context"),
        (
            JuizObjectClass::ExecutionContextFactory(""),
            "execution_context_factory",
        ),
        (JuizObjectClass::Broker(""), "broker"),
        (JuizObjectClass::BrokerFactory(""), "broker_factory"),
        (JuizObjectClass::BrokerProxy(""), "broker_proxy"),
        (
            JuizObjectClass::BrokerProxyFactory(""),
            "broker_proxy_factory",
        ),
        (JuizObjectClass::System(""), "system"),
        (JuizObjectClass::ProcessProxy(""), "process_proxy"),
        (JuizObjectClass::Topic(""), "topic"),
    ];
    for (c, s) in classes.iter() {
        assert_eq!(c.as_str(), *s);
        assert_eq!(format!("{}", c), format!("JuizObjectClass({})", s));
    }

    let core = ObjectCore::create(JuizObjectClass::Process("ProcessImpl"), "inc", "inc0");
    assert_eq!(core.identifier(), "core://core/process/inc0::inc");
    assert_eq!(core.profile_full()?["class_name"], jvalue!("process"));

    let fcore = ObjectCore::create_factory(JuizObjectClass::ProcessFactory("F"), "inc");
    assert_eq!(fcore.identifier(), "core://core/process_factory/inc::inc");

    let mut c2 = ObjectCore::new(
        "id".to_owned(),
        JuizObjectClass::Topic("T"),
        "t",
        "n",
        "b",
        "bt",
    );
    c2.set_identifier("id2".to_owned());
    assert_eq!(c2.clone().identifier(), "id2");

    struct Obj(ObjectCore);
    impl JuizObjectCoreHolder for Obj {
        fn core(&self) -> &ObjectCore {
            &self.0
        }
    }
    impl JuizObject for Obj {}
    let o = Obj(c2);
    assert_eq!(o.identifier(), "id2");
    assert_eq!(o.class_name().as_str(), "topic");
    assert_eq!(o.type_name(), "t");
    assert_eq!(o.name(), "n");
    assert_eq!(o.broker_name(), "b");
    assert_eq!(o.broker_type(), "bt");
    assert_eq!(o.profile_full()?["broker_type_name"], jvalue!("bt"));
    Ok(())
}

#[test]
fn process_manifest_language_from_value() -> JuizResult<()> {
    let m: ProcessManifest =
        jvalue!({"type_name": "t", "name": "t0", "language": "python"}).try_into()?;
    assert_eq!(m.name, Some("t0".to_owned()));
    assert_eq!(m.language, "python");
    Ok(())
}

#[test]
fn connection_identifier_string_round_trip() -> JuizResult<()> {
    let s = "connection://core://core/process/a0::a|arg|core://core/process/b0::b".to_owned();
    let c: ConnectionIdentifier = s.clone().try_into()?;
    let out: String = c.clone().into();
    assert_eq!(out, s);
    let back: ConnectionIdentifier = out.try_into()?;
    assert_eq!(back, c);
    let from_value: ConnectionIdentifier = jvalue!(s).try_into()?;
    assert_eq!(from_value, c);
    Ok(())
}

#[test]
fn container_identifier_has_name_then_type_name() -> JuizResult<()> {
    // issue #15: name と type_name が入れ替わらない
    let id = ContainerManifest::new("counter")
        .name("counter0")
        .identifier()?;
    assert_eq!(id.name, "counter0");
    assert_eq!(id.type_name, "counter");
    assert_eq!(id.to_string(), "core://core/container/counter0::counter");
    let parsed: ContainerIdentifier = id.to_string().try_into()?;
    assert_eq!(parsed, id);
    Ok(())
}

#[test]
fn description_deserializes_from_serialized_form() -> JuizResult<()> {
    // issue #19: Serialize した {"text": ...} の形からも読み戻せる
    let d = Description::new("hello");
    let v = serde_json::to_value(&d)?;
    assert_eq!(v, jvalue!({"text": "hello"}));
    let back: Description = serde_json::from_value(v)?;
    assert_eq!(back.text, "hello");
    let from_str: Description = serde_json::from_value(jvalue!("plain"))?;
    assert_eq!(from_str.text, "plain");
    Ok(())
}
