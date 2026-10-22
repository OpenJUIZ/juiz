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
# libjuiz_sdk はビルドしたディレクトリから読み込めるよう rpath を入れる (C++ の例の cmake と同じ)
cabal build all \
    --extra-lib-dirs="$sdk_dir" \
    --ghc-options="-optl-Wl,-rpath,$sdk_dir"

for name in increment_process_hs decrement_process_hs; do
    lib=$(find dist-newstyle -name "lib$name.$ext" -type f | head -n 1)
    if [[ -z "$lib" ]]; then
        echo "error: lib$name.$ext が dist-newstyle に見つかりません" >&2
        exit 1
    fi
    cp "$lib" "$out_dir/"
    echo "copied $lib -> $out_dir/"
done
