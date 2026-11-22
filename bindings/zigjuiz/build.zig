//! zigjuiz のビルド (issue #573)。
//!
//! zigjuiz は、利用者の build.zig から依存 (build.zig.zon の dependencies) として使う。
//! 利用者の build.zig は次のように書く (README.md)。
//!
//! ```zig
//! const std = @import("std");
//! const zigjuiz = @import("zigjuiz");
//!
//! pub fn build(b: *std.Build) void {
//!     const juiz = zigjuiz.init(b, .{
//!         .target = b.standardTargetOptions(.{}),
//!         .optimize = b.standardOptimizeOption(.{ .preferred_optimize_mode = .ReleaseSafe }),
//!     });
//!     const lib = juiz.addPlugin(.{ .name = "increment_process_zig", .root_source_file = b.path("src/increment.zig") });
//!     b.installArtifact(lib);
//! }
//! ```
//!
//! libjuiz_sdk は次の順で探す (見つからなければ `juiz sdk install` を案内してビルドを失敗させる)。
//!   1. -Djuiz-sdk-lib=<libjuiz_sdk のあるディレクトリ>
//!   2. -Djuiz-sdk=<libjuiz_sdk のアーカイブを展開したディレクトリ> の lib
//!   3. 環境変数 JUIZ_SDK_HOME の <版>/lib (juiz sdk install の置き場所)
//!   4. `juiz sdk path --version <版>` の lib
//! <版> は zigjuiz の版 (build.zig.zon の version、juiz と同じ)。
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
const std = @import("std");
const builtin = @import("builtin");

const zon = @import("build.zig.zon");

/// zigjuiz の版。juiz・libjuiz_sdk と同じ版を使う
pub const version: []const u8 = zon.version;

/// zigjuiz が対応する Zig の版 (plans/zig-plan.md §4.6)。違う版ではビルドしない
pub const supported_zig = std.SemanticVersion{ .major = 0, .minor = 17, .patch = 0 };

comptime {
    const v = builtin.zig_version;
    if (v.major != supported_zig.major or v.minor != supported_zig.minor) {
        @compileError(std.fmt.comptimePrint(
            "zigjuiz {s} requires Zig {d}.{d}.x (this zig is {d}.{d}.{d})",
            .{ version, supported_zig.major, supported_zig.minor, v.major, v.minor, v.patch },
        ));
    }
}

/// C ヘッダ juiz_sdk.h の場所 (zigjuiz のディレクトリから)。リポジトリでは bindings/cjuiz/include/ を指す。
/// 配布するアーカイブ (issue #577) では、作るときにヘッダを include/ に写してここを書き換える
const header_path = "../cjuiz/include/juiz_sdk.h";

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const header = b.path(header_path);
    const c = b.addTranslateC(.{
        .root_source_file = header,
        .target = target,
        .optimize = optimize,
    });
    _ = b.addModule("juiz", .{
        .root_source_file = b.path("src/juiz.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{.{ .name = "c", .module = c.createModule() }},
    });
}

pub const InitOptions = struct {
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    /// 依存の名前 (利用者の build.zig.zon の dependencies に書いた名前)。zig fetch --save なら "zigjuiz"
    dependency_name: []const u8 = "zigjuiz",
};

pub const PluginOptions = struct {
    /// ライブラリの名前 (lib<名前>.so など)。conf の plugins の型名と同じにする
    name: []const u8,
    root_source_file: std.Build.LazyPath,
    /// プラグインのソースが読み込むモジュール (addTypes の型など)。juiz は常に読み込める
    imports: []const std.Build.Module.Import = &.{},
};

pub const TypesOptions = struct {
    /// 読む .jidl / .avsc (import した先は juiz-idl が読む)
    files: []const std.Build.LazyPath,
    /// import の探し先 (juiz-idl の -I)
    import_dirs: []const std.Build.LazyPath = &.{},
    /// import した先のファイル。変わったときに作り直すために書く (CMake の juiz_generate_types の DEPENDS と同じ)
    depends: []const std.Build.LazyPath = &.{},
};

