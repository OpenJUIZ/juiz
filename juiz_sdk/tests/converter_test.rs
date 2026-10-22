//! C/C++ バインディング向けの extern "C" 変換関数のテスト

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex};

use juiz_sdk::prelude::*;
use juiz_sdk::value::capsule_converter::*;
use juiz_sdk::value::capsule_map_converter::capsule_map_get_capsule;
use juiz_sdk::value::capsule_ptr_converter::*;
use juiz_sdk::value::converter_error::*;
use juiz_sdk::value::value_converter::*;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

unsafe fn read_c_str(p: *const u8, len: usize) -> String {
    String::from_utf8(std::slice::from_raw_parts(p, len).to_vec()).unwrap()
}

#[test]
fn value_type_predicates_and_getters() {
    unsafe {
        let mut i = jvalue!(-5);
        let mut out_i = 0i64;
        assert!(value_is_int(&mut i));
        assert_eq!(value_get_int(&mut i, &mut out_i), JUIZ_OK);
        assert_eq!(out_i, -5);

        let mut u = jvalue!(5u64);
        let mut out_u = 0u64;
        assert!(value_is_uint(&mut u));
        assert_eq!(value_get_uint(&mut u, &mut out_u), JUIZ_OK);
        assert_eq!(out_u, 5);
        assert_eq!(value_get_uint(&mut i, &mut out_u), JUIZ_VALUE_TYPE_ERROR);

        let mut f = jvalue!(0.5);
        let mut out_f = 0f64;
        assert!(value_is_float(&mut f));
        assert_eq!(value_get_float(&mut f, &mut out_f), JUIZ_OK);
        assert_eq!(out_f, 0.5);

        let mut b = jvalue!(true);
        let mut out_b = false;
        assert!(value_is_bool(&mut b));
        assert_eq!(value_get_bool(&mut b, &mut out_b), JUIZ_OK);
        assert!(out_b);

        let mut s = jvalue!("abc");
        let mut out_s: *const u8 = std::ptr::null();
        assert!(value_is_string(&mut s));
        assert_eq!(value_get_string(&mut s, &mut out_s), 0);
        assert_eq!(read_c_str(out_s, 3), "abc");
        let mut out_len: usize = 0;
        assert_eq!(
            value_get_string_with_len(&mut s, &mut out_s, &mut out_len),
            JUIZ_OK
        );
        assert_eq!(read_c_str(out_s, out_len), "abc");

        // 型が違う場合はエラーコードを返す
        assert_eq!(value_get_int(&mut s, &mut out_i), JUIZ_VALUE_TYPE_ERROR);
        assert_eq!(value_get_float(&mut s, &mut out_f), JUIZ_VALUE_TYPE_ERROR);
        assert_eq!(value_get_bool(&mut s, &mut out_b), JUIZ_VALUE_TYPE_ERROR);
        assert_eq!(value_get_string(&mut i, &mut out_s), -1);

        let mut n = Value::Null;
        assert!(value_is_null(&mut n));
        assert!(!value_is_object(&mut n));
        assert!(!value_is_array(&mut n));
    }
}

