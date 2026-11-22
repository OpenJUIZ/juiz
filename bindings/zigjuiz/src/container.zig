//! juiz.exportContainer / juiz.exportContainerProcess: Zig の値を juiz の Container の状態にし、
//! それを読み書きする関数を ContainerProcess として公開する (issue #574)。
//!
//! ```zig
//! // counter.zig (Container と ContainerProcess の両方から @import する)
//! pub const Counter = struct { value: i64 };
//!
//! // example_container_zig.zig
//! fn create(args: struct { value: i64 = 0 }) !counter.Counter { return .{ .value = args.value }; }
//! comptime { juiz.exportContainer(create, .{ .type_name = "example_container_zig", .args = .{ .value = "initial value" } }); }
//!
//! // example_container_zig_increment.zig
//! fn increment(self: *counter.Counter, args: struct { arg0: i64 = 1 }) !i64 { ... }
//! comptime { juiz.exportContainerProcess(increment, .{ .type_name = "...", .container_type = "example_container_zig", .args = ... }); }
//! ```
//!
//! juiz に渡す Container の `void*` は、先頭に目印 (zigjuiz の印と、状態の型の名前・大きさから作った値) を置いた
//! Header を指す。ContainerProcess は目印を比べ、zigjuiz の Container でないか、状態の型が違えば
//! JUIZ_ZIG_CONTAINER_TYPE_ERROR (-1104) を返す (別の言語の Container を読んで落ちないように)。
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const types = @import("types.zig");
const errors = @import("errors.zig");
const entry = @import("entry.zig");
const process = @import("process.zig");
const pose = @import("pose.zig");
const Value = value.Value;

/// zigjuiz の Container の目印 ("ZIGJUIZC" の 8 バイト)
const magic: u64 = 0x435a49554a47495a;

/// juiz に渡す Container の中身 (C の構造体と同じ並び。別のライブラリの zigjuiz からも同じ位置で読めるように)
const Header = extern struct {
    magic: u64,
    /// 状態の型の目印 (typeKey)
    type_key: u64,
    /// 状態の型の名前 (エラー文に使う。Container を作ったライブラリの中の文字列)
    type_name: [*:0]const u8,
    /// 利用者の状態 (State) を c_allocator で確保したもの
    state: *anyopaque,
};

/// 状態の型の目印。型の名前 (@typeName、モジュールの中の位置を含む)・大きさ・整列から作る。
/// Container と ContainerProcess は別々のライブラリになるので、両方で **同じファイルの同じ型** を @import して使う
fn typeKey(comptime State: type) u64 {
    @setEvalBranchQuota(100000);
    const name = @typeName(State);
    var h = std.hash.Wyhash.init(0);
    h.update(name);
    h.update(std.mem.asBytes(&@as(u64, @sizeOf(State))));
    h.update(std.mem.asBytes(&@as(u64, @alignOf(State))));
    return h.final();
}

fn hasDeinit(comptime State: type) bool {
    return switch (@typeInfo(State)) {
        .@"struct", .@"union", .@"enum", .@"opaque" => @hasDecl(State, "deinit"),
        else => false,
    };
}

/// 関数 create を Container を作る関数として公開する。`comptime { juiz.exportContainer(create, .{ ... }); }` の形で書く。
///
/// create の形は `fn ([ctx: juiz.Context], [args: Args]) State` か `!State`。Args は Process と同じく struct で、
/// conf の containers に書いた値 (無ければ既定値) で呼ばれる。返した State が Container の状態になる。
/// State が `pub fn deinit(self: *State) void` を持っていれば、Container を削除するときに呼んでから解放する。
///
/// オプション: .type_name (必須)、.description、.args (引数の説明、Process と同じ)
pub fn exportContainer(comptime create: anytype, comptime options: anytype) void {
    const Entry = ContainerEntry(create, options);
    @export(&Entry.manifestEntryPoint, .{ .name = "manifest_entry_point" });
    @export(&Entry.containerFactoryEntryPoint, .{ .name = "container_factory_entry_point" });
    @export(&Entry.destroy, .{ .name = "container_factory_destroy_entry_point" });
}

