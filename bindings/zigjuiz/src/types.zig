//! Process の引数・戻り値に使える Zig の型と、juiz の値との変換 (comptime に型で振り分ける)。
//!
//! | Zig の型 | manifest の type_name | 読み方 |
//! |---|---|---|
//! | 整数 (i64・u64・i32 など 64 bit 以下) | Int | 型に収まらなければ引数のエラー |
//! | f64・f32 | Float | 整数の値も受ける |
//! | bool | Bool | |
//! | []const u8 | String | 呼び出しの arena に複製する |
//! | juiz.Value | Object | 辞書・配列などそのまま |
//! | 型付きの struct / enum (datatype.zig) | Object / String と schema | libjuiz_sdk が型で読む (issue #576) |
//! | juiz.Image | Image | 画像を arena に複製する (image.zig、issue #576) |
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const datatype = @import("datatype.zig");
const image = @import("image.zig");
const Value = value.Value;

const string_type = []const u8;

/// T が引数・戻り値に使える型でなければコンパイルエラーにする。what はエラー文に入れる場所の説明
pub fn check(comptime T: type, comptime what: []const u8) void {
    _ = typeName(T, what);
}

/// manifest の引数の type_name
pub fn typeName(comptime T: type, comptime what: []const u8) []const u8 {
    if (T == Value) return "Object";
    if (T == string_type) return "String";
    if (T == image.Image) return "Image";
    if (comptime datatype.isTyped(T)) {
        datatype.checkTyped(T);
        // datatype.md §6 の表 (record は Object、enum は String)
        return if (@typeInfo(T) == .@"enum") "String" else "Object";
    }
    switch (@typeInfo(T)) {
        .int => |info| {
            if (info.bits > 64) @compileError(what ++ ": " ++ @typeName(T) ++ " is too large (zigjuiz supports integers up to 64 bits)");
            return "Int";
        },
        .float => |info| {
            if (info.bits != 32 and info.bits != 64) @compileError(what ++ ": " ++ @typeName(T) ++ " is not supported (use f64 or f32)");
            return "Float";
        },
        .bool => return "Bool",
        else => @compileError(what ++ ": " ++ @typeName(T) ++ " is not supported by zigjuiz " ++
            "(use an integer, f64, f32, bool, []const u8, juiz.Value, juiz.Image or a typed type made by juiz-idl -g zig)"),
    }
}

/// T の値を Value にする (既定値をマニフェストに書くとき・戻り値を書くとき)。juiz.Image は値にならない (呼ばない)
pub fn toValue(comptime T: type, allocator: std.mem.Allocator, v: T) error{OutOfMemory}!Value {
    if (comptime datatype.isTyped(T)) return datatype.toValue(T, allocator, v);
    if (T == Value) return v;
    if (T == string_type) return .{ .string = v };
    switch (@typeInfo(T)) {
        .int => |info| {
            if (info.signedness == .signed) return .{ .int = @intCast(v) };
            // 符号なしでも i64 に収まれば Int で書く (juiz の Int は i64)
            if (std.math.cast(i64, v)) |i| return .{ .int = i };
            return .{ .uint = @intCast(v) };
        },
        .float => return .{ .float = @floatCast(v) },
        .bool => return .{ .bool = v },
        else => unreachable,
    }
}

pub const ReadError = error{ TypeMismatch, OutOfRange, OutOfMemory, UnsupportedValue, DoesNotMatchType, SchemaInvalid, NotImage, ImageFormat };

/// 引数の capsule_ptr を T として読む。文字列などは allocator に複製する。name は引数名 (型付きの型のエラーの位置)
pub fn readArg(comptime T: type, allocator: std.mem.Allocator, cp: ?*c.capsule_ptr, comptime name: []const u8) ReadError!T {
    if (comptime datatype.isTyped(T)) return datatype.readTyped(T, allocator, cp, name ++ "");
    if (T == image.Image) return image.read(allocator, cp);
    if (T == Value) return value.readCapsulePtr(allocator, cp);
    if (T == string_type) {
        var p: [*c]u8 = null;
        var len: usize = 0;
        if (c.capsule_ptr_get_string_with_len(cp, &p, &len) != c.JUIZ_OK) return error.TypeMismatch;
        return try allocator.dupe(u8, if (len == 0) "" else p[0..len]);
    }
    switch (@typeInfo(T)) {
        .int => |info| {
            var i: i64 = 0;
            if (c.capsule_ptr_get_int(cp, &i) == c.JUIZ_OK) {
                return std.math.cast(T, i) orelse error.OutOfRange;
            }
            if (info.signedness == .unsigned) {
                var u: u64 = 0;
                if (c.capsule_ptr_get_uint(cp, &u) == c.JUIZ_OK) {
                    return std.math.cast(T, u) orelse error.OutOfRange;
                }
            }
            return error.TypeMismatch;
        },
        .float => {
            var f: f64 = 0;
            if (c.capsule_ptr_get_float(cp, &f) == c.JUIZ_OK) return @floatCast(f);
            // 1 のように小数点の無い数も受ける
            var i: i64 = 0;
            if (c.capsule_ptr_get_int(cp, &i) == c.JUIZ_OK) return @floatFromInt(i);
            var u: u64 = 0;
            if (c.capsule_ptr_get_uint(cp, &u) == c.JUIZ_OK) return @floatFromInt(u);
            return error.TypeMismatch;
        },
        .bool => {
            var b: i64 = 0;
            if (c.capsule_ptr_get_bool(cp, &b) != c.JUIZ_OK) return error.TypeMismatch;
            return b != 0;
        },
        else => unreachable,
    }
}

/// 戻り値を capsule に書く
pub fn writeResult(comptime T: type, allocator: std.mem.Allocator, out: ?*c.capsule, v: T) error{ OutOfMemory, WriteFailed, ImageFormat }!void {
    if (T == void) {
        if (c.capsule_set_null(out) != c.JUIZ_OK) return error.WriteFailed;
        return;
    }
    if (T == image.Image) return image.write(out, v);
    return value.writeCapsule(allocator, out, try toValue(T, allocator, v));
}

/// readArg のエラーの説明 (引数のエラー文に入れる)
pub fn describeReadError(comptime T: type, e: ReadError) []const u8 {
    return switch (e) {
        error.TypeMismatch => "expected " ++ typeName(T, "") ++ " (" ++ @typeName(T) ++ ")",
        error.OutOfRange => "value does not fit in " ++ @typeName(T),
        error.OutOfMemory => "out of memory",
        error.UnsupportedValue => "value could not be read",
        error.DoesNotMatchType => "the value does not match the type " ++ typedName(T) ++ " (see the juiz log for the reason)",
        error.SchemaInvalid => "the schema of " ++ typedName(T) ++ " is invalid (see the juiz log for the reason)",
        error.NotImage => "expected an image",
        error.ImageFormat => "the image has a format that zigjuiz can not read",
    };
}

fn typedName(comptime T: type) []const u8 {
    return if (comptime datatype.isTyped(T)) @as(datatype.TypeInfo, T.juiz_type).name else @typeName(T);
}

/// readArg のエラーのコード。型に合わない値は JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR (C++ / Haskell と同じ)、ほかは JUIZ_ZIG_ARGUMENT_ERROR
pub fn readErrorCode(e: ReadError) i64 {
    return switch (e) {
        error.DoesNotMatchType => c.JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR,
        else => c.JUIZ_ZIG_ARGUMENT_ERROR,
    };
}
