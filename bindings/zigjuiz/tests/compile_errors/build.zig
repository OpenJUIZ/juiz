//! zigjuiz の comptime の検査 (exportProcess のコンパイルエラー) を確かめるためのビルド。
//! check.sh が src/case.zig を書いてはビルドし、エラー文を確かめる (issue #573)。
const std = @import("std");
const zigjuiz = @import("zigjuiz");

pub fn build(b: *std.Build) void {
    const juiz = zigjuiz.init(b, .{ .target = b.standardTargetOptions(.{}), .optimize = .Debug });
    b.installArtifact(juiz.addPlugin(.{ .name = "case", .root_source_file = b.path("src/case.zig") }));
}
