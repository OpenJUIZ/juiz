//! exportProcess・exportContainer・exportContainerProcess が共通に使う、comptime の検査と entry point の部品。
//!
//! 関数の形 (引数の並び) は次のどれか。[] は省ける。
//!   ContainerProcess: fn (self: *State または *const State, [ctx: juiz.Context], [args: Args]) R
//!   Process・Container を作る関数: fn ([ctx: juiz.Context], [args: Args]) R
//! Args は struct で、フィールド名・型・既定値がマニフェストの引数になる。R は使える型 (types.zig) か、そのエラー共用体 (!T)。
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const types = @import("types.zig");
const errors = @import("errors.zig");
const pose = @import("pose.zig");
const Value = value.Value;

/// 関数が最初の引数 (ContainerProcess では Container の次) で受け取れる、呼び出しの情報
pub const Context = struct {
    /// 呼び出しごとの arena。ここで確保したものは、戻り値を juiz に渡し終えたら zigjuiz がまとめて解放する
    /// (戻り値の文字列や juiz.Value の中身をここで作ってよい)
    allocator: std.mem.Allocator,
};

/// options (無名の struct) の項目が known に入っているか、.type_name があるかを確かめ、.type_name を返す
pub fn checkOptions(comptime what: []const u8, comptime options: anytype, comptime known: []const []const u8) []const u8 {
    const O = @TypeOf(options);
    const o_info = @typeInfo(O);
    if (o_info != .@"struct") @compileError(what ++ ": options must be a struct literal like .{ .type_name = \"...\" }");
    for (o_info.@"struct".field_names) |name| {
        for (known) |k| {
            if (std.mem.eql(u8, k, name)) break;
        } else {
            comptime var list: []const u8 = "";
            inline for (known, 0..) |k, i| list = list ++ (if (i == 0) "." else ", .") ++ k;
            @compileError(what ++ ": unknown option ." ++ name ++ " (known: " ++ list ++ ")");
        }
    }
    if (!@hasField(O, "type_name")) @compileError(what ++ ": .type_name is required");
    const type_name: []const u8 = options.type_name;
    if (type_name.len == 0) @compileError(what ++ ": .type_name must not be empty");
    return type_name;
}

/// options の項目 name の値。無ければ default
pub fn option(comptime options: anytype, comptime name: []const u8, comptime T: type, comptime default: T) T {
    return if (@hasField(@TypeOf(options), name)) @field(options, name) else default;
}

