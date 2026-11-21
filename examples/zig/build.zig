//! juiz の Zig の例 (examples/zig) のビルド (issue #573)。zigjuiz (bindings/zigjuiz) を使う。
//!
//!   zig build --release -Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリの絶対パス>
//!
//! できた共有ライブラリは zig-out/lib に置かれる。ふだんは build.sh を使う
//! (リポジトリの target/debug を渡してビルドし、../../target に写す)。
const std = @import("std");
const zigjuiz = @import("zigjuiz");

/// 例の一覧。名前は conf の type_name と同じ (ファイル名は lib<名前>.so など)
const examples = [_][]const u8{
    "increment_process_zig",
    "greet_process_zig",
    "echo_types_process_zig",
    "fail_process_zig",
};

pub fn build(b: *std.Build) void {
    const juiz = zigjuiz.init(b, .{
        .target = b.standardTargetOptions(.{}),
        // --release のときは ReleaseSafe (範囲外アクセスなどの安全検査を残す)。build.sh は --release を渡す
        .optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSafe }),
    });
    for (examples) |name| {
        const lib = juiz.addPlugin(.{
            .name = name,
            .root_source_file = b.path(b.fmt("src/{s}.zig", .{name})),
        });
        b.installArtifact(lib);
    }
}
