//! Profile = Manifest + 実行時の状態 の型の JSON (issue #505、docs/spec/manifest-profile.md §8.1)

use juiz_sdk::manifests::{ArgumentProfile, BrokerProfile, ContainerProfile, ProcessProfile, TopicProfile};
use juiz_sdk::prelude::*;

#[test]
fn process_profile_puts_argument_values_into_arguments() -> JuizResult<()> {
    let mut p: ProcessProfile = ProcessManifest::new("inc")
        .name("inc0")
        .add_int_arg("a", "", 1)
        .add_int_arg("b", "", 2)
        .trace(true)
        .try_into()?;
    p.set_argument_value("a", Some(jvalue!(5)));
    let v = serde_json::to_value(&p)?;
    assert_eq!(v["name"], jvalue!("inc0"));
    assert_eq!(v["arguments"][0]["value"], jvalue!(5));
    // 値の無い引数には value を出さない
    assert!(v["arguments"][1].get("value").is_none());
    // 今の profile の JSON に無い manifest の項目は出さない (§8.1 の 3)
    assert!(v.get("trace").is_none());

    let back: ProcessProfile = serde_json::from_value(v.clone())?;
    assert_eq!(back.argument_values.get("a"), Some(&jvalue!(5)));
    assert!(back.argument_values.get("b").is_none());
    assert_eq!(back.manifest.arguments.len(), 2);
    assert_eq!(serde_json::to_value(&back)?, v);

    // value: null は値なしとして読む
    let mut v2 = v.clone();
    v2["arguments"][1]["value"] = Value::Null;
    let back: ProcessProfile = serde_json::from_value(v2)?;
    assert!(back.argument_values.get("b").is_none());

    // 名前の無い profile は読めない
    let mut v3 = v;
    v3["name"] = Value::Null;
    assert!(serde_json::from_value::<ProcessProfile>(v3).is_err());
    Ok(())
}

#[test]
fn process_profile_argument_profiles_and_manifest() -> JuizResult<()> {
    let p = ProcessProfile::new("p0", "p").add_arg(ArgumentProfile {
        value: Some(jvalue!(3)),
        ..ArgumentProfile::new_int("i", 1)
    });
    let args = p.argument_profiles();
    assert_eq!(args[0].name, "i");
    assert_eq!(args[0].value, Some(jvalue!(3)));
    assert_eq!(p.instance_name(), "p0");
    let m: ProcessManifest = p.into();
    assert_eq!(m.name.as_deref(), Some("p0"));
    assert_eq!(m.arguments[0].default, jvalue!(1));
    Ok(())
}

#[test]
fn container_profile_shows_state_instead_of_declarations() -> JuizResult<()> {
    let mut c: ContainerProfile = ContainerManifest::new("counter")
        .name("c0")
        .rate(10.0)
        .add_process(ProcessManifest::new("get").name("get0"))
        .try_into()?;
    c.model = Some(jvalue!({"format": "glb"}));
    c.processes[0].set_argument_value("x", Some(jvalue!(1)));
    let v = serde_json::to_value(&c)?;
    assert_eq!(v["name"], jvalue!("c0"));
    assert_eq!(v["model"], jvalue!({"format": "glb"}));
    assert_eq!(v["processes"][0]["name"], jvalue!("get0"));
    for key in ["broker_type_name", "broker_name", "rate", "slots", "anchor", "remote_children"] {
        assert!(v.get(key).is_none(), "{key}");
    }

    let back: ContainerProfile = serde_json::from_value(v.clone())?;
    assert_eq!(back.processes.len(), 1);
    assert_eq!(back.manifest.processes[0].name.as_deref(), Some("get0"));
    assert_eq!(back.model, Some(jvalue!({"format": "glb"})));
    assert_eq!(serde_json::to_value(&back)?, v);

    // モデルが無ければ model は null
    let c = ContainerProfile::new("counter", "c1");
    assert_eq!(serde_json::to_value(&c)?["model"], Value::Null);
    Ok(())
}

#[test]
fn broker_and_topic_profiles_have_manifest_shape() -> JuizResult<()> {
    let b = BrokerProfile::new("http0", "http", "core", "core", "desc");
    assert_eq!(serde_json::to_value(&b)?, serde_json::to_value(&b.manifest)?);
    assert_eq!(b.type_name, "http");
    let t = TopicProfile::new("topic0");
    assert_eq!(serde_json::to_value(&t)?, jvalue!({"name": "topic0"}));
    let t: TopicProfile = serde_json::from_value(jvalue!({"name": "topic1"}))?;
    assert_eq!(t.name, "topic1");
    Ok(())
}