/// 関数 f の形を調べた結果
pub fn Signature(comptime f: anytype, comptime what: []const u8, comptime with_self: bool, comptime arg_docs: anytype) type {
    const F = @TypeOf(f);
    const f_info = switch (@typeInfo(F)) {
        .@"fn" => |info| info,
        else => @compileError(what ++ ": the first argument must be a function, not " ++ @typeName(F)),
    };
    if (f_info.is_generic) @compileError(what ++ ": the function must not be generic");
    const params = f_info.param_types;
    const n_self: usize = if (with_self) 1 else 0;
    const shape = if (with_self)
        "fn (self: *State, [ctx: juiz.Context], [args: struct { ... }])"
    else
        "fn ([ctx: juiz.Context], [args: struct { ... }])";
    if (params.len < n_self) @compileError(what ++ ": the function must receive the container state first: " ++ shape);
    if (params.len > n_self + 2) @compileError(what ++ ": too many parameters. The function must be " ++ shape);

    // ContainerProcess の最初の引数は Container の状態へのポインタ
    const Self = if (with_self) params[0].? else void;
    const State, const readonly = if (with_self) switch (@typeInfo(Self)) {
        .pointer => |p| blk: {
            if (p.size != .one) @compileError(what ++ ": the first parameter must be a single pointer to the container state (*State or *const State), not " ++ @typeName(Self));
            break :blk .{ p.child, p.attrs.@"const" };
        },
        else => @compileError(what ++ ": the first parameter must be a pointer to the container state (*State or *const State), not " ++ @typeName(Self)),
    } else .{ void, false };

    const rest = params[n_self..];
    const has_ctx = rest.len >= 1 and rest[0].? == Context;
    if (rest.len == 2 and !has_ctx) @compileError(what ++ ": with 2 parameters after the state, the first must be juiz.Context: " ++ shape);
    const has_args = rest.len == 2 or (rest.len == 1 and !has_ctx);
    const Args = if (has_args) rest[rest.len - 1].? else struct {};
    const args_info = switch (@typeInfo(Args)) {
        .@"struct" => |info| info,
        else => @compileError(what ++ ": arguments must be received as one struct (fn (args: struct { arg1: i64 = 1 }) ...), not " ++ @typeName(Args)),
    };
    if (args_info.is_tuple) @compileError(what ++ ": the argument struct must have named fields");

    // 引数と説明が 1 対 1 に対応しているか
    const Docs = @TypeOf(arg_docs);
    const docs_info = @typeInfo(Docs);
    if (docs_info != .@"struct") @compileError(what ++ ": .args must be a struct literal like .{ .arg1 = \"...\" }");
    for (args_info.field_names, args_info.field_types) |name, T| {
        if (!@hasField(Docs, name)) @compileError(what ++ ": argument '" ++ name ++ "' has no description in .args");
        types.check(T, what ++ ": argument '" ++ name ++ "'");
    }
    for (docs_info.@"struct".field_names) |name| {
        if (!@hasField(Args, name)) @compileError(what ++ ": .args." ++ name ++ " is not an argument of the function");
    }

    const R = f_info.return_type orelse @compileError(what ++ ": the return type must not be generic");
    const Payload = switch (@typeInfo(R)) {
        .error_union => |info| info.payload,
        else => R,
    };

    return struct {
        pub const SelfPtr = Self;
        pub const StateType = State;
        pub const is_readonly = readonly;
        pub const ArgsType = Args;
        pub const ReturnType = R;
        pub const PayloadType = Payload;

        /// マニフェストの arguments
        pub fn manifestArguments(a: std.mem.Allocator) ![]Value {
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
            return arguments;
        }

        /// 引数を cm から読む。読めなければエラー文を message に入れて null を返す
        pub fn readArgs(a: std.mem.Allocator, cm: ?*c.capsule_map, comptime type_name: []const u8, message: *[]const u8) ?Args {
            var args: Args = undefined;
            inline for (args_info.field_names, args_info.field_types, args_info.field_attrs) |name, T, attrs| {
                var cp: ?*c.capsule_ptr = null;
                if (c.capsule_map_get_capsule(cm, name, &cp) != c.JUIZ_OK or cp == null) {
                    // juiz は省いた引数をマニフェストの既定値で補うので、ふつうはここに来ない
                    if (attrs.defaultValue(T)) |d| {
                        @field(args, name) = d;
                    } else {
                        message.* = "argument '" ++ name ++ "' of " ++ type_name ++ " is missing";
                        return null;
                    }
                } else {
                    @field(args, name) = types.readArg(T, a, cp) catch |e| {
                        message.* = switch (e) {
                            inline else => |err| "argument '" ++ name ++ "' of " ++ type_name ++ ": " ++ comptime types.describeReadError(T, err),
                        };
                        return null;
                    };
                }
            }
            return args;
        }

        /// f を呼ぶ。self は ContainerProcess のときだけ使う
        pub fn invoke(self: Self, ctx: Context, args: Args) R {
            const fargs = if (with_self)
                (if (has_ctx and has_args) .{ self, ctx, args } else if (has_ctx) .{ self, ctx } else if (has_args) .{ self, args } else .{self})
            else
                (if (has_ctx and has_args) .{ ctx, args } else if (has_ctx) .{ctx} else if (has_args) .{args} else .{});
            return @call(.auto, f, fargs);
        }
    };
}

/// Component (juiz.exportComponent) に入れる要素の種類。juiz.processPart などが返す型が part_kind に持つ
pub const PartKind = enum { process, container, container_process };

/// manifest_entry_point などの中身。build で作ったマニフェストを ptr に書く
pub fn writeManifest(ptr: ?*c.capsule_ptr, comptime build: fn (std.mem.Allocator) anyerror!Value) i64 {
    var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const manifest = build(a) catch return c.JUIZ_VALUE_CONVERTER_ERROR;
    value.writeCapsulePtr(a, ptr, manifest) catch return c.JUIZ_VALUE_CONVERTER_ERROR;
    return c.JUIZ_OK;
}

/// 関数が返した Zig のエラーのエラー文。ContainerPose の読み書きのエラー (juiz.PoseError) なら、juiz が返した理由も足す
pub fn errorReturnedMessage(buf: []u8, type_name: []const u8, e: anyerror) []const u8 {
    if (pose.isPoseError(e)) {
        const reason = std.mem.span(c.juiz_container_pose_last_error());
        if (reason.len > 0) {
            return std.fmt.bufPrint(buf, "{s} returned error.{s}: {s}", .{ type_name, @errorName(e), reason }) catch "error returned";
        }
    }
    return std.fmt.bufPrint(buf, "{s} returned error.{s}", .{ type_name, @errorName(e) }) catch "error returned";
}

/// エラー文を戻り値の capsule に入れて code を返す
pub fn fail(out: ?*c.capsule, code: i64, message: []const u8) i64 {
    var buf: [512]u8 = undefined;
    const n = @min(message.len, buf.len - 1);
    @memcpy(buf[0..n], message[0..n]);
    buf[n] = 0;
    _ = c.capsule_set_string(out, @ptrCast(&buf));
    return code;
}

/// 戻り値の capsule が無い entry point (Container を作る関数) のエラー。juiz は「標準エラーを見よ」と出すので、エラー文はそこに出す
pub fn failToStderr(code: i64, message: []const u8) i64 {
    std.debug.print("zigjuiz: {s}\n", .{message});
    return code;
}