/// 利用者の build.zig から使う、juiz のプラグインを作る道具
pub const Juiz = struct {
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    /// zigjuiz のモジュール (@import("juiz"))
    module: *std.Build.Module,
    /// libjuiz_sdk のあるディレクトリ (絶対パス)。見つからなければ null
    sdk_lib_dir: ?[]const u8,
    /// libjuiz_sdk が見つからないときに失敗させるステップ
    missing_sdk: ?*std.Build.Step,
    /// juiz-idl のコマンド (-Djuiz-idl、無ければ PATH の juiz-idl)
    idl_exe: []const u8,

    /// .jidl / .avsc から `juiz-idl -g zig` で型を作り (ビルドのたびではなく、入力が変わったときだけ)、
    /// 全部の名前空間を読み込む juiz_idl.zig を根にしたモジュールを返す (issue #576、docs/spec/datatype.md §26.3)。
    /// addPlugin の .imports に `.{ .name = "juiz_types", .module = types }` のように渡し、`@import("juiz_types")` で使う
    pub fn addTypes(self: Juiz, options: TypesOptions) *std.Build.Module {
        const b = self.b;
        const run = b.addSystemCommand(&.{ self.idl_exe, "-g", "zig" });
        const out = run.addPrefixedOutputDirectoryArg("--out-dir=", "juiz_types");
        for (options.import_dirs) |d| run.addPrefixedDirectoryArg("--import-dir=", d);
        for (options.files) |f| run.addFileArg(f);
        for (options.depends) |f| run.addFileInput(f);
        return b.createModule(.{
            .root_source_file = out.path(b, "juiz_idl.zig"),
            .target = self.target,
            .optimize = self.optimize,
            .imports = &.{.{ .name = "juiz", .module = self.module }},
        });
    }

    /// Zig のファイル 1 つから、juiz のプラグイン (共有ライブラリ) を作る。
    /// rpath は $ORIGIN (macOS は @loader_path) と libjuiz_sdk のディレクトリ
    pub fn addPlugin(self: Juiz, options: PluginOptions) *std.Build.Step.Compile {
        const b = self.b;
        const module = b.createModule(.{
            .root_source_file = options.root_source_file,
            .target = self.target,
            .optimize = self.optimize,
            .link_libc = true,
            .imports = &.{.{ .name = "juiz", .module = self.module }},
        });
        for (options.imports) |i| module.addImport(i.name, i.module);
        const lib = b.addLibrary(.{
            .name = options.name,
            .linkage = .dynamic,
            .root_module = module,
        });
        if (self.sdk_lib_dir) |dir| {
            const path: std.Build.LazyPath = .{ .cwd_relative = dir };
            module.addLibraryPath(path);
            module.linkSystemLibrary("juiz_sdk", .{});
            switch (self.target.result.os.tag) {
                // Windows は DLL を隣か PATH から探すので rpath は無い (Windows の確認は issue #578)
                .windows => {},
                .macos, .ios => {
                    module.addRPathSpecial("@loader_path");
                    module.addRPath(path);
                },
                else => {
                    module.addRPathSpecial("$ORIGIN");
                    module.addRPath(path);
                },
            }
        }
        if (self.missing_sdk) |step| lib.step.dependOn(step);
        return lib;
    }
};

/// 利用者の build.zig で一度だけ呼ぶ。-Djuiz-sdk / -Djuiz-sdk-lib のオプションを b に足し、libjuiz_sdk を探す
pub fn init(b: *std.Build, options: InitOptions) Juiz {
    const sdk_lib_opt = b.option([]const u8, "juiz-sdk-lib", "Directory that contains libjuiz_sdk (absolute path)");
    const sdk_opt = b.option([]const u8, "juiz-sdk", "Directory of the extracted libjuiz_sdk archive (default: JUIZ_SDK_HOME/<version> or `juiz sdk path`)");
    const idl_exe = b.option([]const u8, "juiz-idl", "juiz-idl command used by addTypes (default: juiz-idl in PATH)") orelse "juiz-idl";
    const dep = b.dependency(options.dependency_name, .{
        .target = options.target,
        .optimize = options.optimize,
    });

    var missing: ?*std.Build.Step = null;
    const sdk_lib_dir = findSdkLib(b, sdk_lib_opt, sdk_opt) catch |e| blk: {
        const msg = switch (e) {
            error.NotAbsolute => b.fmt("-Djuiz-sdk-lib / -Djuiz-sdk must be an absolute path", .{}),
            error.NotFound => b.fmt(
                \\libjuiz_sdk {s} was not found.
                \\  Install it with `juiz sdk install --version {s}` (it goes to ~/.juiz/sdk/{s}),
                \\  or pass -Djuiz-sdk=<directory of the extracted juiz_sdk-{s}-<target> archive>.
            , .{ version, version, version, version }),
        };
        missing = &b.addFail(msg).step;
        break :blk null;
    };

    return .{
        .b = b,
        .target = options.target,
        .optimize = options.optimize,
        .module = dep.module("juiz"),
        .sdk_lib_dir = sdk_lib_dir,
        .missing_sdk = missing,
        .idl_exe = idl_exe,
    };
}

fn findSdkLib(b: *std.Build, sdk_lib_opt: ?[]const u8, sdk_opt: ?[]const u8) error{ NotFound, NotAbsolute }![]const u8 {
    // rpath に入れるので絶対パスで受ける (相対パスの rpath は juiz を起動したディレクトリから探されてしまう)
    if (sdk_lib_opt) |dir| {
        if (!std.fs.path.isAbsolute(dir)) return error.NotAbsolute;
        return dir;
    }
    if (sdk_opt) |dir| {
        if (!std.fs.path.isAbsolute(dir)) return error.NotAbsolute;
        return b.pathJoin(&.{ dir, "lib" });
    }
    // ここから先は環境変数・ファイル・juiz コマンドを見るので、ビルドの設定のキャッシュを使わない
    b.graph.poisonCache();
    if (b.graph.environ_map.get("JUIZ_SDK_HOME")) |home| {
        const dir = b.pathJoin(&.{ home, version, "lib" });
        if (std.Io.Dir.accessAbsolute(b.graph.io, dir, .{})) |_| return dir else |_| {}
    }
    // juiz コマンドがあれば、その置き場所 (既定は ~/.juiz/sdk/<版>) を聞く
    const stdout = switch (b.runFallible(&.{ "juiz", "sdk", "path", "--version", version }, .{ .stderr_behavior = .ignore })) {
        .success => |out| out,
        else => return error.NotFound,
    };
    const root = std.mem.trim(u8, stdout, " \t\r\n");
    if (root.len == 0) return error.NotFound;
    return b.pathJoin(&.{ root, "lib" });
}
