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
const Value = value.Value;

/// Process の関数が最初の引数で受け取れる、呼び出しの情報
pub const Context = struct {
    /// 呼び出しごとの arena。ここで確保したものは、戻り値を juiz に渡し終えたら zigjuiz がまとめて解放する
    /// (戻り値の文字列や juiz.Value の中身をここで作ってよい)
    allocator: std.mem.Allocator,
};

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
    const O = @TypeOf(options);
    const o_info = @typeInfo(O);
    if (o_info != .@"struct") @compileError("exportProcess: options must be a struct literal like .{ .type_name = \"...\" }");
    for (o_info.@"struct".field_names) |name| {
        const known = [_][]const u8{ "type_name", "description", "args", "use_memo" };
        for (known) |k| {
            if (std.mem.eql(u8, k, name)) break;
        } else @compileError("exportProcess: unknown option ." ++ name ++ " (known: .type_name, .description, .args, .use_memo)");
    }
    if (!@hasField(O, "type_name")) @compileError("exportProcess: .type_name is required");
    const type_name: []const u8 = options.type_name;
    if (type_name.len == 0) @compileError("exportProcess: .type_name must not be empty");
    const description: []const u8 = if (@hasField(O, "description")) options.description else "";
    const use_memo: bool = if (@hasField(O, "use_memo")) options.use_memo else false;
    const arg_docs = if (@hasField(O, "args")) options.args else .{};

    const F = @TypeOf(f);
    const f_info = switch (@typeInfo(F)) {
        .@"fn" => |info| info,
        else => @compileError("exportProcess: the first argument must be a function, not " ++ @typeName(F)),
    };
    if (f_info.is_generic) @compileError("exportProcess: " ++ type_name ++ ": the function must not be generic");
    const params = f_info.param_types;
    if (params.len > 2) @compileError("exportProcess: " ++ type_name ++ ": the function takes at most 2 parameters (juiz.Context and an argument struct)");
    const has_ctx = params.len >= 1 and params[0].? == Context;
    if (params.len == 2 and !has_ctx) @compileError("exportProcess: " ++ type_name ++ ": with 2 parameters, the first must be juiz.Context");
    const has_args = params.len == 2 or (params.len == 1 and !has_ctx);
    const Args = if (has_args) params[params.len - 1].? else struct {};
    const args_info = switch (@typeInfo(Args)) {
        .@"struct" => |info| info,
        else => @compileError("exportProcess: " ++ type_name ++ ": arguments must be received as one struct (fn (args: struct { arg1: i64 = 1 }) ...), not " ++ @typeName(Args)),
    };
    if (args_info.is_tuple) @compileError("exportProcess: " ++ type_name ++ ": the argument struct must have named fields");

    // 引数と説明が 1 対 1 に対応しているか
    const Docs = @TypeOf(arg_docs);
    const docs_info = @typeInfo(Docs);
    if (docs_info != .@"struct") @compileError("exportProcess: " ++ type_name ++ ": .args must be a struct literal like .{ .arg1 = \"...\" }");
    for (args_info.field_names, args_info.field_types) |name, T| {
        if (!@hasField(Docs, name)) @compileError("exportProcess: " ++ type_name ++ ": argument '" ++ name ++ "' has no description in .args");
        types.check(T, "exportProcess: " ++ type_name ++ ": argument '" ++ name ++ "'");
    }
    for (docs_info.@"struct".field_names) |name| {
        if (!@hasField(Args, name)) @compileError("exportProcess: " ++ type_name ++ ": .args." ++ name ++ " is not an argument of the function");
    }

    const R = f_info.return_type orelse @compileError("exportProcess: " ++ type_name ++ ": the return type must not be generic");
    const Payload = switch (@typeInfo(R)) {
        .error_union => |info| info.payload,
        else => R,
    };
    if (Payload != void) types.check(Payload, "exportProcess: " ++ type_name ++ ": return type");

    return struct {
        const type_name_z: [:0]const u8 = type_name ++ "";

        fn manifestEntryPoint(ptr: ?*c.capsule_ptr) callconv(.c) i64 {
            var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
            defer arena.deinit();
            const a = arena.allocator();
            const manifest = buildManifest(a) catch return c.JUIZ_VALUE_CONVERTER_ERROR;
            value.writeCapsulePtr(a, ptr, manifest) catch return c.JUIZ_VALUE_CONVERTER_ERROR;
            return c.JUIZ_OK;
        }

        fn buildManifest(a: std.mem.Allocator) !Value {
            var arguments = try a.alloc(Value, args_info.field_names.len);
            inline for (args_info.field_names, args_info.field_types, args_info.field_attrs, 0..) |name, T, attrs, i| {
                const default: Value = if (attrs.defaultValue(T)) |d| types.toValue(T, d) else .null;
                const doc: []const u8 = @field(arg_docs, name);
                const members = try a.dupe(Value.Member, &.{
                    .{ .key = "name", .value = .{ .string = name } },
                    .{ .key = "type_name", .value = .{ .string = comptime types.typeName(T, "") } },
                    .{ .key = "description", .value = .{ .string = doc } },
                    .{ .key = "default", .value = default },
                });
                arguments[i] = .{ .object = members };
            }
            const members = try a.dupe(Value.Member, &.{
                .{ .key = "type_name", .value = .{ .string = type_name } },
                .{ .key = "language", .value = .{ .string = "zig" } },
                .{ .key = "description", .value = .{ .string = description } },
                .{ .key = "factory", .value = .{ .string = "process_factory" } },
                .{ .key = "use_memo", .value = .{ .bool = use_memo } },
                .{ .key = "arguments", .value = .{ .array = arguments } },
            });
            return .{ .object = members };
        }

        fn call(cm: ?*c.capsule_map, out: ?*c.capsule) callconv(.c) i64 {
            var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
            defer arena.deinit();
            const a = arena.allocator();

            var args: Args = undefined;
            inline for (args_info.field_names, args_info.field_types, args_info.field_attrs) |name, T, attrs| {
                var cp: ?*c.capsule_ptr = null;
                if (c.capsule_map_get_capsule(cm, name, &cp) != c.JUIZ_OK or cp == null) {
                    // juiz は省いた引数をマニフェストの既定値で補うので、ふつうはここに来ない
                    if (attrs.defaultValue(T)) |d| {
                        @field(args, name) = d;
                    } else {
                        return fail(out, errors.zig_argument_error, "argument '" ++ name ++ "' of " ++ type_name ++ " is missing");
                    }
                } else {
                    @field(args, name) = types.readArg(T, a, cp) catch |e| {
                        return fail(out, errors.zig_argument_error, switch (e) {
                            inline else => |err| "argument '" ++ name ++ "' of " ++ type_name ++ ": " ++ comptime types.describeReadError(T, err),
                        });
                    };
                }
            }

            const ctx = Context{ .allocator = a };
            const fargs = if (has_ctx and has_args) .{ ctx, args } else if (has_ctx) .{ctx} else if (has_args) .{args} else .{};
            const raw = @call(.auto, f, fargs);
            const result: Payload = switch (@typeInfo(R)) {
                .error_union => raw catch |e| {
                    var buf: [256]u8 = undefined;
                    const msg = std.fmt.bufPrint(&buf, "{s} returned error.{s}", .{ type_name, @errorName(e) }) catch "error returned";
                    return fail(out, errors.zig_error_returned, msg);
                },
                else => raw,
            };
            types.writeResult(Payload, a, out, result) catch {
                return fail(out, errors.zig_result_error, "failed to write the result of " ++ type_name);
            };
            return c.JUIZ_OK;
        }

        fn processFactoryEntryPoint() callconv(.c) *const fn (?*c.capsule_map, ?*c.capsule) callconv(.c) i64 {
            return &call;
        }
    };
}

/// エラー文を戻り値の capsule に入れて code を返す
fn fail(out: ?*c.capsule, code: i64, message: []const u8) i64 {
    var buf: [512]u8 = undefined;
    const n = @min(message.len, buf.len - 1);
    @memcpy(buf[0..n], message[0..n]);
    buf[n] = 0;
    _ = c.capsule_set_string(out, @ptrCast(&buf));
    return code;
}
