//! 型付きの型 (juiz のデータ型、docs/spec/datatype.md §26、issue #576)。
//!
//! `pub const juiz_type: juiz.TypeInfo` 宣言を持つ struct / enum を**型付きの型**と呼ぶ。`juiz-idl -g zig` が作る型と、
//! 同じ宣言を手で書いた型がこれに当たる。辞書 (datatype.md §4) との変換は、ここで `@typeInfo` を見て汎用に行う。
//!
//! | juiz | Zig |
//! |---|---|
//! | boolean / int / long / float / double | bool / i32 / i64 / f32 / f64 |
//! | string | []const u8 |
//! | bytes | juiz.Bytes (辞書では base64) |
//! | array<T> / map<T> | []const T / juiz.Map(T) |
//! | T? | ?T |
//! | record / enum | 型付きの struct / enum (フィールド名は IDL の名前、enum は TypeInfo.symbols の順) |
//! | any | juiz.Value |
//!
//! 引数は libjuiz_sdk が型で読み (capsule_ptr_read_typed_value。足りないフィールドを既定値で埋め、型にないキーを落とし、
//! 知らない記号は列挙の既定値にする)、読んだ辞書を Zig の値にする。戻り値は辞書にして返す (ネイティブ値は使わない)。
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const Value = value.Value;

/// 型の情報。型付きの型は `pub const juiz_type: juiz.TypeInfo = .{ ... };` を持つ
pub const TypeInfo = struct {
    /// 型の完全名 (robot.geometry.Pose2D)
    name: []const u8,
    /// 指紋 (sha256:...)。juiz-idl が生成したときの値で、表示・検査用。マニフェストには libjuiz_sdk が計算した値が載る
    fingerprint: []const u8 = "",
    /// それだけで完結した Avro のスキーマ (.avsc の JSON)
    avro: []const u8,
    /// enum の記号 (Zig の enum のフィールドの順)。空なら Zig のフィールド名を記号にする
    symbols: []const []const u8 = &.{},
};

/// bytes (辞書では base64 の文字列、datatype.md §4)
pub const Bytes = struct {
    pub const juiz_bytes = {};
    data: []const u8 = "",
};

/// map<V> (辞書ではオブジェクト)。並びは辞書を読んだ順
pub fn Map(comptime V: type) type {
    return struct {
        const Self = @This();
        pub const juiz_map_value = V;
        pub const Entry = struct { key: []const u8, value: V };

        entries: []const Entry = &.{},

        /// key の値。無ければ null
        pub fn get(self: Self, key: []const u8) ?V {
            for (self.entries) |e| {
                if (std.mem.eql(u8, e.key, key)) return e.value;
            }
            return null;
        }
    };
}

fn hasDeclSafe(comptime T: type, comptime name: []const u8) bool {
    return switch (@typeInfo(T)) {
        .@"struct", .@"enum", .@"union", .@"opaque" => @hasDecl(T, name),
        else => false,
    };
}

/// T が型付きの型 (juiz_type を持つ struct / enum) か
pub fn isTyped(comptime T: type) bool {
    return hasDeclSafe(T, "juiz_type") and switch (@typeInfo(T)) {
        .@"struct", .@"enum" => true,
        else => false,
    };
}

fn isMap(comptime T: type) bool {
    return hasDeclSafe(T, "juiz_map_value") and T == Map(T.juiz_map_value);
}

/// T が型付きの型の中 (フィールド) に書ける型でなければコンパイルエラーにする
pub fn checkField(comptime T: type, comptime what: []const u8) void {
    if (T == Value or T == []const u8 or T == Bytes) return;
    if (comptime isMap(T)) return checkField(T.juiz_map_value, what);
    switch (@typeInfo(T)) {
        .bool => {},
        .int => |info| if (info.bits > 64) @compileError(what ++ ": " ++ @typeName(T) ++ " is too large"),
        .float => |info| if (info.bits != 32 and info.bits != 64) @compileError(what ++ ": " ++ @typeName(T) ++ " is not supported (use f64 or f32)"),
        .optional => |o| checkField(o.child, what),
        .pointer => |p| {
            if (p.size != .slice or !p.attrs.@"const") @compileError(what ++ ": " ++ @typeName(T) ++ " is not supported (use []const T for array<T>)");
            checkField(p.child, what);
        },
        .@"struct", .@"enum" => if (!isTyped(T)) @compileError(what ++ ": " ++ @typeName(T) ++
            " is not a typed type (generate it with juiz-idl -g zig, or declare pub const juiz_type: juiz.TypeInfo)"),
        else => @compileError(what ++ ": " ++ @typeName(T) ++ " can not be a field of a typed type"),
    }
}

