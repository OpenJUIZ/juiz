//! 型付きの引数・戻り値の例とテスト用 Component (typed_component_zig、issue #576、docs/spec/datatype.md §26)。
//!
//! 型は juiz-idl -g zig が juiz_idl/test_package/idl/geometry.jidl から作ったもの (build.zig の juiz.addTypes が作り、
//! juiz_types という名前で読み込む) と、juiz_type を手で書いた Odometry を使う。
//! Haskell の typed_component_hs・C++ の cpp_typed_component と同じ構成 (テストは juiz_core/tests/integration/datatype/zig.rs)。
const std = @import("std");
const juiz = @import("juiz");
const types = @import("juiz_types");

const geometry = types.robot.geometry;
const Pose2D = geometry.Pose2D;
const Everything = geometry.Everything;
const MotorMode = geometry.MotorMode;

/// juiz_type を手で書いた型 (IDL を使わないとき。C++ の JUIZ_TYPE、Haskell の unsafeTypeInfo に当たる)
const Odometry = struct {
    distance: f64,
    steps: i64 = 0,

    pub const juiz_type: juiz.TypeInfo = .{
        .name = "handwritten.Odometry",
        .avro =
        \\{"type": "record", "name": "Odometry", "namespace": "handwritten", "fields": [
        \\  {"name": "distance", "type": "double"}, {"name": "steps", "type": "long", "default": 0}]}
        ,
    };
};

const origin: Pose2D = .{ .position = .{ .x = 0, .y = 0 } };

/// 結果が型付きの型なら output_schema が載る
fn poseMake(args: struct { x: f64 = 0 }) Pose2D {
    return .{ .position = .{ .x = args.x, .y = 0 } };
}

fn poseMove(args: struct { pose: Pose2D = origin, dx: f64 = 1 }) Pose2D {
    var p = args.pose;
    p.position.x += args.dx;
    return p;
}

/// 既定値の無い型付きの引数 (pose) と、既定値のある型付きの引数 (offset)
fn poseX(args: struct { pose: Pose2D, offset: Pose2D = origin }) f64 {
    return args.pose.position.x + args.offset.position.x;
}

/// Pose2D の frame と comment (既定値のあるフィールド) を文字列で返す
fn poseFrame(ctx: juiz.Context, args: struct { pose: Pose2D }) ![]const u8 {
    return std.fmt.allocPrint(ctx.allocator, "{s}/{s}", .{ args.pose.frame, args.pose.comment orelse "none" });
}

/// いろいろな型のフィールドを Zig の値にして、少し変えて返す (C++ の cpp_everything_echo・Haskell の hs_everything_echo と同じ変え方)
fn everythingEcho(ctx: juiz.Context, args: struct { value: Everything }) !Everything {
    const a = ctx.allocator;
    var e = args.value;
    e.i += 1;
    e.l += 1;
    e.data = .{ .data = try std.mem.concat(a, u8, &.{ e.data.data, &.{255} }) };
    e.list = try std.mem.concat(a, i64, &.{ e.list, &.{@as(i64, @intCast(e.list.len))} });
    e.table = .{ .entries = try std.mem.concat(a, juiz.Map(f64).Entry, &.{ e.table.entries, &.{.{ .key = "b", .value = 2.5 }} }) };
    e.maybe = null;
    e.mode = .position;
    e.header.frame_id = try std.mem.concat(a, u8, &.{ e.header.frame_id, "!" });
    e.type = try std.mem.concat(a, u8, &.{ e.type, "?" });
    return e;
}

/// 列挙の引数と戻り値 (辞書では記号の文字列)
fn modeNext(args: struct { mode: MotorMode = .idle }) MotorMode {
    return switch (args.mode) {
        .idle => .velocity,
        .velocity => .position,
        .position => .idle,
    };
}

fn odometryStep(args: struct { odometry: Odometry = .{ .distance = 0 } }) Odometry {
    return .{ .distance = args.odometry.distance + 1, .steps = args.odometry.steps + 1 };
}

/// 型付きの引数を持つ Container。受け取った Pose2D の x を覚える
/// (Pose2D の文字列は呼び出しの arena のものなので、取っておくなら複製する。ここでは数だけ持つ)
const Holder = struct {
    last_x: f64,
};

fn createHolder(args: struct { initial: Pose2D = origin }) Holder {
    return .{ .last_x = args.initial.position.x };
}

/// 受け取った Pose2D を覚え、x を scale 倍して返す
fn holderScale(self: *Holder, args: struct { pose: Pose2D, scale: f64 = 2 }) Pose2D {
    self.last_x = args.pose.position.x;
    var p = args.pose;
    p.position.x *= args.scale;
    return p;
}

fn holderLast(self: *const Holder) f64 {
    return self.last_x;
}

comptime {
    juiz.exportComponent(.{
        .type_name = "typed_component_zig",
        .description = "Zig typed component (test).",
        .parts = .{
            juiz.processPart(poseMake, .{ .type_name = "zig_pose_make", .args = .{ .x = "" } }),
            juiz.processPart(poseMove, .{ .type_name = "zig_pose_move", .args = .{ .pose = "", .dx = "" } }),
            juiz.processPart(poseX, .{ .type_name = "zig_pose_x", .args = .{ .pose = "", .offset = "" } }),
            juiz.processPart(poseFrame, .{ .type_name = "zig_pose_frame", .args = .{ .pose = "" } }),
            juiz.processPart(everythingEcho, .{ .type_name = "zig_everything_echo", .args = .{ .value = "" } }),
            juiz.processPart(modeNext, .{ .type_name = "zig_mode_next", .args = .{ .mode = "" } }),
            juiz.processPart(odometryStep, .{ .type_name = "zig_odometry_step", .args = .{ .odometry = "" } }),
            juiz.containerPart(createHolder, .{ .type_name = "zig_pose_holder", .args = .{ .initial = "" } }),
            juiz.containerProcessPart(holderScale, .{ .type_name = "zig_pose_holder_scale", .container_type = "zig_pose_holder", .args = .{ .pose = "", .scale = "" } }),
            juiz.containerProcessPart(holderLast, .{ .type_name = "zig_pose_holder_last", .container_type = "zig_pose_holder" }),
        },
    });
}