#[test]
fn value_object_setters_build_nested_values() {
    unsafe {
        let mut obj = jvalue!({});
        assert!(value_is_object(&mut obj));
        value_object_set_int(&mut obj, cstr("i").as_ptr(), -1);
        value_object_set_uint(&mut obj, cstr("u").as_ptr(), 1);
        value_object_set_float(&mut obj, cstr("f").as_ptr(), 1.5);
        value_object_set_string(&mut obj, cstr("s").as_ptr(), cstr("str").as_ptr());
        value_object_set_bool(&mut obj, cstr("b").as_ptr(), 1);
        let arr = value_object_set_empty_array(&mut obj, cstr("arr").as_ptr());
        value_array_push_int(arr, -2);
        value_array_push_uint(arr, 2);
        value_array_push_float(arr, 2.5);
        value_array_push_bool(arr, 0);
        value_array_push_string(arr, cstr("x").as_ptr());
        let inner_obj = value_array_push_empty_object(arr);
        value_object_set_int(inner_obj, cstr("k").as_ptr(), 9);
        let inner_arr = value_array_push_empty_array(arr);
        value_array_push_int(inner_arr, 10);
        let nested = value_object_set_empty_object(&mut obj, cstr("nested").as_ptr());
        value_object_set_bool(nested, cstr("flag").as_ptr(), 0);

        assert_eq!(
            obj,
            jvalue!({
                "i": -1, "u": 1, "f": 1.5, "s": "str", "b": true,
                "arr": [-2, 2, 2.5, false, "x", {"k": 9}, [10]],
                "nested": {"flag": false}
            })
        );

        let got = value_object_get_value(&mut obj, cstr("s").as_ptr());
        assert_eq!(*got, jvalue!("str"));
        assert!(value_object_get_value(&mut obj, cstr("missing").as_ptr()).is_null());
    }
}

extern "C" fn sum_ints(arg: *mut c_void, v: *mut Value) {
    unsafe {
        let total = &mut *(arg as *mut i64);
        *total += (*v).as_i64().unwrap();
    }
}

extern "C" fn collect_keys(arg: *mut c_void, key: *const c_char, _v: *mut Value) {
    unsafe {
        let keys = &mut *(arg as *mut Vec<String>);
        // キー文字列は callback の間だけ有効で、解放は value_object_foreach が行う (issue #130)
        let k = CStr::from_ptr(key);
        keys.push(k.to_str().unwrap().to_owned());
    }
}

#[test]
fn value_foreach_callbacks() {
    unsafe {
        let mut arr = jvalue!([1, 2, 3]);
        let mut total = 0i64;
        assert_eq!(
            value_array_foreach(&mut arr, sum_ints, &mut total as *mut i64 as *mut c_void),
            JUIZ_OK
        );
        assert_eq!(total, 6);

        let mut obj = jvalue!({"a": 1, "b": 2});
        let mut keys: Vec<String> = Vec::new();
        assert_eq!(
            value_object_foreach(
                &mut obj,
                collect_keys,
                &mut keys as *mut Vec<String> as *mut c_void,
            ),
            JUIZ_OK
        );
        keys.sort();
        assert_eq!(keys, vec!["a".to_owned(), "b".to_owned()]);
    }
}

