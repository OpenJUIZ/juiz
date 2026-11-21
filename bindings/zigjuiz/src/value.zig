//! juiz の値 (JSON 相当) を Zig で持つ型 `juiz.Value` と、libjuiz_sdk の value との読み書き。
//!
//! Value は libjuiz_sdk から借りずに Zig 側に写した値で、文字列・配列・辞書の中身は
//! 読んだときに渡したアロケータ (Process の呼び出しごとの arena) が持つ。
const std = @import("std");
const c = @import("c");

pub const Value = union(enum) {
    null,
    bool: bool,
    int: i64,
    uint: u64,
    float: f64,
    string: []const u8,
    array: []const Value,
    /// 辞書。キーの順は libjuiz_sdk が渡した順 (書くときはこの順で書く)
    object: []const Member,

    pub const Member = struct {
        key: []const u8,
        value: Value,
    };

    /// 辞書の要素。辞書でないか、キーが無ければ null
    pub fn get(self: Value, key: []const u8) ?Value {
        switch (self) {
            .object => |members| {
                for (members) |m| {
                    if (std.mem.eql(u8, m.key, key)) return m.value;
                }
                return null;
            },
            else => return null,
        }
    }

    /// 整数として読む (int・uint のどちらでも、型に収まれば)。整数でなければ null
    pub fn asInt(self: Value, comptime T: type) ?T {
        return switch (self) {
            .int => |v| std.math.cast(T, v),
            .uint => |v| std.math.cast(T, v),
            else => null,
        };
    }

    /// 数として読む (float のほか int・uint も)。数でなければ null
    pub fn asFloat(self: Value) ?f64 {
        return switch (self) {
            .float => |v| v,
            .int => |v| @floatFromInt(v),
            .uint => |v| @floatFromInt(v),
            else => null,
        };
    }

    pub fn asBool(self: Value) ?bool {
        return switch (self) {
            .bool => |v| v,
            else => null,
        };
    }

    pub fn asString(self: Value) ?[]const u8 {
        return switch (self) {
            .string => |v| v,
            else => null,
        };
    }

    pub fn asArray(self: Value) ?[]const Value {
        return switch (self) {
            .array => |v| v,
            else => null,
        };
    }

    pub fn asObject(self: Value) ?[]const Member {
        return switch (self) {
            .object => |v| v,
            else => null,
        };
    }
};

// ---- libjuiz_sdk の value から読む ----

/// libjuiz_sdk の value を Value に写す。文字列などは allocator に複製する
pub fn read(allocator: std.mem.Allocator, v: ?*c.value) error{ OutOfMemory, UnsupportedValue }!Value {
    if (v == null or c.value_is_null(v)) return .null;
    if (c.value_is_bool(v)) {
        var b: bool = false;
        if (c.value_get_bool(v, &b) != c.JUIZ_OK) return error.UnsupportedValue;
        return .{ .bool = b };
    }
    if (c.value_is_int(v)) {
        var i: i64 = 0;
        if (c.value_get_int(v, &i) != c.JUIZ_OK) return error.UnsupportedValue;
        return .{ .int = i };
    }
    if (c.value_is_uint(v)) {
        var u: u64 = 0;
        if (c.value_get_uint(v, &u) != c.JUIZ_OK) return error.UnsupportedValue;
        return .{ .uint = u };
    }
    if (c.value_is_float(v)) {
        var f: f64 = 0;
        if (c.value_get_float(v, &f) != c.JUIZ_OK) return error.UnsupportedValue;
        return .{ .float = f };
    }
    if (c.value_is_string(v)) {
        var p: [*c]u8 = null;
        var len: usize = 0;
        if (c.value_get_string_with_len(v, &p, &len) != c.JUIZ_OK) return error.UnsupportedValue;
        return .{ .string = try allocator.dupe(u8, if (len == 0) "" else p[0..len]) };
    }
    if (c.value_is_array(v)) {
        var reader = CollectionReader(Value){ .allocator = allocator };
        _ = c.value_array_foreach(v, CollectionReader(Value).onElement, &reader);
        if (reader.err) |e| return e;
        return .{ .array = try reader.items.toOwnedSlice(allocator) };
    }
    if (c.value_is_object(v)) {
        var reader = CollectionReader(Value.Member){ .allocator = allocator };
        _ = c.value_object_foreach(v, CollectionReader(Value.Member).onMember, &reader);
        if (reader.err) |e| return e;
        return .{ .object = try reader.items.toOwnedSlice(allocator) };
    }
    return error.UnsupportedValue;
}

