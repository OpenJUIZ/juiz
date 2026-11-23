//! ContainerProcess の中から、呼び出し元 Container の位置姿勢 (ContainerPose) を読み書きする
//! (docs/spec/transformations.md §12.3、issue #574)。C++ の juiz/pose.h、Haskell の Juiz.Pose と同じことをする。
//!
//! ```zig
//! fn move(self: *Robot, args: struct { x: f64 = 0 }) !juiz.PoseStamped {
//!     const now = try juiz.pose(self, "map");            // 基準 map から見た自分
//!     var p = now.pose;
//!     p.position.x += args.x;
//!     return juiz.setPose(self, "map", p);              // 自分を動かす (readonly では書けない)
//! }
//! ```
//!
//! 第 1 引数は ContainerProcess が受け取った Container の状態へのポインタ (self)。zigjuiz は ContainerProcess を
//! 呼ぶ間、そのスレッドに「self と juiz が渡した口 (capsule_map)」の組を覚えておき、ここで探して使う。
//! readonly の ContainerProcess (self が *const State) からは、setPose / setTransform / setModel / clearModel は
//! コンパイルエラーになる。3D モデルは setModel / clearModel で差し替える (docs/spec/container-model.md §13.2、#564)。
const std = @import("std");
const c = @import("c");
const value = @import("value.zig");
const Value = value.Value;

/// 位置 (position、m) と向き (orientation、四元数 x, y, z, w)。juiz_std.geometry.Pose
pub const Pose = c.juiz_pose;
/// 時刻つきの Pose。基準は渡した reference
pub const PoseStamped = c.juiz_pose_stamped;
/// 平行移動 (translation) と回転 (rotation)。juiz_std.geometry.Transform
pub const Transform = c.juiz_transform;
/// 時刻つきの Transform。端点は渡した source / destination
pub const TransformStamped = c.juiz_transform_stamped;
pub const Vector3 = c.juiz_vector3;
pub const Quaternion = c.juiz_quaternion;
pub const Time = c.juiz_time;

/// 回転なしの四元数
pub const quaternion_identity: Quaternion = .{ .x = 0, .y = 0, .z = 0, .w = 1 };

/// ContainerPose の読み書きのエラー。juiz の理由の文は poseLastError() で読める
/// (関数がこのエラーをそのまま返すと、zigjuiz はエラー文に理由も入れる)
pub const PoseError = error{
    /// Container が System の Transformations につながっていない。ContainerProcess の外から呼んだときも (-901)
    TransformationsNotAttached,
    /// 基準と Container の両方を含む Anchor がない (-902)
    NoCommonAnchor,
    /// 根の Anchor・基準自身・基準が子孫の Container への書き込み (-903)
    PoseNotWritable,
    /// 長さ 0 の四元数、NaN・無限大 (-904)
    InvalidPoseValue,
    /// そのほかの失敗 (-905)
    TransformationsError,
    /// 呼び出し元 Container が端点でも writers でもない Transform への書き込み (-906)
    TransformNotWritable,
    /// Transform が無い (-907)
    TransformNotFound,
    /// setModel の値の誤り (形式に合わない・上限超えなど)、readonly の ContainerProcess からの差し替え (-908)
    ContainerModelInvalid,
    /// 名前が長すぎて NUL 終端の文字列にできない (メモリ不足)
    OutOfMemory,
};

/// ContainerPose の読み書きのエラーなら true (エラー文に理由を足すため)
pub fn isPoseError(e: anyerror) bool {
    const name = @errorName(e);
    if (std.mem.eql(u8, name, "OutOfMemory")) return false;
    inline for (@typeInfo(PoseError).error_set.error_names.?) |err_name| {
        if (std.mem.eql(u8, name, err_name)) return true;
    }
    return false;
}

/// このスレッドで最後に失敗した ContainerPose の読み書きの理由 (成功なら空文字列)。次に読み書きするまで有効
pub fn poseLastError() []const u8 {
    return std.mem.span(c.juiz_container_pose_last_error());
}

// ---- 呼び出し中の ContainerProcess の口 ----

/// 呼び出し中の ContainerProcess の、Container の状態と juiz が渡した口の組 (スレッドごとの連結リスト)。
/// ContainerProcess の中から別の Process を呼んで、さらに ContainerProcess が呼ばれることもあるので、積み重ねる
pub const PoseCall = struct {
    container: *const anyopaque,
    cm: ?*c.capsule_map,
    prev: ?*PoseCall,
};

threadlocal var pose_calls: ?*PoseCall = null;

/// ContainerProcess の entry point が、関数を呼ぶ前に call を積む。呼んだ後に pop で外す
pub fn push(call: *PoseCall) void {
    call.prev = pose_calls;
    pose_calls = call;
}

