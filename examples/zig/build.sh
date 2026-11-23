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
    # Windows の DLL は lib の付かない <名前>.dll で zig-out/bin に置かれる (issue #579)
    MINGW* | MSYS* | CYGWIN*) ext=dll; prefix=""; lib_dir=bin; sdk_dir="$(cygpath -m "$sdk_dir")" ;;
    *) ext=so; prefix=lib; lib_dir=lib ;;
esac

if [[ ! -e "$sdk_dir/${prefix}juiz_sdk.$ext" ]]; then
    echo "error: $sdk_dir/${prefix}juiz_sdk.$ext がありません。先にリポジトリの直下で cargo build を実行してください。" >&2
    exit 1
fi

cd "$here"
# --release で ReleaseSafe (範囲外アクセスなどの安全検査を残す、build.zig の preferred_optimize_mode) にする
# 型付きの例 (issue #576) は juiz-idl -g zig で型を作る。cargo build で target/debug にできた juiz-idl を使う
idl_exe="$repo_root/target/debug/juiz-idl"
case "$(uname -s)" in
    # cygpath は juiz-idl.exe があると .exe を付けて返すので、付いていないときだけ足す (issue #579)
    MINGW* | MSYS* | CYGWIN*)
        idl_exe="$(cygpath -m "$idl_exe")"
        [[ "$idl_exe" == *.exe ]] || idl_exe="$idl_exe.exe"
        ;;
esac
$zig build --release -Djuiz-sdk-lib="$sdk_dir" -Djuiz-idl="$idl_exe"

# 例の一覧は build.zig の examples と同じ
names=(
    increment_process_zig greet_process_zig echo_types_process_zig fail_process_zig
    example_container_zig example_container_zig_increment example_container_zig_get mismatched_container_process_zig
    zig_part zig_part_pose zig_part_set_pose zig_part_try zig_part_peek zig_part_sleep zig_part_lookup zig_part_set_transform zig_part_set_model
    example_component_zig image_component_zig typed_component_zig
)
for name in "${names[@]}"; do
    lib="zig-out/$lib_dir/$prefix$name.$ext"
    if [[ ! -f "$lib" ]]; then
        echo "error: $lib がありません" >&2
        exit 1
    fi
    cp "$lib" "$out_dir/"
    echo "copied $lib -> $out_dir/"
done
