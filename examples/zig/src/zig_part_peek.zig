//! readonly の ContainerProcess (zig_part_peek、issue #574)。状態を *const で受けると readonly になり、
//! juiz は Container を読み出しロックで呼ぶので、同じ Container の readonly 同士は並行に走る。
//! sleep_ms だけ待ってから、基準 reference から見た自分の ContainerPose を返す。
//! (ここで juiz.setPose を呼ぶとコンパイルエラーになる)
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn peek(self: *const state.Part, ctx: juiz.Context, args: struct { reference: []const u8 = "map", sleep_ms: u32 = 0 }) !juiz.Value {
    state.sleepMs(args.sleep_ms);
    const p = try juiz.pose(self, args.reference);
    return juiz.poseStampedToValue(ctx.allocator, p, args.reference);
}

comptime {
    juiz.exportContainerProcess(peek, .{
        .type_name = "zig_part_peek",
        .container_type = "zig_part",
        .args = .{ .reference = "reference container", .sleep_ms = "milliseconds to wait" },
    });
}