/// 型付きの型の中身を comptime に確かめる (フィールドの型、enum の記号の数)
pub fn checkTyped(comptime T: type) void {
    const info: TypeInfo = T.juiz_type;
    switch (@typeInfo(T)) {
        .@"struct" => |s| {
            for (s.field_names, s.field_types) |name, FT| checkField(FT, info.name ++ "." ++ name);
        },
        .@"enum" => |e| {
            if (info.symbols.len != 0 and info.symbols.len != e.field_names.len) {
                @compileError(info.name ++ ": juiz_type.symbols must list one symbol for each field of the enum");
            }
        },
        else => unreachable,
    }
}

/// enum の記号 (TypeInfo.symbols、無ければフィールド名)
fn symbolOf(comptime T: type, comptime i: usize) []const u8 {
    const info: TypeInfo = T.juiz_type;
    return if (info.symbols.len != 0) info.symbols[i] else @typeInfo(T).@"enum".field_names[i];
}

// ---- libjuiz_sdk の juiz_type ----

/// 型ごとに 1 つの juiz_type (最初に使ったときに作り、プラグインを閉じるまで持つ)
fn Handle(comptime T: type) type {
    return struct {
        pub const Type = T;
        var slot = std.atomic.Value(?*c.juiz_type).init(null);
    };
}

pub const SchemaError = error{SchemaInvalid};

/// T の juiz_type。スキーマが datatype.md §3 に合わなければ error.SchemaInvalid (理由は juiz のログ)
pub fn handle(comptime T: type) SchemaError!*c.juiz_type {
    const H = Handle(T);
    if (H.slot.load(.acquire)) |h| return h;
    const avro: [:0]const u8 = @as(TypeInfo, T.juiz_type).avro ++ "";
    var t: ?*c.juiz_type = null;
    if (c.juiz_type_new(avro.ptr, &t) != c.JUIZ_OK or t == null) return error.SchemaInvalid;
    // ほかのスレッドが先に作っていれば、そちらを使う
    if (H.slot.cmpxchgStrong(null, t, .acq_rel, .acquire)) |existing| {
        c.juiz_type_free(t);
        return existing.?;
    }
    return t.?;
}

/// マニフェストに載せる型参照と type_name (juiz_type_describe の結果)
pub const Description = struct {
    /// {name, fingerprint, avro}
    schema: Value,
    /// manifest の type_name (Object・String など、datatype.md §6)
    type_name: []const u8,
};

pub fn describe(comptime T: type, allocator: std.mem.Allocator) (SchemaError || error{ OutOfMemory, UnsupportedValue })!Description {
    const h = try handle(T);
    var ctx = Capture{ .allocator = allocator };
    const r = c.juiz_type_describe(h, Capture.callback, &ctx);
    if (ctx.err) |e| return e;
    if (r != c.JUIZ_OK) return error.UnsupportedValue;
    const schema = ctx.result.get("schema") orelse return error.UnsupportedValue;
    const type_name = (ctx.result.get("type_name") orelse return error.UnsupportedValue).asString() orelse return error.UnsupportedValue;
    return .{ .schema = schema, .type_name = type_name };
}

/// libjuiz_sdk が callback に渡した value を Value に写す
const Capture = struct {
    allocator: std.mem.Allocator,
    result: Value = .null,
    err: ?error{ OutOfMemory, UnsupportedValue } = null,

    fn callback(arg: ?*anyopaque, v: ?*c.value) callconv(.c) i64 {
        const self: *Capture = @ptrCast(@alignCast(arg.?));
        self.result = value.read(self.allocator, v) catch |e| {
            self.err = e;
            return c.JUIZ_VALUE_CONVERTER_ERROR;
        };
        return c.JUIZ_OK;
    }
};

pub const ReadError = error{ DoesNotMatchType, SchemaInvalid, OutOfMemory, UnsupportedValue };

/// 引数の capsule_ptr を型付きの型 T として読む。path はエラーの位置の先頭 (引数名)
pub fn readTyped(comptime T: type, allocator: std.mem.Allocator, cp: ?*c.capsule_ptr, path: [:0]const u8) ReadError!T {
    const h = try handle(T);
    var ctx = Capture{ .allocator = allocator };
    const r = c.capsule_ptr_read_typed_value(cp, h, path.ptr, Capture.callback, &ctx);
    if (ctx.err) |e| return e;
    if (r == c.JUIZ_VALUE_DOES_NOT_MATCH_TYPE_ERROR) return error.DoesNotMatchType;
    if (r != c.JUIZ_OK) return error.UnsupportedValue;
    return fromValue(T, allocator, ctx.result) catch |e| switch (e) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.DoesNotMatchType,
    };
}

// ---- 辞書との変換 ----

pub const ConvertError = error{ TypeMismatch, OutOfRange, OutOfMemory };

