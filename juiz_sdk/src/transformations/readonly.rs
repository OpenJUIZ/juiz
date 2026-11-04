//! readonly の ContainerProcess の呼び出し中であることを覚えておく (docs/spec/transformations.md §14、issue #377)
//!
//! ホストは readonly の ContainerProcess を呼ぶ間 [`ReadonlyScope`] を持つ。その間、同じスレッドから
//! その Container の ContainerPose に書き込む ([`ContainerPoseHandle::set_pose`]) と `PoseNotWritableError` になる。
//! スレッドごと・Container ごとに数えるので、同じ Container の readonly が別のスレッドで並行に走っていても、
//! API や別の Process からの書き込みは今まで通りできる。

use std::cell::RefCell;

use super::handle::ContainerPoseHandle;

thread_local! {
    /// このスレッドで呼び出し中の readonly ContainerProcess の Container (PoseState のアドレス)。入れ子があるので積む
    static READONLY_CALLS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Container を見分けるキー。ContainerImpl と ContainerPtr は同じ PoseState を指すので、そのアドレスを使う
pub(crate) fn key_of(handle: &ContainerPoseHandle) -> usize {
    std::sync::Arc::as_ptr(&handle.state()) as *const () as usize
}

/// このスレッドで、`handle` の Container の readonly ContainerProcess を呼び出し中か
pub fn in_readonly_call(handle: &ContainerPoseHandle) -> bool {
    let key = key_of(handle);
    READONLY_CALLS
        .try_with(|c| c.borrow().contains(&key))
        .unwrap_or(false)
}

/// readonly の ContainerProcess を呼ぶ間持つ。落とすと外れる
pub struct ReadonlyScope {
    key: usize,
}

impl ReadonlyScope {
    pub fn enter(handle: &ContainerPoseHandle) -> Self {
        let key = key_of(handle);
        let _ = READONLY_CALLS.try_with(|c| c.borrow_mut().push(key));
        ReadonlyScope { key }
    }
}

impl Drop for ReadonlyScope {
    fn drop(&mut self) {
        let key = self.key;
        let _ = READONLY_CALLS.try_with(|c| {
            let mut c = c.borrow_mut();
            if let Some(i) = c.iter().rposition(|k| *k == key) {
                c.remove(i);
            }
        });
    }
}
