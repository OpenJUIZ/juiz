use juiz_sdk::{
    anyhow::anyhow,
    log,
    prelude::*,
    value::{jvalue, value_to_capsule, Value},
};
use std::path::PathBuf;

use crate::brokers::SystemBrokerProxy;

use super::CallbackContainerType;

pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut system_callbacks = CallbackContainerType::new();
    system_callbacks.insert("profile", |cb, args| {
        log::debug!("[READ  ] system/profile called");
        let recursive = args.get_param("recursive").is_some_and(|v| v == "true");
        if recursive {
            Ok(value_to_capsule(
                // 通ってきた System は CRUDBroker が置いている (issue #559)
                cb.lock()?
                    .system_profile_recursive_via(&crate::brokers::walk::current_via())?,
            ))
        } else {
            Ok(value_to_capsule(cb.lock()?.system_profile()?))
        }
    });
    // つながった System の木 (UUID だけ、issue #559)。通ってきた System は CRUDBroker が置いている
    system_callbacks.insert("walk", |cb, _args| {
        log::debug!("[READ  ] system/walk called");
        Ok(value_to_capsule(
            cb.lock()?
                .system_walk_via(&crate::brokers::walk::current_via())?,
        ))
    });
    system_callbacks.insert("uuid", |cb, _args| {
        log::debug!("[READ  ] system/uuid called");
        Ok(value_to_capsule(cb.lock()?.system_uuid()?))
    });
    system_callbacks.insert("filesystem_list", |cb, _args| {
        log::debug!("[READ  ] system/filesystem_list called");
        let param = _args.get_params();
        let mut path = ".".to_owned();
        if param.contains_key("path") {
            path = param.get("path").unwrap().clone();
        }
        Ok(value_to_capsule(
            cb.lock()?.system_filesystem_list(PathBuf::from(path))?,
        ))
    });
    system_callbacks
}

pub(crate) fn create_update_callbacks() -> CallbackContainerType {
    let mut system_callbacks = CallbackContainerType::new();
    system_callbacks.insert("add_subsystem", |cb, args| {
        log::debug!("[UPDATE] system/add_subsystem called");
        let param = args.get_params();
        let mut manif: Value = args.get("profile")?.extract_value()?;
        match manif.as_object_mut() {
            Some(obj) => {
                let accessed_broker_id = if param.contains_key("accessed_broker_id") {
                    jvalue!(param.get("accessed_broker_id").unwrap().clone())
                } else {
                    jvalue!("")
                };
                obj.insert("accessed_broker_id".to_owned(), accessed_broker_id);
            }
            None => {}
        }
        Ok(value_to_capsule(
            // ロックは CoreBrokerPtr 側で必要なときだけ取る (issue #159)
            match cb.system_add_subsystem(manif) {
                Ok(v) => Ok(v),
                Err(e) => {
                    log::error!("system_add_subsystem() failed. Error: {e:}");
                    Err(e)
                }
            }?,
        ))
    });
    system_callbacks.insert("add_mastersystem", |cb, args| {
        log::debug!("[UPDATE] system/add_mastersystem called");
        let param = args.get_params();
        let mut manif: Value = args.get("profile")?.extract_value()?;
        match manif.as_object_mut() {
            Some(obj) => {
                let accessed_broker_id = if param.contains_key("accessed_broker_id") {
                    jvalue!(param.get("accessed_broker_id").unwrap().clone())
                } else {
                    jvalue!("")
                };
                obj.insert("accessed_broker_id".to_owned(), accessed_broker_id);
            }
            None => {}
        }
        Ok(value_to_capsule(cb.system_add_mastersystem(manif)?))
    });
    system_callbacks.insert("remove_subsystem", |cb, args| {
        log::debug!("[UPDATE] system/remove_subsystem called");
        let uuid = match args.get("uuid")?.extract_value()?.as_str() {
            Some(s) => Ok(uuid::Uuid::parse_str(s)?),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_remove_subsystem need 'uuid' argument.".to_owned()
            })),
        }?;
        Ok(value_to_capsule(
            cb.lock_mut()?.system_remove_subsystem(uuid)?,
        ))
    });
    system_callbacks.insert("load_process", |cb, args| {
        log::debug!("[UPDATE] system/load_process called");
        let filepath = match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_process need 'filepath' argument.".to_owned()
            })),
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_process need 'language' argument.".to_owned()
            })),
        }?;
        Ok(value_to_capsule(
            cb.lock_mut()?.system_load_process(language, filepath)?,
        ))
    });
    system_callbacks.insert("load_container", |cb, args| {
        log::debug!("[UPDATE] system/load_container called");
        let filepath = match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_container need 'filepath' argument.".to_owned()
            })),
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_container need 'language' argument.".to_owned()
            })),
        }?;
        Ok(value_to_capsule(
            cb.lock_mut()?.system_load_container(language, filepath)?,
        ))
    });
    system_callbacks.insert("load_container_process", |cb, args| {
        log::debug!("[UPDATE] system/load_container_process called");
        let filepath = match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_container_process need 'filepath' argument.".to_owned()
            })),
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_container_process need 'language' argument.".to_owned()
            })),
        }?;
        Ok(value_to_capsule(
            cb.lock_mut()?
                .system_load_container_process(language, filepath)?,
        ))
    });
    system_callbacks.insert("load_component", |cb, args| {
        log::debug!("[UPDATE] system/load_component called");
        let filepath = match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_container need 'filepath' argument.".to_owned()
            })),
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError {
                message: "system_load_container need 'language' argument.".to_owned()
            })),
        }?;
        let manifest: ComponentManifest =
            cb.lock_mut()?.system_load_component(language, filepath)?;
        Ok(serde_json::to_value(manifest)?.into())
    });
    system_callbacks
}