pub fn pop(call: *PoseCall) void {
    pose_calls = call.prev;
}

fn portOf(container: *const anyopaque) PoseError!?*c.capsule_map {
    var p = pose_calls;
    while (p) |call| : (p = call.prev) {
        if (call.container == container) return call.cm;
    }
    return error.TransformationsNotAttached;
}

fn containerPtr(comptime what: []const u8, self: anytype, comptime writes: bool) *const anyopaque {
    const T = @TypeOf(self);
    const info = switch (@typeInfo(T)) {
        .pointer => |p| p,
        else => @compileError(what ++ ": the first argument must be the container state the container process received (self), not " ++ @typeName(T)),
    };
    if (writes and info.attrs.@"const") @compileError(what ++ " can not be called from a readonly container process (the container state is *const)");
    return @ptrCast(self);
}

fn check(r: i64) PoseError!void {
    return switch (r) {
        c.JUIZ_OK => {},
        c.JUIZ_TRANSFORMATIONS_NOT_ATTACHED_ERROR => error.TransformationsNotAttached,
        c.JUIZ_NO_COMMON_ANCHOR_ERROR => error.NoCommonAnchor,
        c.JUIZ_POSE_NOT_WRITABLE_ERROR => error.PoseNotWritable,
        c.JUIZ_INVALID_POSE_VALUE_ERROR => error.InvalidPoseValue,
        c.JUIZ_TRANSFORM_NOT_WRITABLE_ERROR => error.TransformNotWritable,
        c.JUIZ_TRANSFORM_NOT_FOUND_ERROR => error.TransformNotFound,
        c.JUIZ_CONTAINER_MODEL_INVALID_ERROR => error.ContainerModelInvalid,
        else => error.TransformationsError,
    };
}

/// 名前を NUL 終端の文字列にして f(名前) を呼ぶ
fn withZ(buf: []u8, s: []const u8) PoseError![*:0]const u8 {
    if (s.len + 1 > buf.len) return error.OutOfMemory;
    @memcpy(buf[0..s.len], s);
    buf[s.len] = 0;
    return @ptrCast(buf.ptr);
}

const name_max = 1024;

/// 基準 reference (Container の名前か ID) から見た、self の Container の ContainerPose (§7.1)
pub fn pose(self: anytype, reference: []const u8) PoseError!PoseStamped {
    const cm = try portOf(containerPtr("juiz.pose", self, false));
    var buf: [name_max]u8 = undefined;
    var out: PoseStamped = std.mem.zeroes(PoseStamped);
    try check(c.juiz_container_pose_get(cm, try withZ(&buf, reference), &out));
    return out;
}

/// self の Container を、基準 reference から見て p の位置姿勢にする (§7.2)。書いた後の値を返す。
/// readonly の ContainerProcess (self が *const State) からは呼べない (コンパイルエラー)
pub fn setPose(self: anytype, reference: []const u8, p: Pose) PoseError!PoseStamped {
    const cm = try portOf(containerPtr("juiz.setPose", self, true));
    var buf: [name_max]u8 = undefined;
    var out: PoseStamped = std.mem.zeroes(PoseStamped);
    try check(c.juiz_container_pose_set(cm, try withZ(&buf, reference), &p, &out));
    return out;
}

/// self 以外の Container target (名前か ID) の、基準 reference から見た ContainerPose (§7.4、tf の lookupTransform に当たる)。
/// 名前は self の Container のネームスペースで解決する。読み出しだけなので readonly の ContainerProcess からも呼べる
pub fn lookup(self: anytype, target: []const u8, reference: []const u8) PoseError!PoseStamped {
    const cm = try portOf(containerPtr("juiz.lookup", self, false));
    var tbuf: [name_max]u8 = undefined;
    var rbuf: [name_max]u8 = undefined;
    var out: PoseStamped = std.mem.zeroes(PoseStamped);
    try check(c.juiz_container_pose_lookup(cm, try withZ(&tbuf, target), try withZ(&rbuf, reference), &out));
    return out;
}

/// self の Container から、Transform (source → destination、名前か ID) の値を t にする (§7.5)。書いた後の値を返す。
/// 書けるのは self の Container が端点か、conf の transforms: の writers に入っている Transform だけ (error.TransformNotWritable)。
/// readonly の ContainerProcess からは呼べない (コンパイルエラー)
pub fn setTransform(self: anytype, source: []const u8, destination: []const u8, t: Transform) PoseError!TransformStamped {
    const cm = try portOf(containerPtr("juiz.setTransform", self, true));
    var sbuf: [name_max]u8 = undefined;
    var dbuf: [name_max]u8 = undefined;
    var out: TransformStamped = std.mem.zeroes(TransformStamped);
    try check(c.juiz_container_transform_set(cm, try withZ(&sbuf, source), try withZ(&dbuf, destination), &t, &out));
    return out;
}

