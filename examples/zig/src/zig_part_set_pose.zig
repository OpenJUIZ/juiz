//! 自分を基準 reference から見て (x, y, z)・(qx, qy, qz, qw) にする (zig_part_set_pose、issue #574)。書いた後の値を返す
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn setPose(self: *state.Part, ctx: juiz.Context, args: struct {
    reference: []const u8 = "map",
    x: f64 = 0,
    y: f64 = 0,
    z: f64 = 0,
    qx: f64 = 0,
    qy: f64 = 0,
    qz: f64 = 0,
    qw: f64 = 1,
}) !juiz.Value {
    const q: juiz.Quaternion = .{ .x = args.qx, .y = args.qy, .z = args.qz, .w = args.qw };
    const p = try juiz.setPose(self, args.reference, juiz.makePose(args.x, args.y, args.z, q));
    return juiz.poseStampedToValue(ctx.allocator, p, args.reference);
}

comptime {
    juiz.exportContainerProcess(setPose, .{
        .type_name = "zig_part_set_pose",
        .container_type = "zig_part",
        .args = .{ .reference = "reference container", .x = "", .y = "", .z = "", .qx = "", .qy = "", .qz = "", .qw = "" },
    });
}
