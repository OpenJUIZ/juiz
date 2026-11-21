//! zigjuiz: Zig で juiz の Process・Container・ContainerProcess を書くためのライブラリ (issue #573、#574)。利用者は `@import("juiz")` で使う。
//!
//! ```zig
//! const juiz = @import("juiz");
//!
//! fn increment(args: struct { arg1: i64 = 1 }) !i64 {
//!     return std.math.add(i64, args.arg1, 1);
//! }
//!
//! comptime {
//!     juiz.exportProcess(increment, .{
//!         .type_name = "increment_process_zig",
//!         .description = "Adds 1 to arg1.",
//!         .args = .{ .arg1 = "value to be incremented" },
//!     });
//! }
//! ```
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
const process = @import("process.zig");
const container = @import("container.zig");
const pose_ = @import("pose.zig");
const value = @import("value.zig");
const errors = @import("errors.zig");

pub const Value = value.Value;
pub const Context = process.Context;
pub const exportProcess = process.exportProcess;
pub const exportContainer = container.exportContainer;
pub const exportContainerProcess = container.exportContainerProcess;

// ContainerPose (docs/spec/transformations.md §12.3)。ContainerProcess の中から self を渡して使う
pub const Pose = pose_.Pose;
pub const PoseStamped = pose_.PoseStamped;
pub const Transform = pose_.Transform;
pub const TransformStamped = pose_.TransformStamped;
pub const Vector3 = pose_.Vector3;
pub const Quaternion = pose_.Quaternion;
pub const Time = pose_.Time;
pub const Rpy = pose_.Rpy;
pub const PoseError = pose_.PoseError;
pub const quaternion_identity = pose_.quaternion_identity;
pub const pose = pose_.pose;
pub const setPose = pose_.setPose;
pub const lookup = pose_.lookup;
pub const setTransform = pose_.setTransform;
pub const poseLastError = pose_.poseLastError;
pub const makePose = pose_.makePose;
pub const makePose2d = pose_.makePose2d;
pub const makeTransform = pose_.makeTransform;
pub const makeTransform2d = pose_.makeTransform2d;
pub const fromRpy = pose_.fromRpy;
pub const toRpy = pose_.toRpy;
pub const yaw = pose_.yaw;
pub const poseToValue = pose_.poseToValue;
pub const poseStampedToValue = pose_.poseStampedToValue;
pub const transformToValue = pose_.transformToValue;
pub const transformStampedToValue = pose_.transformStampedToValue;

/// zigjuiz が返すエラーコード (juiz_sdk.h の JUIZ_ZIG_*)
pub const error_codes = errors;

/// libjuiz_sdk の C の宣言 (bindings/cjuiz/include/juiz_sdk.h を translate-c したもの)。ふつうは使わない
pub const c = @import("c");
