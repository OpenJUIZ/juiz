//! 文字列を組み立てて返す Process (issue #573)。
//!
//! 最初の引数に juiz.Context を受けると、呼び出しごとの arena (ctx.allocator) が使える。
//! arena で作った戻り値は、zigjuiz が juiz に渡し終えてから解放する。
const std = @import("std");
const juiz = @import("juiz");

fn greet(ctx: juiz.Context, args: struct { name: []const u8 = "juiz", times: u32 = 1 }) ![]const u8 {
    var text: std.ArrayList(u8) = .empty;
    for (0..args.times) |_| {
        try text.appendSlice(ctx.allocator, "Hello, ");
        try text.appendSlice(ctx.allocator, args.name);
        try text.appendSlice(ctx.allocator, "! ");
    }
    return std.mem.trimEnd(u8, text.items, " ");
}

comptime {
    juiz.exportProcess(greet, .{
        .type_name = "greet_process_zig",
        .description = "Greets name, times times (Zig).",
        .args = .{
            .name = "name to greet",
            .times = "how many times to greet",
        },
    });
}
