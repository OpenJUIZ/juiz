//! juiz.exportComponent: 1 つのライブラリに複数の Process・Container・ContainerProcess をまとめて公開する (issue #575)。
//!
//! ```zig
//! comptime {
//!     juiz.exportComponent(.{
//!         .type_name = "example_component_zig",
//!         .description = "Processes and a container in one library.",
//!         .parts = .{
//!             juiz.processPart(increment, .{ .type_name = "example_component_zig_increment", .args = .{ .arg0 = "..." } }),
//!             juiz.containerPart(createCounter, .{ .type_name = "example_component_zig_counter", .args = .{ .value = "..." } }),
//!             juiz.containerProcessPart(counterGet, .{ .type_name = "example_component_zig_counter_get", .container_type = "example_component_zig_counter" }),
//!         },
//!     });
//! }
//! ```
//!
//! juiz が探す C の関数 (bindings/cjuiz/README.md「Component」) を @export で作る。
//!   - component_manifest_entry_point: 要素のマニフェストをまとめた Component のマニフェストを書く
//!   - <型名>_factory_entry_point: 要素ごとの factory (Process の関数・Container を作る関数・ContainerProcess の関数を返す)
//!   - <型名>_factory_destroy_entry_point: Container ごとの解放の関数
//! C++ / Haskell は関数名からシンボル名を付けるが、Zig は comptime に関数名が取れないので **型名** から付ける。
//!
//! 次の間違いはコンパイルエラーにする (C++ / Haskell では読み込むときのエラー)。
//!   - 型名が Component の中で重なる
//!   - ContainerProcess の .container_type が Component の中の Container でない
//!   - ContainerProcess の self の型が、その Container の状態の型と違う
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const entry = @import("entry.zig");
const process = @import("process.zig");
const container = @import("container.zig");
const Value = value.Value;

/// Component に入れる Process。f と options は juiz.exportProcess と同じ
pub fn processPart(comptime f: anytype, comptime options: anytype) type {
    return process.ProcessEntry(f, options);
}

/// Component に入れる Container。create と options は juiz.exportContainer と同じ
pub fn containerPart(comptime create: anytype, comptime options: anytype) type {
    return container.ContainerEntry(create, options);
}

/// Component に入れる ContainerProcess。f と options は juiz.exportContainerProcess と同じ。
/// .container_type は同じ Component の Container の型名にする
pub fn containerProcessPart(comptime f: anytype, comptime options: anytype) type {
    return container.ContainerProcessEntry(f, options);
}

/// 要素をまとめて Component として公開する。`comptime { juiz.exportComponent(.{ ... }); }` の形で書く。
///
/// オプション:
///   .type_name (必須): Component の名前 (ふつうはライブラリの名前と同じにする)
///   .description: 説明
///   .parts (必須): juiz.processPart / juiz.containerPart / juiz.containerProcessPart を並べたもの (.{ ... })
pub fn exportComponent(comptime options: anytype) void {
    const Entry = ComponentEntry(options);
    @export(&Entry.componentManifestEntryPoint, .{ .name = "component_manifest_entry_point" });
    inline for (Entry.all_parts) |P| {
        const factory = comptime factoryName(P);
        switch (P.part_kind) {
            .process => @export(&P.processFactoryEntryPoint, .{ .name = factory ++ "_entry_point" }),
            .container => {
                @export(&P.containerFactoryEntryPoint, .{ .name = factory ++ "_entry_point" });
                @export(&P.destroy, .{ .name = factory ++ "_destroy_entry_point" });
            },
            .container_process => @export(&P.containerProcessFactoryEntryPoint, .{ .name = factory ++ "_entry_point" }),
        }
    }
}

/// 要素の factory のシンボル名 (juiz はこれに _entry_point を付けて探す)
fn factoryName(comptime P: type) []const u8 {
    return P.part_type_name ++ "_factory";
}

