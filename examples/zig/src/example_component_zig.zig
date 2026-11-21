//! 1 つのライブラリに Process・Container・ContainerProcess をまとめた Component (example_component_zig、issue #575)。
//!
//! 設定ファイルの plugins の components に、ライブラリの名前 (example_component_zig) を書いて読み込む
//! (example_component.conf)。要素ごとに exportProcess などを書く代わりに、juiz.exportComponent に並べる。
//! Container と ContainerProcess が同じライブラリなので、状態の型を別のファイルに分けなくてよい。
const std = @import("std");
const juiz = @import("juiz");

/// 作られて、まだ削除されていない Container の数 (テスト用)
var alive_count = std.atomic.Value(i64).init(0);

/// Container の状態。整数を一つ保持する
const Counter = struct {
    value: i64,

    /// Container を削除するときに zigjuiz が呼ぶ
    pub fn deinit(self: *Counter) void {
        _ = self;
        _ = alive_count.fetchSub(1, .monotonic);
    }
};

fn increment(args: struct { arg0: i64 = 1 }) !i64 {
    return std.math.add(i64, args.arg0, 1);
}

fn greet(ctx: juiz.Context, args: struct { name: []const u8 = "juiz" }) ![]const u8 {
    return std.fmt.allocPrint(ctx.allocator, "Hello, {s}!", .{args.name});
}

fn createCounter(args: struct { value: i64 = 0 }) Counter {
    _ = alive_count.fetchAdd(1, .monotonic);
    return .{ .value = args.value };
}

fn counterIncrement(self: *Counter, args: struct { arg0: i64 = 1 }) !i64 {
    self.value = try std.math.add(i64, self.value, args.arg0);
    return self.value;
}

/// 状態を *const で受けるので readonly
fn counterGet(self: *const Counter) i64 {
    return self.value;
}

comptime {
    juiz.exportComponent(.{
        .type_name = "example_component_zig",
        .description = "Processes and a container in one library (Zig).",
        .parts = .{
            juiz.processPart(increment, .{
                .type_name = "example_component_zig_increment",
                .description = "Adds 1 to arg0.",
                .args = .{ .arg0 = "value to be incremented" },
            }),
            juiz.processPart(greet, .{
                .type_name = "example_component_zig_greet",
                .description = "Greets name.",
                .args = .{ .name = "name to greet" },
            }),
            juiz.containerPart(createCounter, .{
                .type_name = "example_component_zig_counter",
                .description = "Holds an integer value.",
                .args = .{ .value = "initial value" },
            }),
            juiz.containerProcessPart(counterIncrement, .{
                .type_name = "example_component_zig_counter_increment",
                .container_type = "example_component_zig_counter",
                .description = "Adds arg0 to the value of the container.",
                .args = .{ .arg0 = "value to be added" },
            }),
            juiz.containerProcessPart(counterGet, .{
                .type_name = "example_component_zig_counter_get",
                .container_type = "example_component_zig_counter",
                .description = "Returns the value of the container.",
            }),
        },
    });
}

/// 作られて、まだ削除されていない Container の数 (テスト用)
export fn example_component_zig_alive_count() i64 {
    return alive_count.load(.monotonic);
}
