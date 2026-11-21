//! 自分のエラーを返す Process (issue #573)。fail が true なら error.RequestedFailure を返す。
//! juiz のエラーの message は "fail_process_zig returned error.RequestedFailure" になる。
const juiz = @import("juiz");

fn failIfAsked(args: struct { fail: bool = false }) error{RequestedFailure}!bool {
    if (args.fail) return error.RequestedFailure;
    return true;
}

comptime {
    juiz.exportProcess(failIfAsked, .{
        .type_name = "fail_process_zig",
        .description = "Returns an error when fail is true (Zig).",
        .args = .{ .fail = "return an error" },
    });
}
