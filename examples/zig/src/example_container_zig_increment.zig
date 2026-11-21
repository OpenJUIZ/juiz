//! example_container_zig の値に arg0 を足し、足した後の値を返す ContainerProcess (example_container_zig_increment、issue #574)。
//!
//! 第 1 引数が Container の状態 (*Counter)。i64 からあふれるときは error.Overflow を返す (juiz のエラーになり、値は変わらない)。
const std = @import("std");
const juiz = @import("juiz");
const state = @import("example_container_state.zig");

fn increment(self: *state.Counter, args: struct { arg0: i64 = 1 }) !i64 {
    self.value = try std.math.add(i64, self.value, args.arg0);
    return self.value;
}

comptime {
    juiz.exportContainerProcess(increment, .{
        .type_name = "example_container_zig_increment",
        .container_type = "example_container_zig",
        .description = "Adds arg0 to the value of the container (Zig).",
        .args = .{ .arg0 = "value to be added" },
    });
}
