//! 整数を一つ保持する Container (example_container_zig、issue #574)。
//! 初期値は設定ファイルの containers に書いた value (無ければ 0)。
//!
//! 返した Counter が Container の状態になる。Container を削除すると Counter.deinit が呼ばれる。
//! juiz のテスト (juiz_core/tests/integration/plugins/zig.rs) が生きている Container の数を数えられるよう、
//! example_container_zig_alive_count も公開している。
const juiz = @import("juiz");
const state = @import("example_container_state.zig");

fn create(args: struct { value: i64 = 0 }) state.Counter {
    _ = state.alive_count.fetchAdd(1, .monotonic);
    return .{ .value = args.value };
}

comptime {
    juiz.exportContainer(create, .{
        .type_name = "example_container_zig",
        .description = "Holds an integer value (Zig).",
        .args = .{ .value = "initial value" },
    });
}

/// 作られて、まだ削除されていない Container の数 (テスト用)
export fn example_container_zig_alive_count() i64 {
    return state.alive_count.load(.monotonic);
}
