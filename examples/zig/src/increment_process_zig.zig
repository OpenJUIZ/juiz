//! arg1 に 1 を足す Process (issue #573)。
//!
//! 引数は 1 つの struct で受ける。フィールド名・型・既定値がそのままマニフェストの引数になる。
const std = @import("std");
const juiz = @import("juiz");

fn increment(args: struct { arg1: i64 = 1 }) !i64 {
    // i64 の最大値に 1 を足すと error.Overflow。zigjuiz が juiz のエラーにする (juiz は落ちない)
    return std.math.add(i64, args.arg1, 1);
}

comptime {
    juiz.exportProcess(increment, .{
        .type_name = "increment_process_zig",
        .description = "Adds 1 to arg1 (Zig).",
        .args = .{ .arg1 = "value to be incremented" },
    });
}
