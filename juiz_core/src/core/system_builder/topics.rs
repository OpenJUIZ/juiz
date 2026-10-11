use crate::prelude::*;

pub(super) fn setup_subscribe_topic(
    system: &System,
    process: ProcessPtr,
    arg_name: &String,
    sub_topic_info: TopicManifest,
) -> JuizResult<()> {
    log::trace!("setup_subscrie_topic(process, {arg_name}, {sub_topic_info:?}) called");
    let _topic_name = sub_topic_info.name.as_str();
    let r = system.core_broker().lock_mut().and_then(|mut cb| {
        let _id = process.identifier();
        cb.worker_mut()
            .process_subscribe_topic(process, arg_name, sub_topic_info)
    });
    r
}

pub(super) fn setup_publish_topic(
    system: &System,
    process: ProcessPtr,
    pub_topic_info: TopicManifest,
) -> JuizResult<()> {
    log::trace!("setup_publish_topic(process, {pub_topic_info:?}) called");
    let _topic_name = pub_topic_info.name.as_str();
    let r = system.core_broker().lock_mut().and_then(|mut cb| {
        let _id = process.identifier();
        cb.worker_mut()
            .process_publish_topic(process, pub_topic_info)
    });
    r
}
