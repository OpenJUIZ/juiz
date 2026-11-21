#!/usr/bin/env bash
# zigjuiz の exportProcess・exportContainer・exportContainerProcess・exportComponent が、間違った書き方を
# コンパイルエラーにすることを確かめる (issue #573、#574、#575)。
# 書き方ごとに src/case.zig を書いてビルドし、期待するエラー文が出るかを見る。最後に正しい書き方がビルドできることも見る。
#
#   bindings/zigjuiz/tests/compile_errors/check.sh <libjuiz_sdk のあるディレクトリの絶対パス>
#
# zig コマンドは環境変数 ZIG で変えられる (例: ZIG="python3 -m ziglang")。scripts/test.sh から呼ばれる。
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sdk_dir="${1:?usage: check.sh <libjuiz_sdk dir>}"
zig="${ZIG:-zig}"
cd "$here"
mkdir -p src
trap 'rm -f src/case.zig' EXIT

failed=0
# expect_error <期待するエラー文の一部> <case.zig の中身 (juiz の import の後)>
expect_error() {
    local expected="$1" body="$2" out
    printf 'const juiz = @import("juiz");\n%s\n' "$body" > src/case.zig
    if out=$($zig build -Djuiz-sdk-lib="$sdk_dir" 2>&1); then
        echo "FAIL: built without error (expected: $expected)" >&2
        failed=1
    elif ! grep -qF -- "$expected" <<<"$out"; then
        echo "FAIL: expected error '$expected', got:" >&2
        echo "$out" | grep "error:" | head -3 >&2
        failed=1
    else
        echo "ok: $expected"
    fi
}

expect_error "argument 'a' has no description in .args" \
    'fn f(args: struct { a: i64 }) i64 { return args.a; } comptime { juiz.exportProcess(f, .{ .type_name = "x" }); }'
expect_error ".args.b is not an argument of the function" \
    'fn f(args: struct { a: i64 }) i64 { return args.a; } comptime { juiz.exportProcess(f, .{ .type_name = "x", .args = .{ .a = "d", .b = "e" } }); }'
expect_error "argument 'a': *i64 is not supported by zigjuiz" \
    'fn f(args: struct { a: *i64 }) i64 { return args.a.*; } comptime { juiz.exportProcess(f, .{ .type_name = "x", .args = .{ .a = "d" } }); }'
expect_error "arguments must be received as one struct" \
    'fn f(a: i64) i64 { return a; } comptime { juiz.exportProcess(f, .{ .type_name = "x" }); }'
expect_error "unknown option .typo" \
    'fn f() i64 { return 1; } comptime { juiz.exportProcess(f, .{ .type_name = "x", .typo = 1 }); }'
expect_error "return type: []u8 is not supported by zigjuiz" \
    'fn f() []u8 { return undefined; } comptime { juiz.exportProcess(f, .{ .type_name = "x" }); }'
expect_error ".type_name is required" \
    'fn f() i64 { return 1; } comptime { juiz.exportProcess(f, .{ .description = "x" }); }'
expect_error "u128 is too large" \
    'fn f(args: struct { a: u128 }) u128 { return args.a; } comptime { juiz.exportProcess(f, .{ .type_name = "x", .args = .{ .a = "d" } }); }'

# Container と ContainerProcess (issue #574)
expect_error ".container_type (the type name of the container) is required" \
    'const S = struct { v: i64 }; fn f(self: *S) i64 { return self.v; } comptime { juiz.exportContainerProcess(f, .{ .type_name = "x" }); }'
expect_error "the first parameter must be a pointer to the container state" \
    'fn f(args: struct { a: i64 }) i64 { return args.a; } comptime { juiz.exportContainerProcess(f, .{ .type_name = "x", .container_type = "c", .args = .{ .a = "d" } }); }'
expect_error "the function must receive the container state first" \
    'fn f() i64 { return 1; } comptime { juiz.exportContainerProcess(f, .{ .type_name = "x", .container_type = "c" }); }'
