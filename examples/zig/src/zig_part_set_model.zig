//! 3D モデルを text (空なら外す) に差し替え、差し替えた後のモデル情報を返す (zig_part_set_model、issue #564)。
//! 失敗ならエラーの名前を返す
const std = @import("std");
const juiz = @import("juiz");
const state = @import("zig_part_state.zig");

fn setModel(self: *state.Part, ctx: juiz.Context, args: struct {
    text: []const u8 = "",
    format: []const u8 = "sdf",
}) !juiz.Value {
    if (args.text.len == 0) {
        juiz.clearModel(self) catch |e| return .{ .string = @errorName(e) };
        return .{ .object = try ctx.allocator.dupe(juiz.Value.Member, &.{.{ .key = "cleared", .value = .{ .bool = true } }}) };
    }
    const options: juiz.Value = .{ .object = try ctx.allocator.dupe(juiz.Value.Member, &.{
        .{ .key = "color", .value = .{ .string = "#ffff00" } },
        .{ .key = "file_name", .value = .{ .string = try std.fmt.allocPrint(ctx.allocator, "zig.{s}", .{args.format}) } },
    }) };
    return juiz.setModel(self, ctx.allocator, args.text, args.format, options) catch |e| .{ .string = @errorName(e) };
}

comptime {
    juiz.exportContainerProcess(setModel, .{
        .type_name = "zig_part_set_model",
        .container_type = "zig_part",
        .args = .{ .text = "model contents (empty to clear)", .format = "model format" },
    });
}
