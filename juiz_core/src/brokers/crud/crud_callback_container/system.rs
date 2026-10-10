use std::path::PathBuf;
use juiz_sdk::{prelude::*, anyhow::anyhow, log, value::{jvalue, value_to_capsule, Value}};

use crate::brokers::SystemBrokerProxy;

use super::CallbackContainerType;




pub(crate) fn create_read_callbacks() -> CallbackContainerType {
    let mut system_callbacks = CallbackContainerType::new();
    system_callbacks.insert("profile_full", |_crud, cb, _args| {
        log::debug!("[READ  ] system/profile_full called");
        Ok(value_to_capsule(cb.lock()?.system_profile_full()?))
    });
    system_callbacks.insert("uuid", |_crud, cb, _args| {
        log::debug!("[READ  ] system/uuid called");
        Ok(value_to_capsule(cb.lock()?.system_uuid()?))
    });
    system_callbacks.insert("filesystem_list", |_crud, cb, _args| {
        log::debug!("[READ  ] system/filesystem_list called");
        let param = _args.get_params();
        let mut path = ".".to_owned();
        if param.contains_key("path") {
            path = param.get("path").unwrap().clone();
        }
        Ok(value_to_capsule(cb.lock()?.system_filesystem_list(PathBuf::from(path))?))
    });
    system_callbacks
}

pub(crate) fn create_update_callbacks() -> CallbackContainerType {
    let mut system_callbacks = CallbackContainerType::new();
    system_callbacks.insert("add_subsystem", |_crud,cb, args| {
        log::debug!("[UPDATE] system/add_subsystem called");
        let param = args.get_params();
        let  mut manif: Value = args.get("profile")?.extract_value()?;
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
        Ok(value_to_capsule( match cb.lock_mut()?.system_add_subsystem(manif) {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("system_add_subsystem() failed. Error: {e:}");
                Err(e)
            }
        }?))
    });
    system_callbacks.insert("add_mastersystem", |_crud,cb, args| {
        log::debug!("[UPDATE] system/add_mastersystem called");
        let param = args.get_params();
        let  mut manif: Value = args.get("profile")?.extract_value()?;
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
        Ok(value_to_capsule(cb.lock_mut()?.system_add_mastersystem(manif)?))
    });
    system_callbacks.insert("load_process", |_crud, cb, args| {
        log::debug!("[UPDATE] system/load_process called");
        let filepath=  match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_process need 'filepath' argument.".to_owned() }))
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_process need 'language' argument.".to_owned() }))
        }?;
        Ok(value_to_capsule(cb.lock_mut()?.system_load_process(language, filepath)?))
    });    
    system_callbacks.insert("load_container", |_crud, cb, args| {
        log::debug!("[UPDATE] system/load_container called");
        let filepath=  match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_container need 'filepath' argument.".to_owned() }))
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_container need 'language' argument.".to_owned() }))
        }?;
        Ok(value_to_capsule(cb.lock_mut()?.system_load_container(language, filepath)?))
    }); 
    system_callbacks.insert("load_container_process", |_crud, cb, args| {
        log::debug!("[UPDATE] system/load_container_process called");
        let filepath=  match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_container_process need 'filepath' argument.".to_owned() }))
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_container_process need 'language' argument.".to_owned() }))
        }?;
        Ok(value_to_capsule(cb.lock_mut()?.system_load_container_process(language, filepath)?))
    }); 
    system_callbacks.insert("load_component", |_crud, cb, args| {
        log::debug!("[UPDATE] system/load_component called");
        let filepath=  match args.get("filepath")?.extract_value()?.as_str() {
            Some(fp_str) => Ok(fp_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_container need 'filepath' argument.".to_owned() }))
        }?;
        let language = match args.get("language")?.extract_value()?.as_str() {
            Some(lang_str) => Ok(lang_str.to_owned()),
            None => Err(anyhow!(JuizError::InvalidValueError { message: "system_load_container need 'language' argument.".to_owned() }))
        }?;
        let manifest: ComponentManifest = cb.lock_mut()?.system_load_component(language, filepath)?;
        Ok(serde_json::to_value(manifest)?.into())
    }); 
    system_callbacks
}