fn CollectionReader(comptime Item: type) type {
    return struct {
        const Self = @This();
        allocator: std.mem.Allocator,
        items: std.ArrayList(Item) = .empty,
        err: ?error{ OutOfMemory, UnsupportedValue } = null,

        fn onElement(arg: ?*anyopaque, v: ?*c.value) callconv(.c) void {
            const self: *Self = @ptrCast(@alignCast(arg.?));
            if (self.err != null) return;
            const item = read(self.allocator, v) catch |e| {
                self.err = e;
                return;
            };
            self.items.append(self.allocator, item) catch |e| {
                self.err = e;
            };
        }

        fn onMember(arg: ?*anyopaque, key: [*c]const u8, v: ?*c.value) callconv(.c) void {
            const self: *Self = @ptrCast(@alignCast(arg.?));
            if (self.err != null) return;
            const k = self.allocator.dupe(u8, std.mem.span(key)) catch |e| {
                self.err = e;
                return;
            };
            const item = read(self.allocator, v) catch |e| {
                self.err = e;
                return;
            };
            self.items.append(self.allocator, .{ .key = k, .value = item }) catch |e| {
                self.err = e;
            };
        }
    };
}

/// capsule_ptr (Process の引数など) の中身を Value に写す
pub fn readCapsulePtr(allocator: std.mem.Allocator, cp: ?*c.capsule_ptr) error{ OutOfMemory, UnsupportedValue }!Value {
    var ctx = LockRead{ .allocator = allocator };
    const r = c.capsule_ptr_lock_as_value_with_arg(cp, LockRead.callback, &ctx);
    if (ctx.err) |e| return e;
    if (r != c.JUIZ_OK) return error.UnsupportedValue;
    return ctx.result;
}

const LockRead = struct {
    allocator: std.mem.Allocator,
    result: Value = .null,
    err: ?error{ OutOfMemory, UnsupportedValue } = null,

    fn callback(arg: ?*anyopaque, v: ?*c.value) callconv(.c) i64 {
        const self: *LockRead = @ptrCast(@alignCast(arg.?));
        self.result = read(self.allocator, v) catch |e| {
            self.err = e;
            return c.JUIZ_VALUE_CONVERTER_ERROR;
        };
        return c.JUIZ_OK;
    }
};

// ---- libjuiz_sdk の value に書く ----

/// 辞書 obj の key に value を入れる (value_object_set_*)。libjuiz_sdk の関数は NUL 終端の文字列を取るので allocator に複製する
pub fn setMember(allocator: std.mem.Allocator, obj: ?*c.value, key: []const u8, value: Value) error{ OutOfMemory, WriteFailed }!void {
    const k = try allocator.dupeSentinel(u8, key, 0);
    const child: ?*c.value = switch (value) {
        .null => c.value_object_set_null(obj, k),
        .bool => |b| c.value_object_set_bool(obj, k, @intFromBool(b)),
        .int => |i| c.value_object_set_int(obj, k, i),
        .uint => |u| c.value_object_set_uint(obj, k, u),
        .float => |f| c.value_object_set_float(obj, k, f),
        .string => |s| c.value_object_set_string(obj, k, try allocator.dupeSentinel(u8, s, 0)),
        .array => c.value_object_set_empty_array(obj, k),
        .object => c.value_object_set_empty_object(obj, k),
    };
    if (child == null) return error.WriteFailed;
    try fillCollection(allocator, child, value);
}

