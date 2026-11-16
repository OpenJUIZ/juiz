//! value_object_foreach がキー文字列をリークしないことを確かめるテスト (issue #130)
//!
//! 以前はキーごとに CString::into_raw したポインタを callback に渡し、誰も解放していなかった。
//! 確保と解放のバイト数を数えるアロケータを使う。テストバイナリはクレートで 1 本 (issue #523) なので、
//! このアロケータはバイナリ全体に効く。ほかのテストと混ざらないよう、数えるのは [`count_net_bytes`] の
//! 中にいるスレッドの確保と解放だけにする。

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicIsize, Ordering};

use juiz_sdk::prelude::*;
use juiz_sdk::value::converter_error::JUIZ_OK;
use juiz_sdk::value::value_converter::value_object_foreach;

struct CountingAllocator;

static NET_BYTES: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    // アロケータの中から読むので、確保をしない const の初期化にする
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

fn add_net_bytes(n: isize) {
    if COUNTING.try_with(|c| c.get()).unwrap_or(false) {
        NET_BYTES.fetch_add(n, Ordering::SeqCst);
    }
}

/// `f` を実行する間に、このスレッドで確保して解放しなかったバイト数
fn count_net_bytes(f: impl FnOnce()) -> isize {
    let before = NET_BYTES.load(Ordering::SeqCst);
    COUNTING.with(|c| c.set(true));
    f();
    COUNTING.with(|c| c.set(false));
    NET_BYTES.load(Ordering::SeqCst) - before
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        add_net_bytes(layout.size() as isize);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        add_net_bytes(-(layout.size() as isize));
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
    let leaked = count_net_bytes(|| {
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
    });
    assert_eq!(total, 100 * ("alpha".len() + "beta".len() + "gamma".len()));
    assert_eq!(leaked, 0, "value_object_foreach leaked {leaked} bytes");
}
