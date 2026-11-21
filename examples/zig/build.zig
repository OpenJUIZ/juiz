//! juiz の Zig の例 (examples/zig) のビルド。
//!
//! zigjuiz (Step 1 以降) を使わず、C ヘッダ bindings/cjuiz/include/juiz_sdk.h を addTranslateC で
//! Zig のモジュール `c` にして、手書きの C ABI の関数でプラグインを作る (issue #572)。
//!
//!   zig build -Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリの絶対パス>
//!
//! できた共有ライブラリは zig-out/lib に置かれる。ふだんは build.sh を使う
//! (リポジトリの target/debug を渡してビルドし、../../target に写す)。
const std = @import("std");
const builtin = @import("builtin");

/// 対応する Zig の版 (docs: plans/zig-plan.md §4.6)。違う版ではビルドしない
const supported_zig = std.SemanticVersion{ .major = 0, .minor = 17, .patch = 0 };

comptime {
    const v = builtin.zig_version;
    if (v.major != supported_zig.major or v.minor != supported_zig.minor) {
        @compileError(std.fmt.comptimePrint(
            "juiz の Zig の例は Zig {d}.{d}.x 用です (この zig は {d}.{d}.{d})",
            .{ supported_zig.major, supported_zig.minor, v.major, v.minor, v.patch },
        ));
    }
}

/// 例の一覧。名前は conf の type_name と同じ (ファイル名は lib<名前>.so など)
const examples = [_][]const u8{
    "increment_process_zig",
};

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    // --release のときは ReleaseSafe (範囲外アクセスなどの安全検査を残す)。build.sh は --release を渡す
    const optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSafe });
    // rpath に入れるので絶対パスで受ける (相対パスの rpath は juiz を起動したディレクトリから探されてしまう)
    const sdk_dir = b.option([]const u8, "juiz-sdk-lib", "libjuiz_sdk のあるディレクトリ (絶対パス)") orelse {
        std.debug.print("error: -Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリの絶対パス> を指定してください (examples/zig/build.sh を使うと指定されます)\n", .{});
        std.process.exit(1);
    };
    if (!std.fs.path.isAbsolute(sdk_dir)) {
        std.debug.print("error: -Djuiz-sdk-lib は絶対パスで指定してください ({s})\n", .{sdk_dir});
        std.process.exit(1);
    }
    const sdk_lib: std.Build.LazyPath = .{ .cwd_relative = sdk_dir };

    const c = b.addTranslateC(.{
        .root_source_file = b.path("../../bindings/cjuiz/include/juiz_sdk.h"),
        .target = target,
        .optimize = optimize,
    });
    const c_module = c.createModule();

    for (examples) |name| {
        const module = b.createModule(.{
            .root_source_file = b.path(b.fmt("src/{s}.zig", .{name})),
            .target = target,
            .optimize = optimize,
            .link_libc = true,
            .imports = &.{.{ .name = "c", .module = c_module }},
        });
        module.addLibraryPath(sdk_lib);
        module.linkSystemLibrary("juiz_sdk", .{});
        // libjuiz_sdk をビルドしたディレクトリから読み込めるよう rpath を入れる (C++ / Haskell の例と同じ)
        if (target.result.os.tag != .windows) {
            module.addRPath(sdk_lib);
        }
        const lib = b.addLibrary(.{
            .name = name,
            .linkage = .dynamic,
            .root_module = module,
        });
        b.installArtifact(lib);
    }
}
