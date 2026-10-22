//! value_object_foreach がキー文字列をリークしないことを確かめるテスト (issue #130)
//!
//! 以前はキーごとに CString::into_raw したポインタを callback に渡し、誰も解放していなかった。
//! 確保と解放のバイト数を数えるアロケータを使うので、他のテストと混ざらないよう別のファイルにしている。

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicIsize, Ordering};

use juiz_sdk::prelude::*;
use juiz_sdk::value::converter_error::JUIZ_OK;
use juiz_sdk::value::value_converter::value_object_foreach;

struct CountingAllocator;

static NET_BYTES: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        NET_BYTES.fetch_add(layout.size() as isize, Ordering::SeqCst);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        NET_BYTES.fetch_sub(layout.size() as isize, Ordering::SeqCst);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// C から使う呼び出し側と同じく、キーを読むだけで解放しない
extern "C" fn sum_key_lengths(arg: *mut c_void, key: *const c_char, _v: *mut Value) {
    unsafe {
        *(arg as *mut usize) += CStr::from_ptr(key).to_bytes().len();
    }
}

#[test]
fn value_object_foreach_does_not_leak_keys() {
    let mut obj = jvalue!({"alpha": 1, "beta": 2, "gamma": 3});
    let mut total = 0usize;
    let before = NET_BYTES.load(Ordering::SeqCst);
    for _ in 0..100 {
        let ret = unsafe {
            value_object_foreach(
                &mut obj,
                sum_key_lengths,
                &mut total as *mut usize as *mut c_void,
            )
        };
        assert_eq!(ret, JUIZ_OK);
    }
    let after = NET_BYTES.load(Ordering::SeqCst);
    assert_eq!(total, 100 * ("alpha".len() + "beta".len() + "gamma".len()));
    assert_eq!(
        after - before,
        0,
        "value_object_foreach leaked {} bytes",
        after - before
    );
}
