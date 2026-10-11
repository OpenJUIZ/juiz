use std::{
    ffi::CStr,
    sync::{Arc, Mutex},
};

use super::converter_error::*;
use crate::prelude::*;

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_value(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    let c = capsule.as_ref().unwrap();
    let is_value = c.lock().and_then(|cap| Ok(cap.is_value()));
    is_value.unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_int(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| {
                let v = cap.as_value().unwrap();
                Ok(v.is_i64())
            })
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_float(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| {
                let v = cap.as_value().unwrap();
                Ok(v.is_f64())
            })
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_bool(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| {
                let v = cap.as_value().unwrap();
                Ok(v.is_boolean())
            })
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_array(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| {
                let v = cap.as_value().unwrap();
                Ok(v.is_array())
            })
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_object(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| Ok(cap.as_value().unwrap().is_object()))
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_string(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| Ok(cap.as_value().unwrap().is_string()))
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_null(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| Ok(cap.as_value().unwrap().is_null()))
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_uint(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    if arc_capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        return c
            .lock()
            .and_then(|cap| Ok(cap.as_value().unwrap().is_u64()))
            .unwrap();
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_value(capsule: *mut Capsule) -> bool {
    let c = capsule.as_ref().unwrap();
    c.is_value()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_int(capsule: *mut Capsule) -> bool {
    if capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        c.as_value().unwrap().is_i64()
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_int(capsule: *mut Capsule, v: *mut i64) -> i64 {
    if capsule_is_value(capsule) {
        let val = capsule.as_ref().unwrap().as_value().unwrap();
        if !val.is_i64() {
            JUIZ_CAPSULE_TYPE_ERROR
        } else {
            *v = val.as_i64().unwrap();
            JUIZ_OK
        }
    } else {
        JUIZ_CAPSULE_NO_VALUE
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_int(capsule: *mut Capsule, v: i64) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!(v).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_uint(capsule: *mut Capsule) -> bool {
    if capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        c.as_value().unwrap().is_u64()
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_uint(capsule: *mut Capsule, v: *mut u64) -> i64 {
    if capsule_is_value(capsule) {
        let val = capsule.as_ref().unwrap().as_value().unwrap();
        if !val.is_u64() {
            JUIZ_CAPSULE_TYPE_ERROR
        } else {
            *v = val.as_u64().unwrap();
            JUIZ_OK
        }
    } else {
        JUIZ_CAPSULE_NO_VALUE
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_uint(capsule: *mut Capsule, v: u64) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!(v).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_float(capsule: *mut Capsule) -> bool {
    if capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        c.as_value().unwrap().is_f64()
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_float(capsule: *mut Capsule, v: *mut f64) -> i64 {
    if capsule_is_value(capsule) {
        let val = capsule.as_ref().unwrap().as_value().unwrap();
        if !val.is_f64() {
            JUIZ_CAPSULE_TYPE_ERROR
        } else {
            *v = val.as_f64().unwrap();
            JUIZ_OK
        }
    } else {
        JUIZ_CAPSULE_NO_VALUE
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_float(capsule: *mut Capsule, v: f64) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!(v).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_bool(capsule: *mut Capsule) -> bool {
    if capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        c.as_value().unwrap().is_boolean()
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_bool(capsule: *mut Capsule, v: *mut i64) -> i64 {
    if capsule_is_value(capsule) {
        let val = capsule.as_ref().unwrap().as_value().unwrap();
        if !val.is_boolean() {
            JUIZ_CAPSULE_TYPE_ERROR
        } else {
            *v = if val.as_bool().unwrap() { 1 } else { 0 };
            JUIZ_OK
        }
    } else {
        JUIZ_CAPSULE_NO_VALUE
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_bool(capsule: *mut Capsule, v: i64) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!(if v != 0 { true } else { false }).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_string(capsule: *mut Capsule) -> bool {
    if capsule_is_value(capsule) {
        let c = capsule.as_ref().unwrap();
        c.as_value().unwrap().is_string()
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_string(capsule: *mut Capsule, v: *mut *const u8) -> i64 {
    if capsule_is_value(capsule) {
        let val = capsule.as_ref().unwrap().as_value().unwrap();
        if !val.is_string() {
            JUIZ_CAPSULE_TYPE_ERROR
        } else {
            *v = val.as_str().unwrap().as_ptr();
            JUIZ_OK
        }
    } else {
        JUIZ_CAPSULE_NO_VALUE
    }
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_string(
    capsule: *mut Capsule,
    v: *const std::os::raw::c_char,
) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!(CStr::from_ptr(v).to_str().unwrap()).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_empty_object(capsule: *mut Capsule) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!({}).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_empty_array(capsule: *mut Capsule) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!([]).into());
    JUIZ_OK
}