pub fn ContainerEntry(comptime create: anytype, comptime options: anytype) type {
    const type_name = entry.checkOptions("exportContainer", options, &.{ "type_name", "description", "args" });
    const what = "exportContainer: " ++ type_name;
    const description: []const u8 = entry.option(options, "description", []const u8, "");
    const arg_docs = if (@hasField(@TypeOf(options), "args")) options.args else .{};
    const Sig = entry.Signature(create, what, false, arg_docs);
    const State = Sig.PayloadType;
    switch (@typeInfo(State)) {
        .void, .noreturn, .@"opaque", .pointer, .error_set => @compileError(what ++ ": the function must return the container state (a struct or other value), not " ++ @typeName(State)),
        else => {},
    }

    return struct {
        /// Component (component.zig) が要素の種類と名前・状態の型を見るための印
        pub const part_kind: entry.PartKind = .container;
        pub const part_type_name = type_name;
        pub const StateType = State;

        const state_type_name: [:0]const u8 = @typeName(State) ++ "";
        const key = typeKey(State);

        fn manifestEntryPoint(ptr: ?*c.capsule_ptr) callconv(.c) i64 {
            return entry.writeManifest(ptr, buildManifest);
        }

        fn buildManifest(a: std.mem.Allocator) !Value {
            return manifest(a, "container_factory", &.{});
        }

        /// マニフェスト。factory は juiz が <factory>_entry_point を探すときのシンボル名、processes はこの Container の
        /// ContainerProcess のマニフェスト (Component のときだけ入れる)
        pub fn manifest(a: std.mem.Allocator, comptime factory: []const u8, processes: []const Value) !Value {
            const members = try a.dupe(Value.Member, &.{
                .{ .key = "type_name", .value = .{ .string = type_name } },
                .{ .key = "language", .value = .{ .string = "zig" } },
                .{ .key = "description", .value = .{ .string = description } },
                .{ .key = "factory", .value = .{ .string = factory } },
                .{ .key = "arguments", .value = .{ .array = try Sig.manifestArguments(a) } },
                .{ .key = "processes", .value = .{ .array = processes } },
            });
            return .{ .object = members };
        }

        fn construct(cm: ?*c.capsule_map, pobj: ?*?*anyopaque) callconv(.c) i64 {
            var arena = std.heap.ArenaAllocator.init(std.heap.c_allocator);
            defer arena.deinit();
            const a = arena.allocator();

            var failure: entry.Failure = .{};
            const args = Sig.readArgs(a, cm, type_name, &failure) orelse return entry.failToStderr(failure.code, failure.message);
            const raw = Sig.invoke({}, .{ .allocator = a }, args);
            const state: State = switch (@typeInfo(Sig.ReturnType)) {
                .error_union => raw catch |e| {
                    var buf: [512]u8 = undefined;
                    return entry.failToStderr(errors.zig_error_returned, entry.errorReturnedMessage(&buf, type_name, e));
                },
                else => raw,
            };
            const s = std.heap.c_allocator.create(State) catch {
                var st = state;
                if (comptime hasDeinit(State)) st.deinit();
                return entry.failToStderr(errors.zig_error_returned, type_name ++ ": out of memory");
            };
            s.* = state;
            const h = std.heap.c_allocator.create(Header) catch {
                if (comptime hasDeinit(State)) s.deinit();
                std.heap.c_allocator.destroy(s);
                return entry.failToStderr(errors.zig_error_returned, type_name ++ ": out of memory");
            };
            h.* = .{ .magic = magic, .type_key = key, .type_name = state_type_name.ptr, .state = s };
            if (pobj) |p| p.* = h;
            return c.JUIZ_OK;
        }

        pub fn containerFactoryEntryPoint() callconv(.c) *const fn (?*c.capsule_map, ?*?*anyopaque) callconv(.c) i64 {
            return &construct;
        }

        /// juiz が Container を削除するときに呼ぶ。deinit (あれば) を呼んでから、状態と Header を解放する
        pub fn destroy(obj: ?*anyopaque) callconv(.c) void {
            const h: *Header = @ptrCast(@alignCast(obj orelse return));
            // 自分が作った Container だけを解放する。二重に解放しないよう、先に目印を消す
            if (h.magic != magic or h.type_key != key) return;
            h.magic = 0;
            const s: *State = @ptrCast(@alignCast(h.state));
            if (comptime hasDeinit(State)) s.deinit();
            std.heap.c_allocator.destroy(s);
            std.heap.c_allocator.destroy(h);
        }
    };
}

/// 関数 f を ContainerProcess として公開する。`comptime { juiz.exportContainerProcess(f, .{ ... }); }` の形で書く。
///
/// f の形は `fn (self: *State, [ctx: juiz.Context], [args: Args]) R`。self は Container の状態
/// (exportContainer の関数が返した値) へのポインタで、State は Container と同じファイルの同じ型にする。
/// self を `*const State` で受けると readonly の ContainerProcess になる (juiz は Container を読み出しロックで呼ぶので、
/// 同じ Container の readonly 同士は並行に走る。juiz.setPose / juiz.setTransform はコンパイルエラーになる)。
///
/// オプション: .type_name (必須)、.container_type (必須、Container の型名)、.description、.args、.use_memo
pub fn exportContainerProcess(comptime f: anytype, comptime options: anytype) void {
    const Entry = ContainerProcessEntry(f, options);
    @export(&Entry.manifestEntryPoint, .{ .name = "manifest_entry_point" });
    @export(&Entry.containerProcessFactoryEntryPoint, .{ .name = "container_process_factory_entry_point" });
}

