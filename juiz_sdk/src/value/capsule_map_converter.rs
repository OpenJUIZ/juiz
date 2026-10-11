use std::ffi::CStr;

use super::converter_error::*;
use crate::prelude::*;

#[no_mangle]
pub unsafe extern "C" fn capsule_map_get_capsule(
    cmap: *mut CapsuleMap,
    name: *const std::os::raw::c_char,
    ptr: &mut *mut CapsulePtr,
) -> i64 {
    match cmap
        .as_mut()
        .unwrap()
        .get_mutref(CStr::from_ptr(name).to_str().unwrap())
    {
        Err(_) => {
            *ptr = std::ptr::null_mut();
            JUIZ_CAPSULEMAP_NO_VALUE
        }
        Ok(cp) => {
            *ptr = cp;
            JUIZ_OK
        }
    }
}