#[test]
fn capsule_converters_get_and_set() {
    unsafe {
        let mut c = Capsule::from(jvalue!(1));
        let mut i = 0i64;
        assert!(capsule_is_value(&mut c));
        assert!(capsule_is_int(&mut c));
        assert_eq!(capsule_get_int(&mut c, &mut i), JUIZ_OK);
        assert_eq!(i, 1);
        assert_eq!(capsule_set_int(&mut c, -7), JUIZ_OK);
        assert_eq!(capsule_get_int(&mut c, &mut i), JUIZ_OK);
        assert_eq!(i, -7);

        let mut u = 0u64;
        assert_eq!(capsule_set_uint(&mut c, 8), JUIZ_OK);
        assert!(capsule_is_uint(&mut c));
        assert_eq!(capsule_get_uint(&mut c, &mut u), JUIZ_OK);
        assert_eq!(u, 8);

        let mut f = 0f64;
        assert_eq!(capsule_set_float(&mut c, 0.5), JUIZ_OK);
        assert!(capsule_is_float(&mut c));
        assert_eq!(capsule_get_float(&mut c, &mut f), JUIZ_OK);
        assert_eq!(f, 0.5);
        assert_eq!(capsule_get_int(&mut c, &mut i), JUIZ_CAPSULE_TYPE_ERROR);
        assert_eq!(capsule_get_uint(&mut c, &mut u), JUIZ_CAPSULE_TYPE_ERROR);

        let mut b = 0i64;
        assert_eq!(capsule_set_bool(&mut c, 1), JUIZ_OK);
        assert!(capsule_is_bool(&mut c));
        assert_eq!(capsule_get_bool(&mut c, &mut b), JUIZ_OK);
        assert_eq!(b, 1);
        assert_eq!(capsule_set_bool(&mut c, 0), JUIZ_OK);
        assert_eq!(capsule_get_bool(&mut c, &mut b), JUIZ_OK);
        assert_eq!(b, 0);
        assert_eq!(capsule_get_float(&mut c, &mut f), JUIZ_CAPSULE_TYPE_ERROR);

        let mut s: *const u8 = std::ptr::null();
        assert_eq!(capsule_set_string(&mut c, cstr("hey").as_ptr()), JUIZ_OK);
        assert!(capsule_is_string(&mut c));
        assert_eq!(capsule_get_string(&mut c, &mut s), JUIZ_OK);
        assert_eq!(read_c_str(s, 3), "hey");
        let mut len: usize = 0;
        assert_eq!(
            capsule_get_string_with_len(&mut c, &mut s, &mut len),
            JUIZ_OK
        );
        assert_eq!(read_c_str(s, len), "hey");
        assert_eq!(capsule_get_bool(&mut c, &mut b), JUIZ_CAPSULE_TYPE_ERROR);

        assert_eq!(capsule_set_empty_object(&mut c), JUIZ_OK);
        assert_eq!(c.as_value(), Some(&jvalue!({})));
        assert_eq!(capsule_get_string(&mut c, &mut s), JUIZ_CAPSULE_TYPE_ERROR);
        assert_eq!(capsule_set_empty_array(&mut c), JUIZ_OK);
        assert_eq!(c.as_value(), Some(&jvalue!([])));

        // 値を持たないカプセル
        let mut e = Capsule::empty();
        assert!(!capsule_is_value(&mut e));
        assert!(!capsule_is_int(&mut e));
        assert!(!capsule_is_uint(&mut e));
        assert!(!capsule_is_float(&mut e));
        assert!(!capsule_is_bool(&mut e));
        assert!(!capsule_is_string(&mut e));
        assert_eq!(capsule_get_int(&mut e, &mut i), JUIZ_CAPSULE_NO_VALUE);
        assert_eq!(capsule_get_uint(&mut e, &mut u), JUIZ_CAPSULE_NO_VALUE);
        assert_eq!(capsule_get_float(&mut e, &mut f), JUIZ_CAPSULE_NO_VALUE);
        assert_eq!(capsule_get_bool(&mut e, &mut b), JUIZ_CAPSULE_NO_VALUE);
        assert_eq!(capsule_get_string(&mut e, &mut s), JUIZ_CAPSULE_NO_VALUE);
    }
}

#[test]
fn arc_capsule_type_predicates() {
    unsafe {
        let check = |v: Value| -> [bool; 8] {
            let mut a = Arc::new(Mutex::new(Capsule::from(v)));
            [
                arc_capsule_is_int(&mut a),
                arc_capsule_is_uint(&mut a),
                arc_capsule_is_float(&mut a),
                arc_capsule_is_bool(&mut a),
                arc_capsule_is_string(&mut a),
                arc_capsule_is_array(&mut a),
                arc_capsule_is_object(&mut a),
                arc_capsule_is_null(&mut a),
            ]
        };
        assert_eq!(
            check(jvalue!(-1)),
            [true, false, false, false, false, false, false, false]
        );
        assert_eq!(
            check(jvalue!(1)),
            [true, true, false, false, false, false, false, false]
        );
        assert_eq!(
            check(jvalue!(1.5)),
            [false, false, true, false, false, false, false, false]
        );
        assert_eq!(
            check(jvalue!(true)),
            [false, false, false, true, false, false, false, false]
        );
        assert_eq!(
            check(jvalue!("s")),
            [false, false, false, false, true, false, false, false]
        );
        assert_eq!(
            check(jvalue!([])),
            [false, false, false, false, false, true, false, false]
        );
        assert_eq!(
            check(jvalue!({})),
            [false, false, false, false, false, false, true, false]
        );
        assert_eq!(
            check(Value::Null),
            [false, false, false, false, false, false, false, true]
        );

        let mut empty = Arc::new(Mutex::new(Capsule::empty()));
        assert!(!arc_capsule_is_value(&mut empty));
        assert!(!arc_capsule_is_int(&mut empty));
        assert!(!arc_capsule_is_uint(&mut empty));
        assert!(!arc_capsule_is_float(&mut empty));
        assert!(!arc_capsule_is_bool(&mut empty));
        assert!(!arc_capsule_is_string(&mut empty));
        assert!(!arc_capsule_is_array(&mut empty));
        assert!(!arc_capsule_is_object(&mut empty));
        assert!(!arc_capsule_is_null(&mut empty));
    }
}

