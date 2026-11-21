//! zigjuiz で使える引数の型を全部受け取り、辞書 (juiz.Value) にして返す Process (issue #573)。
//! juiz のテスト (juiz_core/tests/integration/plugins/zig.rs) が値の読み書きを確かめるのに使う。
const juiz = @import("juiz");

fn echoTypes(ctx: juiz.Context, args: struct {
    i: i64 = -1,
    u: u64 = 2,
    small: u8 = 3,
    f: f64 = 0.5,
    g: f32 = 1.5,
    b: bool = true,
    s: []const u8 = "zig",
    v: juiz.Value = .{ .object = &.{.{ .key = "k", .value = .{ .int = 1 } }} },
}) !juiz.Value {
    const members = try ctx.allocator.dupe(juiz.Value.Member, &.{
        .{ .key = "i", .value = .{ .int = args.i } },
        .{ .key = "u", .value = .{ .uint = args.u } },
        .{ .key = "small", .value = .{ .int = args.small } },
        .{ .key = "f", .value = .{ .float = args.f } },
        .{ .key = "g", .value = .{ .float = args.g } },
        .{ .key = "b", .value = .{ .bool = args.b } },
        .{ .key = "s", .value = .{ .string = args.s } },
        .{ .key = "v", .value = args.v },
        // 辞書の中の値も読める (v.k が無ければ null)
        .{ .key = "v_k", .value = args.v.get("k") orelse .null },
    });
    return .{ .object = members };
}

comptime {
    juiz.exportProcess(echoTypes, .{
        .type_name = "echo_types_process_zig",
        .description = "Returns all arguments as an object (Zig).",
        .args = .{
            .i = "signed integer",
            .u = "unsigned integer",
            .small = "integer from 0 to 255",
            .f = "float",
            .g = "32-bit float",
            .b = "boolean",
            .s = "string",
            .v = "any value (object)",
        },
    });
}
