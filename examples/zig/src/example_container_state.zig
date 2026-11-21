//! Container の状態の型 (issue #574)。Container (example_container_zig) と ContainerProcess
//! (example_container_zig_increment / _get) の両方がこのファイルを @import する。
//!
//! Container と ContainerProcess は別々のライブラリになるので、zigjuiz は状態の型の名前 (このファイルの中の位置を含む)・
//! 大きさで同じ型かを確かめる。両方のライブラリで、同じファイルの同じ型を使うこと。
const std = @import("std");

/// 作られて、まだ削除されていない Container の数 (テスト用。ライブラリごとに別の変数になる)
pub var alive_count = std.atomic.Value(i64).init(0);

/// 整数を一つ保持する
pub const Counter = struct {
    value: i64,

    /// Container を削除するときに zigjuiz が呼ぶ (状態を解放する前)
    pub fn deinit(self: *Counter) void {
        _ = self;
        _ = alive_count.fetchSub(1, .monotonic);
    }
};
