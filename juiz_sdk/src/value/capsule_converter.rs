use std::sync::{Arc, Mutex};

use super::converter_error::*;
use crate::prelude::*;
use super::capsule_ptr::lock_encoded;

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_value(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    let c = capsule.as_ref().unwrap();
    let is_value = c.lock().and_then(|cap| Ok(cap.is_value()));
    is_value.unwrap()
}

/// 中身の Value を check に渡す。Native ならロックの外で encode してから読む (lazy-evaluation.md §6)
unsafe fn arc_capsule_check_value<F>(capsule: *mut Arc<Mutex<Capsule>>, check: F) -> bool
where
    F: FnOnce(&Value) -> bool,
{
    let c = capsule.as_ref().unwrap();
    match lock_encoded(c, "arc_capsule_check_value") {
        Ok(cap) => cap.as_value().map_or(false, check),
        Err(_) => false,
    }
}

/// 中身の Value。Native なら encode する (Capsule を直接持つのでロックは無い)
unsafe fn capsule_value<'a>(capsule: *mut Capsule) -> Option<&'a Value> {
    capsule.as_ref().unwrap().as_value()
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_int(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_i64())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_float(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_f64())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_bool(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_boolean())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_array(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_array())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_object(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_object())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_string(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_string())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_null(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_null())
}

#[no_mangle]
pub unsafe extern "C" fn arc_capsule_is_uint(capsule: *mut Arc<Mutex<Capsule>>) -> bool {
    arc_capsule_check_value(capsule, |v| v.is_u64())
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_value(capsule: *mut Capsule) -> bool {
    let c = capsule.as_ref().unwrap();
    c.is_value()
}

#[no_mangle]
pub unsafe extern "C" fn capsule_is_int(capsule: *mut Capsule) -> bool {
    capsule_value(capsule).map_or(false, |v| v.is_i64())
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_int(capsule: *mut Capsule, v: *mut i64) -> i64 {
    if let Some(val) = capsule_value(capsule) {
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
    capsule_value(capsule).map_or(false, |v| v.is_u64())
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_uint(capsule: *mut Capsule, v: *mut u64) -> i64 {
    if let Some(val) = capsule_value(capsule) {
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
    capsule_value(capsule).map_or(false, |v| v.is_f64())
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_float(capsule: *mut Capsule, v: *mut f64) -> i64 {
    if let Some(val) = capsule_value(capsule) {
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
    capsule_value(capsule).map_or(false, |v| v.is_boolean())
}

#[no_mangle]
pub unsafe extern "C" fn capsule_get_bool(capsule: *mut Capsule, v: *mut i64) -> i64 {
    if let Some(val) = capsule_value(capsule) {
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
    capsule_value(capsule).map_or(false, |v| v.is_string())
}

/// 文字列の先頭ポインタを返す。返す文字列は NUL 終端されていないので、
/// 長さも受け取れる capsule_get_string_with_len を使うこと。
/// 古いプラグインとのバイナリ互換のためだけに残している。
#[no_mangle]
pub unsafe extern "C" fn capsule_get_string(capsule: *mut Capsule, v: *mut *const u8) -> i64 {
    let mut len: usize = 0;
    capsule_get_string_with_len(capsule, v, &mut len)
}

/// 文字列の先頭ポインタ (NUL 終端ではない) とバイト長を返す。
#[no_mangle]
pub unsafe extern "C" fn capsule_get_string_with_len(
    capsule: *mut Capsule,
    v: *mut *const u8,
    len: *mut usize,
) -> i64 {
    if let Some(val) = capsule_value(capsule) {
        match val.as_str() {
            Some(s) => {
                *v = s.as_ptr();
                *len = s.len();
                JUIZ_OK
            }
            None => JUIZ_CAPSULE_TYPE_ERROR,
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
    let Some(s) = c_str_to_str(v) else {
        return JUIZ_VALUE_CONVERTER_ERROR;
    };
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(jvalue!(s).into());
    JUIZ_OK
}

#[no_mangle]
pub unsafe extern "C" fn capsule_set_null(capsule: *mut Capsule) -> i64 {
    let cap = capsule.as_mut().unwrap();
    cap.replace_value(Value::Null.into());
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

/// capsule の中身を value として callback に渡し、callback の戻り値を返す。
/// capsule_set_empty_object / capsule_set_empty_array の後に呼び、オブジェクトや配列を書き込むのに使う。
/// capsule が値を持っていなければ JUIZ_CAPSULE_NO_VALUE を返す。
#[no_mangle]
pub unsafe extern "C" fn capsule_lock_as_value_with_arg(
    capsule: *mut Capsule,
    callback: extern "C" fn(*mut std::ffi::c_void, *mut Value) -> i64,
    arg: *mut std::ffi::c_void,
) -> i64 {
    let cap = capsule.as_mut().unwrap();
    match cap.as_value_mut() {
        Some(v) => callback(arg, v),
        None => JUIZ_CAPSULE_NO_VALUE,
    }
}