#[test]
fn capsule_ptr_converters_get_and_set() {
    unsafe {
        let mut p = CapsulePtr::new();
        let mut i = 0i64;
        let mut u = 0u64;
        let mut f = 0f64;
        let mut b = 0i64;
        let mut s: *const u8 = std::ptr::null();

        // 値が入っていないときはロックエラー扱い
        assert_eq!(
            capsule_ptr_get_int(&mut p, &mut i),
            JUIZ_CAPSULEPTR_LOCK_ERROR
        );

        assert_eq!(capsule_ptr_set_int(&mut p, -3), JUIZ_OK);
        assert_eq!(capsule_ptr_get_int(&mut p, &mut i), JUIZ_OK);
        assert_eq!(i, -3);
        assert_eq!(capsule_ptr_get_uint(&mut p, &mut u), JUIZ_VALUE_TYPE_ERROR);

        assert_eq!(capsule_ptr_set_uint(&mut p, 3), JUIZ_OK);
        assert_eq!(capsule_ptr_get_uint(&mut p, &mut u), JUIZ_OK);
        assert_eq!(u, 3);

        assert_eq!(capsule_ptr_set_float(&mut p, 2.5), JUIZ_OK);
        assert_eq!(capsule_ptr_get_float(&mut p, &mut f), JUIZ_OK);
        assert_eq!(f, 2.5);
        assert_eq!(capsule_ptr_get_int(&mut p, &mut i), JUIZ_VALUE_TYPE_ERROR);
        assert_eq!(capsule_ptr_get_bool(&mut p, &mut b), JUIZ_VALUE_TYPE_ERROR);

        assert_eq!(capsule_ptr_set_bool(&mut p, 1), JUIZ_OK);
        assert_eq!(capsule_ptr_get_bool(&mut p, &mut b), JUIZ_OK);
        assert_eq!(b, 1);
        assert_eq!(capsule_ptr_set_bool(&mut p, 0), JUIZ_OK);
        assert_eq!(capsule_ptr_get_bool(&mut p, &mut b), JUIZ_OK);
        assert_eq!(b, 0);
        assert_eq!(capsule_ptr_get_float(&mut p, &mut f), JUIZ_VALUE_TYPE_ERROR);
        assert_eq!(
            capsule_ptr_get_string(&mut p, &mut s),
            JUIZ_VALUE_TYPE_ERROR
        );

        assert_eq!(capsule_ptr_set_string(&mut p, cstr("ok").as_ptr()), JUIZ_OK);
        assert_eq!(capsule_ptr_get_string(&mut p, &mut s), JUIZ_OK);
        assert_eq!(read_c_str(s, 2), "ok");
        let mut len: usize = 0;
        assert_eq!(
            capsule_ptr_get_string_with_len(&mut p, &mut s, &mut len),
            JUIZ_OK
        );
        assert_eq!(read_c_str(s, len), "ok");

        assert_eq!(capsule_ptr_set_empty_object(&mut p), JUIZ_OK);
        assert!(p.lock_as_map(|m| m.is_empty()).unwrap());
        assert_eq!(capsule_ptr_set_empty_array(&mut p), JUIZ_OK);
        assert_eq!(p.lock_as_value(|v| v.clone()).unwrap(), jvalue!([]));

        let inner = capsule_ptr_get_value(&mut p);
        assert!(arc_capsule_is_array(inner));
    }
}