/// setModel の options を libjuiz_sdk の空の辞書に書く (juiz_container_model_set の options)
const ModelOptions = struct {
    allocator: std.mem.Allocator,
    options: Value,

    fn fill(arg: ?*anyopaque, obj: ?*c.value) callconv(.c) i64 {
        const self: *ModelOptions = @ptrCast(@alignCast(arg.?));
        value.fillCollection(self.allocator, obj, self.options) catch return c.JUIZ_CONTAINER_MODEL_INVALID_ERROR;
        return c.JUIZ_OK;
    }
};

/// setModel のモデル情報を juiz.Value に写す (juiz_container_model_set の info)
const ModelInfo = struct {
    allocator: std.mem.Allocator,
    result: ?Value = null,
    err: ?error{ OutOfMemory, UnsupportedValue } = null,

    fn take(arg: ?*anyopaque, v: ?*c.value) callconv(.c) void {
        const self: *ModelInfo = @ptrCast(@alignCast(arg.?));
        self.result = value.read(self.allocator, v) catch |e| {
            self.err = e;
            return;
        };
    }
};

/// self の Container の 3D モデルを data の中身に差し替え、差し替えた後のモデル情報
/// (juiz_std.model.ContainerModelInfo の辞書、allocator に作る) を返す (docs/spec/container-model.md §13.2、#564)。
/// format は "glb"・"gltf"・"stl"・"sdf"・"other" (空なら options の file_name の拡張子から)。
/// options は scale・origin・up_axis・color・file_name の辞書 (.null なら無し)。
/// 値の誤り (形式に合わない・上限超えなど) は error.ContainerModelInvalid (今のモデルは変わらない)。
/// readonly の ContainerProcess からは呼べない (コンパイルエラー)
pub fn setModel(self: anytype, allocator: std.mem.Allocator, data: []const u8, format: []const u8, options: Value) PoseError!Value {
    const cm = try portOf(containerPtr("juiz.setModel", self, true));
    switch (options) {
        .null, .object => {},
        else => return error.ContainerModelInvalid,
    }
    var fbuf: [64]u8 = undefined;
    const fmt: ?[*:0]const u8 = if (format.len == 0) null else try withZ(&fbuf, format);
    var opts = ModelOptions{ .allocator = allocator, .options = options };
    var info = ModelInfo{ .allocator = allocator };
    try check(c.juiz_container_model_set(
        cm,
        data.ptr,
        data.len,
        fmt,
        if (options == .object) ModelOptions.fill else null,
        &opts,
        ModelInfo.take,
        &info,
    ));
    if (info.err) |e| return switch (e) {
        error.OutOfMemory => error.OutOfMemory,
        error.UnsupportedValue => error.TransformationsError,
    };
    return info.result orelse .null;
}

/// self の Container の 3D モデルを外す (§13.2)。conf や型の既定のモデルにも戻らない。
/// readonly の ContainerProcess からは呼べない (コンパイルエラー)
pub fn clearModel(self: anytype) PoseError!void {
    const cm = try portOf(containerPtr("juiz.clearModel", self, true));
    try check(c.juiz_container_model_clear(cm));
}

// ---- 値を作る・変える ----

/// 位置 (x, y, z) と四元数 q から Pose を作る
pub fn makePose(x: f64, y: f64, z: f64, q: Quaternion) Pose {
    return .{ .position = .{ .x = x, .y = y, .z = z }, .orientation = q };
}

/// 平面 (z = 0) の位置 (x, y) と z 軸まわりの向き yaw [rad] から Pose を作る
pub fn makePose2d(x: f64, y: f64, yaw_rad: f64) Pose {
    return makePose(x, y, 0, fromRpy(0, 0, yaw_rad));
}

/// 平行移動 (x, y, z) と四元数 q から Transform を作る
pub fn makeTransform(x: f64, y: f64, z: f64, q: Quaternion) Transform {
    return .{ .translation = .{ .x = x, .y = y, .z = z }, .rotation = q };
}

/// 平面 (z = 0) の平行移動 (x, y) と z 軸まわりの回転 yaw [rad] から Transform を作る
pub fn makeTransform2d(x: f64, y: f64, yaw_rad: f64) Transform {
    return makeTransform(x, y, 0, fromRpy(0, 0, yaw_rad));
}

/// roll・pitch・yaw [rad]
pub const Rpy = struct { roll: f64, pitch: f64, yaw: f64 };

