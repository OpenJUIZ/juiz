//! ContainerPose を読み書きする例 (zig_part、issue #574) の Container の状態の型。
//! C++ の examples/cpp/component/cpp_pose_component、Haskell の PoseComponent と同じ構成。
//! juiz のテスト juiz_core/tests/integration/transformations/zig.rs が使う。
//!
//! 位置姿勢は juiz が Container ごとに持つ (利用者の状態とは別) ので、ここには数えるための値だけを置く。
const std = @import("std");
const builtin = @import("builtin");

pub const Part = struct {
    count: i64 = 0,
};

/// ms ミリ秒待つ (readonly の並行のテスト用)
pub fn sleepMs(ms: u32) void {
    if (builtin.os.tag == .windows) {
        Sleep(ms);
    } else {
        var ts: std.c.timespec = .{ .sec = @intCast(ms / 1000), .nsec = @intCast((ms % 1000) * std.time.ns_per_ms) };
        while (std.c.nanosleep(&ts, &ts) != 0) {}
    }
}

extern "kernel32" fn Sleep(ms: u32) callconv(.winapi) void;
