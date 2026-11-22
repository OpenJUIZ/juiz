//! zigjuiz の comptime の検査 (exportProcess のコンパイルエラー) を確かめるためのビルド。
//! check.sh が src/case.zig を書いてはビルドし、エラー文を確かめる (issue #573)。
//! -Dtypes-idl=<.jidl の絶対パス> を渡すと、juiz-idl -g zig でその型を作り juiz_types として読み込ませる
//! (juiz-idl -g zig-skeleton の雛形がビルドできるかを確かめる、issue #576)。
const std = @import("std");
const zigjuiz = @import("zigjuiz");

pub fn build(b: *std.Build) void {
    const types_idl = b.option([]const u8, "types-idl", "JIDL file whose types are imported as juiz_types (absolute path)");
    const juiz = zigjuiz.init(b, .{ .target = b.standardTargetOptions(.{}), .optimize = .Debug });
    const imports: []const std.Build.Module.Import = if (types_idl) |idl|
        &.{.{ .name = "juiz_types", .module = juiz.addTypes(.{ .files = &.{.{ .cwd_relative = idl }} }) }}
    else
        &.{};
    b.installArtifact(juiz.addPlugin(.{ .name = "case", .root_source_file = b.path("src/case.zig"), .imports = imports }));
}
