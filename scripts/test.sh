#!/usr/bin/env bash
# juiz のテストを手元と CI で同じ手順で実行するスクリプト。
#
# プラグインを読み込むテスト(system_test や *_broker_api_test など)は、次の前提が揃っていないと失敗する。
#   1. Rust の例: ワークスペースの cargo build で ../target/debug にできる
#   2. C++ の例:  examples/cpp の cmake ビルドで ../target にできる(1 の juiz_sdk にリンクする)
#   3. Python の例: pyyaml httpx pillow docstring_parser と PYTHONPATH=bindings/pyjuiz
# このスクリプトは 1〜3 を済ませてから cargo test -- --test-threads=1 を実行する。
#
# 使い方:
#   scripts/test.sh                      # すべてのテスト
#   scripts/test.sh -p juiz_core         # cargo test に渡す引数
#   scripts/test.sh -p juiz_core -- system_test   # -- 以降はテストバイナリに渡す
#   scripts/test.sh --prepare-only       # 準備だけ行う(cargo llvm-cov などで別にテストを走らせるとき)
#
# 環境変数:
#   PYO3_PYTHON  埋め込み Python と依存パッケージの確認に使う python(既定: python3)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

prepare_only=0
cargo_args=()
test_args=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prepare-only) prepare_only=1 ;;
        -h | --help)
            sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --)
            shift
            test_args=("$@")
            break
            ;;
        *) cargo_args+=("$1") ;;
    esac
    shift
done

python="${PYO3_PYTHON:-python3}"
export PYO3_PYTHON="$python"

step() {
    printf '\n==> %s\n' "$*"
}

step "Rust のワークスペースと例をビルドする (cargo build)"
cargo build

step "C++ の例をビルドする (examples/cpp)"
if ! command -v cmake >/dev/null 2>&1; then
    echo "error: cmake が見つかりません。C++ の例のビルドに cmake と C++ コンパイラが必要です。" >&2
    exit 1
fi
cmake -S examples/cpp -B examples/cpp/build -DCMAKE_BUILD_TYPE=Debug
cmake --build examples/cpp/build --parallel

step "Python の例が使うパッケージを確認する ($python)"
python_packages=(pyyaml httpx pillow docstring_parser)
missing=$("$python" - <<'PY'
import importlib.util
# pip のパッケージ名と import 名の対応
modules = {"pyyaml": "yaml", "httpx": "httpx", "pillow": "PIL", "docstring_parser": "docstring_parser"}
print(" ".join(p for p, m in modules.items() if importlib.util.find_spec(m) is None))
PY
)
if [[ -n "$missing" ]]; then
    echo "error: Python パッケージが足りません: $missing" >&2
    echo "次のコマンドでインストールしてください:" >&2
    echo "  $python -m pip install ${python_packages[*]}" >&2
    exit 1
fi
export PYTHONPATH="$repo_root/bindings/pyjuiz${PYTHONPATH:+:$PYTHONPATH}"
echo "PYTHONPATH=$PYTHONPATH"

if [[ $prepare_only -eq 1 ]]; then
    step "準備が完了しました (--prepare-only)"
    exit 0
fi

# テストはブローカーのポートや埋め込み Python を使うので直列に実行する
# 空配列の展開は macOS 標準の bash 3.2 だと set -u でエラーになるので ${a[@]+...} で書く
step "テストを実行する (cargo test ${cargo_args[*]:-} -- --test-threads=1 ${test_args[*]:-})"
cargo test ${cargo_args[@]+"${cargo_args[@]}"} -- --test-threads=1 ${test_args[@]+"${test_args[@]}"}