/// 辞書 (datatype.md §4) を T にする。文字列などは v の持ち主 (allocator) のものを指す
pub fn fromValue(comptime T: type, allocator: std.mem.Allocator, v: Value) ConvertError!T {
    if (T == Value) return v;
    if (T == []const u8) return v.asString() orelse error.TypeMismatch;
    if (T == Bytes) {
        const s = v.asString() orelse return error.TypeMismatch;
        const d = std.base64.standard.Decoder;
        const n = d.calcSizeForSlice(s) catch return error.TypeMismatch;
        const buf = try allocator.alloc(u8, n);
        d.decode(buf, s) catch return error.TypeMismatch;
        return .{ .data = buf };
    }
    if (comptime isMap(T)) {
        const members = v.asObject() orelse return error.TypeMismatch;
        const entries = try allocator.alloc(T.Entry, members.len);
        for (members, entries) |m, *e| e.* = .{ .key = m.key, .value = try fromValue(T.juiz_map_value, allocator, m.value) };
        return .{ .entries = entries };
    }
    switch (@typeInfo(T)) {
        .bool => return v.asBool() orelse error.TypeMismatch,
        .int => return switch (v) {
            .int, .uint => v.asInt(T) orelse error.OutOfRange,
            else => error.TypeMismatch,
        },
        .float => return @floatCast(v.asFloat() orelse return error.TypeMismatch),
        .optional => |o| return if (v == .null) null else try fromValue(o.child, allocator, v),
        .pointer => |p| {
            const items = v.asArray() orelse return error.TypeMismatch;
            const out = try allocator.alloc(p.child, items.len);
            for (items, out) |item, *o| o.* = try fromValue(p.child, allocator, item);
            return out;
        },
        .@"struct" => |s| {
            comptime checkTyped(T);
            if (v != .object) return error.TypeMismatch;
            var out: T = undefined;
            inline for (s.field_names, s.field_types, s.field_attrs) |name, FT, attrs| {
                if (v.get(name)) |fv| {
                    @field(out, name) = try fromValue(FT, allocator, fv);
                } else if (attrs.defaultValue(FT)) |d| {
                    @field(out, name) = d;
                } else {
                    return error.TypeMismatch;
                }
            }
            return out;
        },
        .@"enum" => |e| {
            comptime checkTyped(T);
            const s = v.asString() orelse return error.TypeMismatch;
            inline for (e.field_names, 0..) |name, i| {
                if (std.mem.eql(u8, s, comptime symbolOf(T, i))) return @field(T, name);
            }
            if (comptime @hasDecl(T, "juiz_default")) return T.juiz_default;
            return error.TypeMismatch;
        },
        else => @compileError(@typeName(T) ++ " can not be converted from a juiz value"),
    }
}

/// T の値を辞書 (datatype.md §4) にする。すべてのフィールドを出す (既定値と同じ値でも省かない)
pub fn toValue(comptime T: type, allocator: std.mem.Allocator, x: T) error{OutOfMemory}!Value {
    if (T == Value) return x;
    if (T == []const u8) return .{ .string = x };
    if (T == Bytes) {
        const e = std.base64.standard.Encoder;
        const buf = try allocator.alloc(u8, e.calcSize(x.data.len));
        return .{ .string = e.encode(buf, x.data) };
    }
    if (comptime isMap(T)) {
        const members = try allocator.alloc(Value.Member, x.entries.len);
        for (x.entries, members) |e, *m| m.* = .{ .key = e.key, .value = try toValue(T.juiz_map_value, allocator, e.value) };
        return .{ .object = members };
    }
    switch (@typeInfo(T)) {
        .bool => return .{ .bool = x },
        .int => |info| {
            if (info.signedness == .signed) return .{ .int = @intCast(x) };
            if (std.math.cast(i64, x)) |i| return .{ .int = i };
            return .{ .uint = @intCast(x) };
        },
        .float => return .{ .float = @floatCast(x) },
        .optional => |o| return if (x) |y| try toValue(o.child, allocator, y) else .null,
        .pointer => |p| {
            const items = try allocator.alloc(Value, x.len);
            for (x, items) |e, *item| item.* = try toValue(p.child, allocator, e);
            return .{ .array = items };
        },
        .@"struct" => |s| {
            comptime checkTyped(T);
            const members = try allocator.alloc(Value.Member, s.field_names.len);
            inline for (s.field_names, s.field_types, 0..) |name, FT, i| {
                members[i] = .{ .key = name, .value = try toValue(FT, allocator, @field(x, name)) };
            }
            return .{ .object = members };
        },
        .@"enum" => |e| {
            comptime checkTyped(T);
            inline for (e.field_names, 0..) |name, i| {
                if (x == @field(T, name)) return .{ .string = comptime symbolOf(T, i) };
            }
            unreachable;
        },
        else => @compileError(@typeName(T) ++ " can not be converted to a juiz value"),
    }
}
