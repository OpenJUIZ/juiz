//! これ一つで使えるようにしたいぞ宣言
//! 


pub use crate::{
    identifier::{
        Identifier,
        IdentifierStruct,
        connection_identifier_new,
    },
    manifests::{
        ArgumentManifest, ArgumentType, 
        ProcessManifest,
        Description,
        ContainerManifest,
        ComponentManifest,
        ContainerIdentifier,
        TopicManifest,
        ProcessProfile,
        ContainerProfile, 
        TopicProfile,
    },
    value::{
        jvalue, Value, 
        Capsule, 
        CapsuleMap,
        CapsulePtr,
        load_str,
        obj_get_str,
        obj_get_bool,
        obj_get_f64,
        obj_get_i64,
        obj_get_mut,
        obj_get,
        obj_get_hashmap,
        obj_get_obj,
        obj_get_array,
        obj_merge,
        obj_merge_mut,
        obj_insert,
        as_obj,
        capsule_to_value,
        value_to_capsule,
        value_merge,
    }, 
    result:: {
        JuizResult,
        JuizError,
    },
    utils::{
        juiz_lock,
        juiz_borrow,
        juiz_borrow_mut,
        juiz_try_lock,
        get_array,
        get_array_mut,
        get_hashmap,
        get_hashmap_mut,
        get_str,
        get_value,
        check_connection_manifest,
        when_contains_do,
        when_contains_do_mut,
    },
    factory::{
        process_factory,
        ProcessFactoryStruct,
        container_factory,
        container_process_factory,
        ContainerFactoryStruct,
        ContainerProcessFactoryStruct,
        container_stack_factory,
        ContainerStackFactoryStruct,
    },
    containers::{
        Container,
        ContainerPtr,
        ContainerImpl,
    },
    connections::{
        Connection,
        ConnectionType,
        SourceConnection,
        DestinationConnection,
        ConnectionManifest,
    },
    object::{
        JuizObject,
        JuizObjectCoreHolder, 
        JuizObjectClass,
        ObjectCore},
    log,
    image::DynamicImage,

};

pub use image;

pub use juiz_macro::{juiz_process, juiz_container, juiz_container_process, juiz_component_process, juiz_component_container, juiz_component_container_process, juiz_component_manifest};
pub use serde_json;
pub use env_logger;