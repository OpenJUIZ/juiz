#!/usr/bin/env bash
# Haskell の例 (examples/haskell) をビルドし、共有ライブラリを ../../target に置く。
# juiz のテスト (juiz_core/tests/haskell_plugin_test.rs) はそこから読み込む。
#
# 前提: 先にワークスペースで cargo build を済ませ、target/debug に libjuiz_sdk があること。
#       GHC 9.6 以降と cabal が必要 (https://www.haskell.org/ghcup/)。
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../.." && pwd)"
sdk_dir="$repo_root/target/debug"
out_dir="$repo_root/target"

case "$(uname -s)" in
    Darwin) ext=dylib ;;
    *) ext=so ;;
esac

if [[ ! -e "$sdk_dir/libjuiz_sdk.$ext" ]]; then
    echo "error: $sdk_dir/libjuiz_sdk.$ext がありません。先にリポジトリの直下で cargo build を実行してください。" >&2
    exit 1
fi

cd "$here"
# macOS では、GHC がリンクのときに Command Line Tools の SDK (/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk)
# を -L で渡す。選ばれている Xcode より Command Line Tools の SDK の版が新しいと、Xcode のリンカがその SDK の
# .tbd を読めずにリンクが失敗する (tapi error: unknown architecture、issue #143)。
# そのときは DEVELOPER_DIR で Command Line Tools のリンカと SDK を使う。DEVELOPER_DIR を指定済みなら何もしない
if [[ "$(uname -s)" == Darwin && -z "${DEVELOPER_DIR:-}" ]]; then
    clt_dir=/Library/Developer/CommandLineTools
    selected_dir="$(xcode-select -p 2>/dev/null || true)"
    if [[ -d "$clt_dir/SDKs/MacOSX.sdk" && "$selected_dir" != "$clt_dir" ]]; then
        clt_sdk="$(basename "$(cd "$clt_dir/SDKs/MacOSX.sdk" && pwd -P)" .sdk)"
        clt_sdk="${clt_sdk#MacOSX}"
        selected_sdk="$(xcrun --sdk macosx --show-sdk-version 2>/dev/null || true)"
        if [[ "${clt_sdk%%.*}" =~ ^[0-9]+$ && "${selected_sdk%%.*}" =~ ^[0-9]+$ \
            && "${clt_sdk%%.*}" -gt "${selected_sdk%%.*}" ]]; then
            echo "Command Line Tools の SDK ($clt_sdk) が $selected_dir の SDK ($selected_sdk) より新しいので、"
            echo "DEVELOPER_DIR=$clt_dir でビルドします (issue #143)"
            export DEVELOPER_DIR="$clt_dir"
        fi
    fi
fi
# 型付きの例 (typed_component_hs など) の型を juiz-idl で作る (docs/spec/datatype.md §22)
juiz_idl="$sdk_dir/juiz-idl"
if [[ ! -x "$juiz_idl" ]]; then
    echo "error: $juiz_idl がありません。先にリポジトリの直下で cargo build -p juiz_idl を実行してください。" >&2
    exit 1
fi
"$juiz_idl" -g haskell -o "$here/gen" "$repo_root/juiz_idl/test_package/idl/geometry.jidl" > /dev/null

# libjuiz_sdk はビルドしたディレクトリから読み込めるよう rpath を入れる (C++ の例の cmake と同じ)
cabal build all \
    --extra-lib-dirs="$sdk_dir" \
    --ghc-options="-optl-Wl,-rpath,$sdk_dir"

for name in increment_process_hs decrement_process_hs value_types_process_hs \
    example_container_hs example_container_hs_increment example_container_hs_get \
    mismatched_container_process_hs example_component_hs broken_component_hs \
    typed_component_hs broken_typed_process_hs; do
    lib=$(find dist-newstyle -name "lib$name.$ext" -type f | head -n 1)
    if [[ -z "$lib" ]]; then
        echo "error: lib$name.$ext が dist-newstyle に見つかりません" >&2
        exit 1
    fi
    cp "$lib" "$out_dir/"
    echo "copied $lib -> $out_dir/"
done