pub fn ComponentEntry(comptime options: anytype) type {
    const type_name = entry.checkOptions("exportComponent", options, &.{ "type_name", "description", "parts" });
    const what = "exportComponent: " ++ type_name;
    const description: []const u8 = entry.option(options, "description", []const u8, "");
    if (!@hasField(@TypeOf(options), "parts")) @compileError(what ++ ": .parts is required (.parts = .{ juiz.processPart(...), ... })");
    const parts = options.parts;
    const parts_info = @typeInfo(@TypeOf(parts));
    if (parts_info != .@"struct" or !parts_info.@"struct".is_tuple) {
        @compileError(what ++ ": .parts must be a tuple like .{ juiz.processPart(...), juiz.containerPart(...) }");
    }
    if (parts.len == 0) @compileError(what ++ ": .parts is empty");
    @setEvalBranchQuota(100000);

    inline for (parts, 0..) |P, i| {
        if (@TypeOf(P) != type or !@hasDecl(P, "part_kind") or @TypeOf(P.part_kind) != entry.PartKind) {
            @compileError(std.fmt.comptimePrint("{s}: .parts[{d}] is not juiz.processPart / juiz.containerPart / juiz.containerProcessPart", .{ what, i }));
        }
        // 型名はシンボル名と juiz の中の名前になるので、Component の中で重ねない
        inline for (parts, 0..) |Q, j| {
            if (j < i and std.mem.eql(u8, P.part_type_name, Q.part_type_name)) {
                @compileError(what ++ ": the type name \"" ++ P.part_type_name ++ "\" is used twice in the component");
            }
        }
    }
    // ContainerProcess の Container は同じ Component に入れ、状態の型も合わせる
    inline for (parts) |P| {
        if (P.part_kind == .container_process) {
            const C = findContainer(parts, P.part_container_type) orelse @compileError(what ++ ": the container process \"" ++
                P.part_type_name ++ "\" belongs to the container \"" ++ P.part_container_type ++ "\", which is not in the component");
            if (C.StateType != P.StateType) {
                @compileError(what ++ ": the container process \"" ++ P.part_type_name ++ "\" receives " ++ @typeName(P.SelfType) ++
                    ", but the container \"" ++ C.part_type_name ++ "\" holds " ++ @typeName(C.StateType));
            }
        }
    }

    return struct {
        pub const all_parts = parts;

        fn componentManifestEntryPoint(ptr: ?*c.capsule_ptr) callconv(.c) i64 {
            return entry.writeManifest(ptr, buildManifest);
        }

        fn buildManifest(a: std.mem.Allocator) !Value {
            var containers: std.ArrayList(Value) = .empty;
            var processes: std.ArrayList(Value) = .empty;
            inline for (parts) |P| {
                const factory = comptime factoryName(P);
                switch (P.part_kind) {
                    .process => try processes.append(a, try P.manifest(a, factory)),
                    .container => {
                        var cps: std.ArrayList(Value) = .empty;
                        inline for (parts) |Q| {
                            if (Q.part_kind == .container_process) {
                                if (comptime std.mem.eql(u8, Q.part_container_type, P.part_type_name)) {
                                    try cps.append(a, try Q.manifest(a, comptime factoryName(Q)));
                                }
                            }
                        }
                        try containers.append(a, try P.manifest(a, factory, cps.items));
                    },
                    // Container の processes に入れた
                    .container_process => {},
                }
            }
            const members = try a.dupe(Value.Member, &.{
                .{ .key = "type_name", .value = .{ .string = type_name } },
                .{ .key = "language", .value = .{ .string = "zig" } },
                .{ .key = "description", .value = .{ .string = description } },
                .{ .key = "containers", .value = .{ .array = containers.items } },
                .{ .key = "processes", .value = .{ .array = processes.items } },
            });
            return .{ .object = members };
        }
    };
}

/// parts の中の、型名が name の Container
fn findContainer(comptime parts: anytype, comptime name: []const u8) ?type {
    inline for (parts) |P| {
        if (P.part_kind == .container) {
            if (comptime std.mem.eql(u8, P.part_type_name, name)) return P;
        }
    }
    return null;
}
