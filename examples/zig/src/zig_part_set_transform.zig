//! Transform (source → destination) の値を平面の (x, y, yaw) で書く (zig_part_set_transform、issue #574)。
//! 書いた後の値 (juiz_std.geometry.TransformStamped の辞書) か、失敗ならエラーの名前を返す
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn setTransform(self: *state.Part, ctx: juiz.Context, args: struct {
    source: []const u8 = "map",
    destination: []const u8 = "",
    x: f64 = 0,
    y: f64 = 0,
    yaw: f64 = 0,
}) !juiz.Value {
    const t = juiz.setTransform(self, args.source, args.destination, juiz.makeTransform2d(args.x, args.y, args.yaw)) catch |e|
        return .{ .string = @errorName(e) };
    return juiz.transformStampedToValue(ctx.allocator, t, args.source, args.destination);
}

comptime {
    juiz.exportContainerProcess(setTransform, .{
        .type_name = "zig_part_set_transform",
        .container_type = "zig_part",
        .args = .{ .source = "source of the transform", .destination = "destination of the transform", .x = "", .y = "", .yaw = "rad" },
    });
}
