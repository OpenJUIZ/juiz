//! 自分以外の Container target の、基準 reference から見た ContainerPose を返す (zig_part_lookup、issue #574)。
//! 読み出しだけなので readonly (*const)。失敗ならエラーの名前を返す
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn lookup(self: *const state.Part, ctx: juiz.Context, args: struct { target: []const u8 = "", reference: []const u8 = "map" }) !juiz.Value {
    const p = juiz.lookup(self, args.target, args.reference) catch |e| return .{ .string = @errorName(e) };
    return juiz.poseStampedToValue(ctx.allocator, p, args.reference);
}

comptime {
    juiz.exportContainerProcess(lookup, .{
        .type_name = "zig_part_lookup",
        .container_type = "zig_part",
        .args = .{ .target = "container to look up", .reference = "reference container" },
    });
}