pub fn ContainerProcessEntry(comptime f: anytype, comptime options: anytype) type {
    const type_name = entry.checkOptions("exportContainerProcess", options, &.{ "type_name", "container_type", "description", "args", "use_memo" });
    const what = "exportContainerProcess: " ++ type_name;
    if (!@hasField(@TypeOf(options), "container_type")) @compileError(what ++ ": .container_type (the type name of the container) is required");
    const container_type: []const u8 = options.container_type;
    if (container_type.len == 0) @compileError(what ++ ": .container_type must not be empty");
    const description: []const u8 = entry.option(options, "description", []const u8, "");
    const use_memo: bool = entry.option(options, "use_memo", bool, false);
    const arg_docs = if (@hasField(@TypeOf(options), "args")) options.args else .{};
    const Sig = entry.Signature(f, what, true, arg_docs);
    const State = Sig.StateType;
    const Payload = Sig.PayloadType;
    if (Payload != void) types.check(Payload, what ++ ": return type");

    return struct {
        /// Component (component.zig) が要素の種類と名前・Container の型名・状態の型を見るための印
        pub const part_kind: entry.PartKind = .container_process;
        pub const part_type_name = type_name;
        pub const part_container_type = container_type;
        pub const StateType = State;
        pub const SelfType = Sig.SelfPtr;

        const key = typeKey(State);

        fn manifestEntryPoint(ptr: ?*c.capsule_ptr) callconv(.c) i64 {
            return entry.writeManifest(ptr, buildManifest);
        }

        fn buildManifest(a: std.mem.Allocator) !Value {
            return manifest(a, "container_process_factory");
        }

        /// マニフェスト。factory は juiz が <factory>_entry_point を探すときのシンボル名
        pub fn manifest(a: std.mem.Allocator, comptime factory: []const u8) !Value {
            const base = [_]Value.Member{
                .{ .key = "type_name", .value = .{ .string = type_name } },
                .{ .key = "language", .value = .{ .string = "zig" } },
                .{ .key = "description", .value = .{ .string = description } },
                .{ .key = "factory", .value = .{ .string = factory } },
                .{ .key = "use_memo", .value = .{ .bool = use_memo } },
                .{ .key = "arguments", .value = .{ .array = try Sig.manifestArguments(a) } },
                .{ .key = "container_type", .value = .{ .string = container_type } },
            };
            const readonly: []const Value.Member = if (Sig.is_readonly) &.{.{ .key = "readonly", .value = .{ .bool = true } }} else &.{};
            return .{ .object = try std.mem.concat(a, Value.Member, &.{ &base, readonly, try entry.outputMembers(Payload, a) }) };
        }

        fn call(obj: ?*anyopaque, cm: ?*c.capsule_map, out: ?*c.capsule) callconv(.c) i64 {
            const h: *const Header = @ptrCast(@alignCast(obj orelse
                return entry.fail(out, errors.zig_container_type_error, what ++ ": the container is null")));
            if (h.magic != magic) {
                return entry.fail(out, errors.zig_container_type_error, type_name ++ ": the container is not a Zig (zigjuiz) container");
            }
            if (h.type_key != key) {
                var buf: [512]u8 = undefined;
                const msg = std.fmt.bufPrint(&buf, "{s}: the container state is {s}, but the process expects {s}", .{
                    type_name, std.mem.span(h.type_name), @typeName(State),
                }) catch type_name ++ ": the container state has another type";
                return entry.fail(out, errors.zig_container_type_error, msg);
            }
            const s: *State = @ptrCast(@alignCast(h.state));
            // 関数の中から juiz.pose などで呼び出し元 Container の ContainerPose を読み書きできるよう、口を覚えておく
            var pc: pose.PoseCall = .{ .container = s, .cm = cm, .prev = null };
            pose.push(&pc);
            defer pose.pop(&pc);
            return process.callWith(Sig, type_name, s, cm, out);
        }

        pub fn containerProcessFactoryEntryPoint() callconv(.c) *const fn (?*anyopaque, ?*c.capsule_map, ?*c.capsule) callconv(.c) i64 {
            return &call;
        }
    };
}
