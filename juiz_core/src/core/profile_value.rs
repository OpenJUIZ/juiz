//! API で返す profile に識別子とクラス名を添える (issue #112)。
//!
//! ProcessProfile / ContainerProfile は識別子を持たないので、そのまま返すと
//! list 側の識別子と突き合わせないと同じ要素だと分からない。
//! 画面などから辿りやすいように、profile_full の応答に `identifier` と `class_name` を加える。

use juiz_sdk::manifests::{ContainerProfile, ProcessProfile};

use crate::prelude::*;

fn insert_identifier(mut value: Value, identifier: String, class_name: &str) -> JuizResult<Value> {
    let map = get_hashmap_mut(&mut value)?;
    map.insert("identifier".to_owned(), jvalue!(identifier));
    map.insert("class_name".to_owned(), jvalue!(class_name));
    Ok(value)
}

/// Process / ContainerProcess / Topic の profile を、識別子とクラス名つきの Value にする
pub(crate) fn process_profile_value(profile: &ProcessProfile) -> JuizResult<Value> {
    let id = profile.identifier();
    let class_name = id.class_name.clone();
    insert_identifier(
        serde_json::to_value(profile)?,
        id.into(),
        class_name.as_str(),
    )
}

/// Container の profile を、識別子とクラス名つきの Value にする。processes の各要素にも付ける
pub(crate) fn container_profile_value(profile: &ContainerProfile) -> JuizResult<Value> {
    let id = profile.identifier();
    let class_name = id.class_name.clone();
    let mut value = insert_identifier(
        serde_json::to_value(profile)?,
        id.into(),
        class_name.as_str(),
    )?;
    let processes = profile
        .processes
        .iter()
        .map(process_profile_value)
        .collect::<JuizResult<Vec<Value>>>()?;
    get_hashmap_mut(&mut value)?.insert("processes".to_owned(), processes.into());
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_profile_value_has_identifier() {
        let v = process_profile_value(&ProcessProfile::new("inc0", "increment_process")).unwrap();
        assert_eq!(
            v["identifier"],
            jvalue!("core://core/process/inc0::increment_process")
        );
        assert_eq!(v["class_name"], jvalue!("process"));
        assert_eq!(v["name"], jvalue!("inc0"));
    }

    #[test]
    fn container_process_profile_value_has_identifier() {
        let p = ProcessProfile::new("get0", "example_container_get")
            .container_name(Some("container0".to_owned()));
        let v = process_profile_value(&p).unwrap();
        assert_eq!(
            v["identifier"],
            jvalue!("core://core/container_process/get0::example_container_get:container0")
        );
        assert_eq!(v["class_name"], jvalue!("container_process"));
    }

    #[test]
    fn container_profile_value_has_identifier() {
        let v = container_profile_value(&ContainerProfile::new("example_container", "container0"))
            .unwrap();
        assert_eq!(
            v["identifier"],
            jvalue!("core://core/container/container0::example_container")
        );
        assert_eq!(v["class_name"], jvalue!("container"));
    }

    #[test]
    fn container_profile_value_has_process_identifiers() {
        let c = ContainerProfile::new("example_container", "container0")
            .add_process(ProcessProfile::new("get0", "example_container_get"));
        let v = container_profile_value(&c).unwrap();
        assert_eq!(
            v["processes"][0]["identifier"],
            jvalue!("core://core/container_process/get0::example_container_get:container0")
        );
    }
}
