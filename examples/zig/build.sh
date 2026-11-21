#!/usr/bin/env bash
# Zig の例 (examples/zig) をビルドし、共有ライブラリを ../../target に置く。
# juiz のテスト (juiz_core/tests/integration/plugins/zig.rs) はそこから読み込む。
#
# 前提: 先にワークスペースで cargo build を済ませ、target/debug に libjuiz_sdk があること。
#       Zig 0.17.x が必要 (https://ziglang.org/download/。pip install ziglang==0.17.1 でも入る)。
#       zig コマンドは環境変数 ZIG で変えられる (例: ZIG="python3 -m ziglang")。
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../.." && pwd)"
sdk_dir="$repo_root/target/debug"
out_dir="$repo_root/target"
zig="${ZIG:-zig}"

case "$(uname -s)" in
    Darwin) ext=dylib; prefix=lib; lib_dir=lib ;;
    # Windows の DLL は zig-out/bin に置かれる (Windows の確認は issue #578)
    MINGW* | MSYS* | CYGWIN*) ext=dll; prefix=""; lib_dir=bin; sdk_dir="$(cygpath -m "$sdk_dir")" ;;
    *) ext=so; prefix=lib; lib_dir=lib ;;
esac

if [[ ! -e "$sdk_dir/${prefix}juiz_sdk.$ext" ]]; then
    echo "error: $sdk_dir/${prefix}juiz_sdk.$ext がありません。先にリポジトリの直下で cargo build を実行してください。" >&2
    exit 1
fi

cd "$here"
# --release で ReleaseSafe (範囲外アクセスなどの安全検査を残す、build.zig の preferred_optimize_mode) にする
$zig build --release -Djuiz-sdk-lib="$sdk_dir"

for name in increment_process_zig greet_process_zig echo_types_process_zig fail_process_zig; do
    lib="zig-out/$lib_dir/$prefix$name.$ext"
    if [[ ! -f "$lib" ]]; then
        echo "error: $lib がありません" >&2
        exit 1
    fi
    cp "$lib" "$out_dir/"
    echo "copied $lib -> $out_dir/"
done