/// roll・pitch・yaw [rad] から四元数を作る。規則は Rust の juiz_sdk::geometry::Quaternion::from_rpy と同じ
/// (ROS と同じ、固定軸 X→Y→Z の順に回す、q = q_z ⊗ q_y ⊗ q_x)
pub fn fromRpy(roll: f64, pitch: f64, yaw_rad: f64) Quaternion {
    const sr = @sin(roll / 2);
    const cr = @cos(roll / 2);
    const sp = @sin(pitch / 2);
    const cp = @cos(pitch / 2);
    const sy = @sin(yaw_rad / 2);
    const cy = @cos(yaw_rad / 2);
    return .{
        .x = sr * cp * cy - cr * sp * sy,
        .y = cr * sp * cy + sr * cp * sy,
        .z = cr * cp * sy - sr * sp * cy,
        .w = cr * cp * cy + sr * sp * sy,
    };
}

/// fromRpy の逆。長さ 1 でない四元数は正規化してから変換する。
/// 長さ 0 (1e-12 未満)・NaN・無限大の四元数は回転なしとみなして (0, 0, 0) を返す (Rust と同じ)
pub fn toRpy(q0: Quaternion) Rpy {
    const n = @sqrt(q0.x * q0.x + q0.y * q0.y + q0.z * q0.z + q0.w * q0.w);
    if (!std.math.isFinite(n) or n < 1e-12) return .{ .roll = 0, .pitch = 0, .yaw = 0 };
    const x = q0.x / n;
    const y = q0.y / n;
    const z = q0.z / n;
    const w = q0.w / n;
    const sinp = std.math.clamp(2 * (w * y - z * x), -1.0, 1.0);
    return .{
        .roll = std.math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y)),
        .pitch = std.math.asin(sinp),
        .yaw = std.math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)),
    };
}

/// z 軸まわりの回転角 [rad] (toRpy の yaw)。平面の移動ロボットの向き
pub fn yaw(q: Quaternion) f64 {
    return toRpy(q).yaw;
}

// ---- juiz.Value への変換 (戻り値にするとき) ----

fn vector3ToValue(a: std.mem.Allocator, v: Vector3) !Value {
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "x", .value = .{ .float = v.x } },
        .{ .key = "y", .value = .{ .float = v.y } },
        .{ .key = "z", .value = .{ .float = v.z } },
    }) };
}

fn quaternionToValue(a: std.mem.Allocator, q: Quaternion) !Value {
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "x", .value = .{ .float = q.x } },
        .{ .key = "y", .value = .{ .float = q.y } },
        .{ .key = "z", .value = .{ .float = q.z } },
        .{ .key = "w", .value = .{ .float = q.w } },
    }) };
}

fn headerToValue(a: std.mem.Allocator, stamp: Time, frame_id: []const u8) !Value {
    const s = try a.dupe(Value.Member, &.{
        .{ .key = "sec", .value = .{ .int = stamp.sec } },
        .{ .key = "nanosec", .value = .{ .int = stamp.nanosec } },
    });
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "stamp", .value = .{ .object = s } },
        .{ .key = "frame_id", .value = .{ .string = frame_id } },
    }) };
}

/// juiz_std.geometry.Pose の辞書にする。a は ctx.allocator など
pub fn poseToValue(a: std.mem.Allocator, p: Pose) !Value {
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "position", .value = try vector3ToValue(a, p.position) },
        .{ .key = "orientation", .value = try quaternionToValue(a, p.orientation) },
    }) };
}

/// juiz_std.geometry.PoseStamped の辞書にする (header.frame_id は reference)
pub fn poseStampedToValue(a: std.mem.Allocator, p: PoseStamped, reference: []const u8) !Value {
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "header", .value = try headerToValue(a, p.stamp, reference) },
        .{ .key = "pose", .value = try poseToValue(a, p.pose) },
    }) };
}

/// juiz_std.geometry.Transform の辞書にする
pub fn transformToValue(a: std.mem.Allocator, t: Transform) !Value {
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "translation", .value = try vector3ToValue(a, t.translation) },
        .{ .key = "rotation", .value = try quaternionToValue(a, t.rotation) },
    }) };
}

/// juiz_std.geometry.TransformStamped の辞書にする (header.frame_id は source、child_frame_id は destination)
pub fn transformStampedToValue(a: std.mem.Allocator, t: TransformStamped, source: []const u8, destination: []const u8) !Value {
    return .{ .object = try a.dupe(Value.Member, &.{
        .{ .key = "header", .value = try headerToValue(a, t.stamp, source) },
        .{ .key = "child_frame_id", .value = .{ .string = destination } },
        .{ .key = "transform", .value = try transformToValue(a, t.transform) },
    }) };
}