expect_error "the function must return the container state" \
    'fn f() void {} comptime { juiz.exportContainer(f, .{ .type_name = "x" }); }'
expect_error "unknown option .container_type" \
    'const S = struct { v: i64 }; fn f() S { return .{ .v = 1 }; } comptime { juiz.exportContainer(f, .{ .type_name = "x", .container_type = "c" }); }'
# readonly (状態を *const で受ける) の ContainerProcess からは書けない
expect_error "juiz.setPose can not be called from a readonly container process" \
    'const S = struct { v: i64 }; fn f(self: *const S) !i64 { _ = try juiz.setPose(self, "map", juiz.makePose(0, 0, 0, juiz.quaternion_identity)); return self.v; } comptime { juiz.exportContainerProcess(f, .{ .type_name = "x", .container_type = "c" }); }'
expect_error "juiz.setTransform can not be called from a readonly container process" \
    'const S = struct { v: i64 }; fn f(self: *const S) !i64 { _ = try juiz.setTransform(self, "a", "b", juiz.makeTransform2d(0, 0, 0)); return self.v; } comptime { juiz.exportContainerProcess(f, .{ .type_name = "x", .container_type = "c" }); }'

# Component (issue #575)。壊れた Component は C++ / Haskell では読み込むときのエラーだが、zigjuiz ではコンパイルエラー
component_parts='const S = struct { v: i64 }; const T = struct { w: i64 }; fn p() i64 { return 1; } fn mk() S { return .{ .v = 1 }; } fn get(self: *const S) i64 { return self.v; } fn getT(self: *const T) i64 { return self.w; }'
expect_error 'the type name "a" is used twice in the component' \
    "$component_parts"' comptime { juiz.exportComponent(.{ .type_name = "comp", .parts = .{ juiz.processPart(p, .{ .type_name = "a" }), juiz.containerPart(mk, .{ .type_name = "a" }) } }); }'
expect_error 'the container process "g" belongs to the container "no_such", which is not in the component' \
    "$component_parts"' comptime { juiz.exportComponent(.{ .type_name = "comp", .parts = .{ juiz.containerPart(mk, .{ .type_name = "c" }), juiz.containerProcessPart(get, .{ .type_name = "g", .container_type = "no_such" }) } }); }'
expect_error 'the container process "g" receives *const case.T, but the container "c" holds case.S' \
    "$component_parts"' comptime { juiz.exportComponent(.{ .type_name = "comp", .parts = .{ juiz.containerPart(mk, .{ .type_name = "c" }), juiz.containerProcessPart(getT, .{ .type_name = "g", .container_type = "c" }) } }); }'
expect_error ".parts[0] is not juiz.processPart / juiz.containerPart / juiz.containerProcessPart" \
    "$component_parts"' comptime { juiz.exportComponent(.{ .type_name = "comp", .parts = .{ p } }); }'
expect_error ".parts is required" \
    "$component_parts"' comptime { juiz.exportComponent(.{ .type_name = "comp" }); }'
expect_error ".parts is empty" \
    "$component_parts"' comptime { juiz.exportComponent(.{ .type_name = "comp", .parts = .{} }); }'
# 要素の間違いは exportProcess などと同じ文で出る
expect_error "argument 'a' has no description in .args" \
    'fn f(args: struct { a: i64 }) i64 { return args.a; } comptime { juiz.exportComponent(.{ .type_name = "comp", .parts = .{ juiz.processPart(f, .{ .type_name = "x" }) } }); }'

# 引数なし・戻り値なし・juiz.Context だけ、もビルドできる
printf 'const juiz = @import("juiz");\n%s\n' \
    'fn f(ctx: juiz.Context) !void { _ = ctx; } comptime { juiz.exportProcess(f, .{ .type_name = "x" }); }' > src/case.zig
if $zig build -Djuiz-sdk-lib="$sdk_dir"; then
    echo "ok: fn (juiz.Context) !void builds"
else
    echo "FAIL: fn (juiz.Context) !void did not build" >&2
    failed=1
fi

exit $failed
