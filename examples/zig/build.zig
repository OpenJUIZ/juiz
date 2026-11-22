//! juiz の Zig の例 (examples/zig) のビルド (issue #573、#574、#575、#576)。zigjuiz (bindings/zigjuiz) を使う。
//!
//!   zig build --release -Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリの絶対パス> -Djuiz-idl=<juiz-idl のパス>
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
    // Container と ContainerProcess (issue #574)。状態の型は example_container_state.zig
    "example_container_zig",
    "example_container_zig_increment",
    "example_container_zig_get",
    "mismatched_container_process_zig",
    // ContainerPose を読み書きする例 (issue #574)。状態の型は zig_part_state.zig
    "zig_part",
    "zig_part_pose",
    "zig_part_set_pose",
    "zig_part_try",
    "zig_part_peek",
    "zig_part_sleep",
    "zig_part_lookup",
    "zig_part_set_transform",
    // Process・Container・ContainerProcess を 1 つのライブラリにまとめた Component (issue #575)
    "example_component_zig",
    // 画像の引数・戻り値 (issue #576)
    "image_component_zig",
};

/// 型付きの引数の例 (issue #576)。juiz-idl -g zig で作った型 (juiz_types) を読み込む
const typed_examples = [_][]const u8{
    "typed_component_zig",
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
    // juiz_idl/test_package/idl/geometry.jidl の型 (common.jidl・stamp.avsc を import する)
    const idl_dir = "../../juiz_idl/test_package/idl";
    const geometry = juiz.addTypes(.{
        .files = &.{b.path(idl_dir ++ "/geometry.jidl")},
        .depends = &.{ b.path(idl_dir ++ "/common.jidl"), b.path(idl_dir ++ "/stamp.avsc") },
    });
    for (typed_examples) |name| {
        const lib = juiz.addPlugin(.{
            .name = name,
            .root_source_file = b.path(b.fmt("src/{s}.zig", .{name})),
            .imports = &.{.{ .name = "juiz_types", .module = geometry }},
        });
        b.installArtifact(lib);
    }
}
