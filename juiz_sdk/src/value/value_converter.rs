use std::ffi::CStr;
use std::ffi::CString;

use super::value::*;
use super::converter_error::*;

#[no_mangle]
pub unsafe extern "C" fn value_is_int(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_i64()
}

#[no_mangle]
pub unsafe extern "C" fn value_get_int(value: *mut Value, output: *mut i64) -> i64 {
    match value.as_ref().unwrap().as_i64() {
        Some(v) => {
            *output = v;
            JUIZ_OK
        },
        None => {
            JUIZ_VALUE_TYPE_ERROR
        }
    }
}


#[no_mangle]
pub unsafe extern "C" fn value_is_uint(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_u64()
}

#[no_mangle]
pub unsafe extern "C" fn value_get_uint(value: *mut Value, output: *mut u64) -> i64 {
    match value.as_ref().unwrap().as_u64() {
        Some(v) => {
            *output = v;
            JUIZ_OK
        },
        None => {
            JUIZ_VALUE_TYPE_ERROR
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn value_is_float(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_f64()
}

#[no_mangle]
pub unsafe extern "C" fn value_get_float(value: *mut Value, output: *mut f64) -> i64 {
    match value.as_ref().unwrap().as_f64() {
        Some(v) => {
            *output = v;
            JUIZ_OK
        },
        None => {
            JUIZ_VALUE_TYPE_ERROR
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn value_is_bool(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_boolean()
}

#[no_mangle]
pub unsafe extern "C" fn value_get_bool(value: *mut Value, output: *mut bool) -> i64 {
    match value.as_ref().unwrap().as_bool() {
        Some(v) => {
            *output = v;
            JUIZ_OK
        },
        None => {
            JUIZ_VALUE_TYPE_ERROR
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn value_is_string(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_string()
}

#[no_mangle]
pub unsafe extern "C" fn value_get_string(value: *mut Value, output: *mut *const u8) -> i64 {
    value.as_ref().unwrap().as_str().and_then(|v| { *output = v.as_ptr(); Some(0)}).or(Some(-1)).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_is_object(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_object()
}

#[no_mangle]
pub unsafe extern "C" fn value_object_get_value(value: *mut Value, key: *const std::os::raw::c_char) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    match obj.get_mut(CStr::from_ptr(key).to_str().unwrap()) {
        Some(v) => { 
            return v;
        },
        None => {
            return std::ptr::null_mut();
        }
    }
}


#[no_mangle]
pub unsafe extern "C" fn value_object_set_int(value: *mut Value, key: *const std::os::raw::c_char, v: i64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!(v));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_object_set_uint(value: *mut Value, key: *const std::os::raw::c_char, v: u64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!(v));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_object_set_float(value: *mut Value, key: *const std::os::raw::c_char, v: f64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!(v));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_object_set_string(value: *mut Value, key: *const std::os::raw::c_char, v: *const std::os::raw::c_char) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!(CStr::from_ptr(v).to_str().unwrap()));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_object_set_bool(value: *mut Value, key: *const std::os::raw::c_char, v: i64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!(v != 0));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_is_array(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_array()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_foreach(value: *mut Value, callback: extern "C" fn(*mut std::ffi::c_void, *mut Value) -> (), arg: *mut std::ffi::c_void) -> () {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    for v in obj.iter_mut() {
        callback(arg, v);
    }
}


#[no_mangle]
pub unsafe extern "C" fn value_object_foreach(value: *mut Value, callback: extern "C" fn(*mut std::ffi::c_void, *mut std::os::raw::c_char, *mut Value) -> (), arg: *mut std::ffi::c_void) -> () {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    for (k, v) in obj.iter_mut() {
        let key = CString::new(k.as_str()).unwrap();
        callback(arg, key.into_raw(), v);
    }
}

#[no_mangle]
pub unsafe extern "C" fn value_object_set_empty_array(value: *mut Value, key: *const std::os::raw::c_char) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!([]));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_object_set_empty_object(value: *mut Value, key: *const std::os::raw::c_char) -> *mut Value {
    let obj = value.as_mut().unwrap().as_object_mut().unwrap();
    let keystr = CStr::from_ptr(key).to_str().unwrap().to_owned();
    obj.insert(keystr.clone(), jvalue!({}));
    obj.get_mut(&keystr).unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_push_int(value: *mut Value, v: i64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!(v));
    obj.last_mut().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_push_uint(value: *mut Value, v: u64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!(v));
    obj.last_mut().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_push_float(value: *mut Value, v: f64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!(v));
    obj.last_mut().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_push_bool(value: *mut Value, v: i64) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!(v != 0));
    obj.last_mut().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_push_string(value: *mut Value, v: *const std::os::raw::c_char) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!(CStr::from_ptr(v).to_str().unwrap()));
    obj.last_mut().unwrap()
}


#[no_mangle]
pub unsafe extern "C" fn value_array_push_empty_object(value: *mut Value) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!({}));
    obj.last_mut().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_array_push_empty_array(value: *mut Value) -> *mut Value {
    let obj = value.as_mut().unwrap().as_array_mut().unwrap();
    obj.push(jvalue!([]));
    obj.last_mut().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn value_is_null(value: *mut Value) -> bool {
    value.as_ref().unwrap().is_null()
}
