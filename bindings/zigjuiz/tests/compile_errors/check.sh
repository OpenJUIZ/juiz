#!/usr/bin/env bash
# zigjuiz の exportProcess が、間違った書き方をコンパイルエラーにすることを確かめる (issue #573)。
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
