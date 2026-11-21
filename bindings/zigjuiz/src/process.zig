//! juiz.exportProcess: Zig の関数を juiz の Process として公開する (comptime に入口とマニフェストを生成する)。
//!
//! juiz が探す C の関数 (bindings/cjuiz/README.md「プラグインが公開する関数」) を @export で作る。
//!   - manifest_entry_point: マニフェストを capsule_ptr に書く
//!   - process_factory_entry_point: Process の関数 (引数を読み、f を呼び、戻り値を書く) を返す
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const types = @import("types.zig");
const errors = @import("errors.zig");
const entry = @import("entry.zig");
const Value = value.Value;

pub const Context = entry.Context;

/// 関数 f を Process として公開する。`comptime { juiz.exportProcess(f, .{ ... }); }` の形で書く。
///
/// f の形は `fn (args: Args) R`・`fn (ctx: juiz.Context, args: Args) R`・`fn () R`・`fn (ctx: juiz.Context) R`。
/// Args は struct で、フィールド名・型・既定値がマニフェストの引数になる。R は使える型 (types.zig) か、そのエラー共用体 (!T)。
pub fn exportProcess(comptime f: anytype, comptime options: anytype) void {
    const Entry = ProcessEntry(f, options);
    @export(&Entry.manifestEntryPoint, .{ .name = "manifest_entry_point" });
    @export(&Entry.processFactoryEntryPoint, .{ .name = "process_factory_entry_point" });
}

/// exportProcess のオプション (無名の struct) で使える項目:
///   .type_name (必須): Process の型名 (conf の type_name と同じにする)
///   .description: 説明
///   .args: 引数の説明。引数の struct のフィールドと同じ名前を全部書く (.{ .arg1 = "..." })
///   .use_memo: 同じ引数で呼ばれたとき前の結果を使い回すか (既定 false)
pub fn ProcessEntry(comptime f: anytype, comptime options: anytype) type {
    const type_name = entry.checkOptions("exportProcess", options, &.{ "type_name", "description", "args", "use_memo" });
    const what = "exportProcess: " ++ type_name;
    const description: []const u8 = entry.option(options, "description", []const u8, "");
    const use_memo: bool = entry.option(options, "use_memo", bool, false);
    const arg_docs = if (@hasField(@TypeOf(options), "args")) options.args else .{};
    const Sig = entry.Signature(f, what, false, arg_docs);
    const Payload = Sig.PayloadType;
    if (Payload != void) types.check(Payload, what ++ ": return type");

    return struct {
        fn manifestEntryPoint(ptr: ?*c.capsule_ptr) callconv(.c) i64 {
            var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
            defer arena.deinit();
            const a = arena.allocator();
            const manifest = buildManifest(a) catch return c.JUIZ_VALUE_CONVERTER_ERROR;
            value.writeCapsulePtr(a, ptr, manifest) catch return c.JUIZ_VALUE_CONVERTER_ERROR;
            return c.JUIZ_OK;
        }

        fn buildManifest(a: std.mem.Allocator) !Value {
            const members = try a.dupe(Value.Member, &.{
                .{ .key = "type_name", .value = .{ .string = type_name } },
                .{ .key = "language", .value = .{ .string = "zig" } },
                .{ .key = "description", .value = .{ .string = description } },
                .{ .key = "factory", .value = .{ .string = "process_factory" } },
                .{ .key = "use_memo", .value = .{ .bool = use_memo } },
                .{ .key = "arguments", .value = .{ .array = try Sig.manifestArguments(a) } },
            });
            return .{ .object = members };
        }

        fn call(cm: ?*c.capsule_map, out: ?*c.capsule) callconv(.c) i64 {
            return callWith(Sig, type_name, {}, cm, out);
        }

        fn processFactoryEntryPoint() callconv(.c) *const fn (?*c.capsule_map, ?*c.capsule) callconv(.c) i64 {
            return &call;
        }
    };
}

/// 引数を読み、関数を呼び、戻り値を out に書く (Process と ContainerProcess で共通)
pub fn callWith(comptime Sig: type, comptime type_name: []const u8, self: Sig.SelfPtr, cm: ?*c.capsule_map, out: ?*c.capsule) i64 {
    var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var message: []const u8 = "";
    const args = Sig.readArgs(a, cm, type_name, &message) orelse return entry.fail(out, errors.zig_argument_error, message);
    const raw = Sig.invoke(self, .{ .allocator = a }, args);
    const result: Sig.PayloadType = switch (@typeInfo(Sig.ReturnType)) {
        .error_union => raw catch |e| {
            var buf: [512]u8 = undefined;
            return entry.fail(out, errors.zig_error_returned, entry.errorReturnedMessage(&buf, type_name, e));
        },
        else => raw,
    };
    types.writeResult(Sig.PayloadType, a, out, result) catch {
        return entry.fail(out, errors.zig_result_error, "failed to write the result of " ++ type_name);
    };
    return c.JUIZ_OK;
}
