//! 基準 reference から見た自分の ContainerPose (juiz_std.geometry.PoseStamped の辞書) を返す (zig_part_pose、issue #574)。
//! ContainerPose のエラーはそのまま返す (zigjuiz が -1101 と、juiz が返した理由のエラー文にする)
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn pose(self: *state.Part, ctx: juiz.Context, args: struct { reference: []const u8 = "map" }) !juiz.Value {
    const p = try juiz.pose(self, args.reference);
    return juiz.poseStampedToValue(ctx.allocator, p, args.reference);
}

comptime {
    juiz.exportContainerProcess(pose, .{
        .type_name = "zig_part_pose",
        .container_type = "zig_part",
        .args = .{ .reference = "reference container" },
    });
}
