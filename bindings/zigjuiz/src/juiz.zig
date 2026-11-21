//! zigjuiz: Zig で juiz の Process を書くためのライブラリ (issue #573)。利用者は `@import("juiz")` で使う。
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
const value = @import("value.zig");
const errors = @import("errors.zig");

pub const Value = value.Value;
pub const Context = process.Context;
pub const exportProcess = process.exportProcess;

/// zigjuiz が返すエラーコード (juiz_sdk.h の JUIZ_ZIG_*)
pub const error_codes = errors;

/// libjuiz_sdk の C の宣言 (bindings/cjuiz/include/juiz_sdk.h を translate-c したもの)。ふつうは使わない
pub const c = @import("c");