extern "C" fn set_to_100(v: *mut Value) {
    unsafe {
        *v = jvalue!(100);
    }
}

extern "C" fn add_arg(arg: *mut c_void, v: *mut Value) -> i64 {
    unsafe {
        let add = *(arg as *mut i64);
        *v = jvalue!((*v).as_i64().unwrap() + add);
        JUIZ_OK
    }
}

#[test]
fn capsule_ptr_lock_callbacks() {
    unsafe {
        let mut p: CapsulePtr = jvalue!(1).into();
        assert_eq!(capsule_ptr_lock_as_value(&mut p, set_to_100), JUIZ_OK);
        assert_eq!(p.lock_as_value(|v| v.clone()).unwrap(), jvalue!(100));
        let mut add = 5i64;
        assert_eq!(
            capsule_ptr_lock_as_value_with_arg(
                &mut p,
                add_arg,
                &mut add as *mut i64 as *mut c_void
            ),
            JUIZ_OK
        );
        assert_eq!(p.lock_as_value(|v| v.clone()).unwrap(), jvalue!(105));
    }
}

#[test]
fn capsule_map_get_capsule_by_name() {
    unsafe {
        let mut m: CapsuleMap = vec![("a", jvalue!(1))].into();
        let mut ptr: *mut CapsulePtr = std::ptr::null_mut();
        assert_eq!(
            capsule_map_get_capsule(&mut m, cstr("a").as_ptr(), &mut ptr),
            JUIZ_OK
        );
        assert!(!ptr.is_null());
        let mut i = 0i64;
        assert_eq!(capsule_ptr_get_int(ptr, &mut i), JUIZ_OK);
        assert_eq!(i, 1);

        assert_eq!(
            capsule_map_get_capsule(&mut m, cstr("missing").as_ptr(), &mut ptr),
            JUIZ_CAPSULEMAP_NO_VALUE
        );
        assert!(ptr.is_null());
    }
}

/// issue #56: 不正な UTF-8 や NULL を渡しても panic せずエラーコードを返す
#[test]
fn set_string_rejects_invalid_utf8_without_panic() {
    unsafe {
        let invalid = CString::new(vec![b'H', b'i', 0xff, 0xfe]).unwrap();

        let mut c = Capsule::from(jvalue!("before"));
        assert_eq!(
            capsule_set_string(&mut c, invalid.as_ptr()),
            JUIZ_VALUE_CONVERTER_ERROR
        );
        assert_eq!(
            capsule_set_string(&mut c, std::ptr::null()),
            JUIZ_VALUE_CONVERTER_ERROR
        );
        assert_eq!(c.as_value(), Some(&jvalue!("before")));

        let mut p = CapsulePtr::from(jvalue!("before"));
        assert_eq!(
            capsule_ptr_set_string(&mut p, invalid.as_ptr()),
            JUIZ_VALUE_CONVERTER_ERROR
        );
        assert_eq!(p.lock_as_value(|v| v.clone()).unwrap(), jvalue!("before"));
    }
}

/// issue #56: 取り出した文字列は長さ付きで読めば後ろのメモリを読まない
#[test]
fn get_string_with_len_returns_exact_length() {
    unsafe {
        let mut c = Capsule::from(jvalue!("Hi"));
        let mut s: *const u8 = std::ptr::null();
        let mut len: usize = usize::MAX;
        assert_eq!(
            capsule_get_string_with_len(&mut c, &mut s, &mut len),
            JUIZ_OK
        );
        assert_eq!(len, 2);
        assert_eq!(read_c_str(s, len), "Hi");

        let mut e = Capsule::from(jvalue!(""));
        assert_eq!(
            capsule_get_string_with_len(&mut e, &mut s, &mut len),
            JUIZ_OK
        );
        assert_eq!(len, 0);
    }
}
