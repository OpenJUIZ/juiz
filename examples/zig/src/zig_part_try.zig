//! 読み書きを試し、エラーの名前 (juiz.PoseError) か "ok" を返す (zig_part_try、issue #574)
const std = @import("std");
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn tryOp(self: *state.Part, args: struct { op: []const u8 = "pose", reference: []const u8 = "map" }) []const u8 {
    const r: juiz.PoseError!void = if (std.mem.eql(u8, args.op, "pose"))
        (if (juiz.pose(self, args.reference)) |_| {} else |e| e)
    else if (std.mem.eql(u8, args.op, "set_pose"))
        (if (juiz.setPose(self, args.reference, juiz.makePose(1, 0, 0, juiz.quaternion_identity))) |_| {} else |e| e)
    else if (std.mem.eql(u8, args.op, "set_pose_zero"))
        (if (juiz.setPose(self, args.reference, juiz.makePose(0, 0, 0, .{ .x = 0, .y = 0, .z = 0, .w = 0 }))) |_| {} else |e| e)
    else {};
    if (r) |_| return "ok" else |e| return @errorName(e);
}

comptime {
    juiz.exportContainerProcess(tryOp, .{
        .type_name = "zig_part_try",
        .container_type = "zig_part",
        .args = .{ .op = "pose / set_pose / set_pose_zero", .reference = "reference container" },
    });
}