/// 配列 arr の末尾に value を足す (value_array_push_*)
pub fn pushElement(allocator: std.mem.Allocator, arr: ?*c.value, value: Value) error{ OutOfMemory, WriteFailed }!void {
    const child: ?*c.value = switch (value) {
        .null => c.value_array_push_null(arr),
        .bool => |b| c.value_array_push_bool(arr, @intFromBool(b)),
        .int => |i| c.value_array_push_int(arr, i),
        .uint => |u| c.value_array_push_uint(arr, u),
        .float => |f| c.value_array_push_float(arr, f),
        .string => |s| c.value_array_push_string(arr, try allocator.dupeSentinel(u8, s, 0)),
        .array => c.value_array_push_empty_array(arr),
        .object => c.value_array_push_empty_object(arr),
    };
    if (child == null) return error.WriteFailed;
    try fillCollection(allocator, child, value);
}

/// 空の配列・辞書として作った target に、value (配列・辞書) の中身を書く。ほかの値では何もしない
pub fn fillCollection(allocator: std.mem.Allocator, target: ?*c.value, value: Value) error{ OutOfMemory, WriteFailed }!void {
    switch (value) {
        .array => |items| for (items) |item| try pushElement(allocator, target, item),
        .object => |members| for (members) |m| try setMember(allocator, target, m.key, m.value),
        else => {},
    }
}

/// Value を capsule (Process の戻り値) に書く
pub fn writeCapsule(allocator: std.mem.Allocator, out: ?*c.capsule, value: Value) error{ OutOfMemory, WriteFailed }!void {
    const r: i64 = switch (value) {
        .null => c.capsule_set_null(out),
        .bool => |b| c.capsule_set_bool(out, @intFromBool(b)),
        .int => |i| c.capsule_set_int(out, i),
        .uint => |u| c.capsule_set_uint(out, u),
        .float => |f| c.capsule_set_float(out, f),
        .string => |s| c.capsule_set_string(out, try allocator.dupeSentinel(u8, s, 0)),
        .array, .object => blk: {
            const r0 = if (value == .array) c.capsule_set_empty_array(out) else c.capsule_set_empty_object(out);
            if (r0 != c.JUIZ_OK) break :blk r0;
            var ctx = LockWrite{ .allocator = allocator, .value = value };
            const r1 = c.capsule_lock_as_value_with_arg(out, LockWrite.callback, &ctx);
            if (ctx.err) |e| return e;
            break :blk r1;
        },
    };
    if (r != c.JUIZ_OK) return error.WriteFailed;
}

/// Value を capsule_ptr (マニフェストなど) に書く。配列・辞書だけ
pub fn writeCapsulePtr(allocator: std.mem.Allocator, cp: ?*c.capsule_ptr, value: Value) error{ OutOfMemory, WriteFailed }!void {
    const r0 = switch (value) {
        .array => c.capsule_ptr_set_empty_array(cp),
        .object => c.capsule_ptr_set_empty_object(cp),
        else => return error.WriteFailed,
    };
    if (r0 != c.JUIZ_OK) return error.WriteFailed;
    var ctx = LockWrite{ .allocator = allocator, .value = value };
    const r1 = c.capsule_ptr_lock_as_value_with_arg(cp, LockWrite.callback, &ctx);
    if (ctx.err) |e| return e;
    if (r1 != c.JUIZ_OK) return error.WriteFailed;
}

const LockWrite = struct {
    allocator: std.mem.Allocator,
    value: Value,
    err: ?error{ OutOfMemory, WriteFailed } = null,

    fn callback(arg: ?*anyopaque, v: ?*c.value) callconv(.c) i64 {
        const self: *LockWrite = @ptrCast(@alignCast(arg.?));
        fillCollection(self.allocator, v, self.value) catch |e| {
            self.err = e;
            return c.JUIZ_VALUE_CONVERTER_ERROR;
        };
        return c.JUIZ_OK;
    }
};
