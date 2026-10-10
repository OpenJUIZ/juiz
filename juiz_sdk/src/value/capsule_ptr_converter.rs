
use std::{ffi::CStr, sync::{Arc, Mutex}};

use crate::prelude::*;
use super::converter_error::*;

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_get_value(capsule_ptr: *mut CapsulePtr) -> *mut Arc<Mutex<Capsule>> {
    capsule_ptr.as_mut().unwrap().value_mut()
}


#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_get_int(capsule_ptr: *mut CapsulePtr, v: *mut i64) -> i64 {
    capsule_ptr.as_ref().unwrap().lock_as_value(|val| {
        val.as_i64().and_then(|iv| { *v = iv; Some(JUIZ_OK)}).or(Some(JUIZ_VALUE_TYPE_ERROR)).unwrap()
    } ).or::<i64>(Ok(JUIZ_CAPSULEPTR_LOCK_ERROR)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_get_uint(capsule_ptr: *mut CapsulePtr, v: *mut u64) -> i64 {
    capsule_ptr.as_ref().unwrap().lock_as_value(|val| {
        val.as_u64().and_then(|iv| { *v = iv; Some(JUIZ_OK)}).or(Some(JUIZ_VALUE_TYPE_ERROR)).unwrap()
    } ).or::<i64>(Ok(JUIZ_CAPSULEPTR_LOCK_ERROR)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_get_float(capsule_ptr: *mut CapsulePtr, v: *mut f64) -> i64 {
    capsule_ptr.as_ref().unwrap().lock_as_value(|val| {
        val.as_f64().and_then(|iv| { *v = iv; Some(JUIZ_OK)}).or(Some(JUIZ_VALUE_TYPE_ERROR)).unwrap()
    } ).or::<i64>(Ok(JUIZ_CAPSULEPTR_LOCK_ERROR)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_get_bool(capsule_ptr: *mut CapsulePtr, v: *mut i64) -> i64 {
    capsule_ptr.as_ref().unwrap().lock_as_value(|val| {
        val.as_bool().and_then(|iv| { if iv { *v = 1 } else { *v = 0 }; Some(JUIZ_OK)}).or(Some(JUIZ_VALUE_TYPE_ERROR)).unwrap()
    } ).or::<i64>(Ok(JUIZ_CAPSULEPTR_LOCK_ERROR)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_get_string(capsule_ptr: *mut CapsulePtr, v: *mut *const u8) -> i64 {
    capsule_ptr.as_ref().unwrap().lock_as_value(|val| {
        val.as_str().and_then(|iv| { *v = iv.as_ptr(); Some(JUIZ_OK)}).or(Some(JUIZ_VALUE_TYPE_ERROR)).unwrap()
    } ).or::<i64>(Ok(JUIZ_CAPSULEPTR_LOCK_ERROR)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_lock_as_value(capsule_ptr: *mut CapsulePtr, callback: extern "C" fn(*mut Value) -> ()) -> () {
    let _ = capsule_ptr.as_mut().unwrap().lock_modify_as_value(|v|->() {
        callback(v)
    } );
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_lock_as_value_with_arg(capsule_ptr: *mut CapsulePtr, callback: extern "C" fn(*mut std::ffi::c_void, *mut Value) -> i64, arg: *mut std::ffi::c_void) -> i64 {
    capsule_ptr.as_mut().unwrap().lock_modify_as_value(|v|->i64 {
        callback(arg, v)
    } ).or::<i64>(Ok(JUIZ_CAPSULEPTR_LOCK_ERROR)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_int(capsule_ptr: *mut CapsulePtr, v: i64) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!(v));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_uint(capsule_ptr: *mut CapsulePtr, v: u64) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!(v));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_float(capsule_ptr: *mut CapsulePtr, v: f64) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!(v));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_bool(capsule_ptr: *mut CapsulePtr, v: i64) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!(v != 0));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_string(capsule_ptr: *mut CapsulePtr, v: *const std::os::raw::c_char) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!(CStr::from_ptr(v).to_str().unwrap()));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_empty_object(capsule_ptr: *mut CapsulePtr) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!({}));
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_ptr_set_empty_array(capsule_ptr: *mut CapsulePtr) -> i64 {
    let c = capsule_ptr.as_mut().unwrap();
    c.replace_with_value(jvalue!([]));
    JUIZ_OK
}