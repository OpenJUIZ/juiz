#!/usr/bin/env bash
# hsjuiz を Hackage から入れたときと同じ形でビルドできるか確かめる(issue #136)。
#
# 1. bindings/hsjuiz を cabal sdist でパッケージにし、ローカルのパッケージ置き場(file+noindex)に置く
# 2. 配布用の libjuiz_sdk(scripts/package-juiz-sdk.sh のアーカイブを展開したもの)だけを使い、
#    juiz のソースの外で Haskell の Process(examples/haskell/src/IncrementProcess.hs)をビルドする
#    (cabal.project の書き方は README と同じ)
# 3. できた共有ライブラリを Python の ctypes で開き、juiz_hs_init と、ライブラリが読み込んだ
#    libjuiz_sdk の juiz_sdk_build_info を呼ぶ(juiz が読み込むときと同じく、ライブラリのハンドルで探す)
#
# 使い方: scripts/check-hsjuiz-package.sh <展開した juiz_sdk のディレクトリ>
#   GHC と cabal が必要。Hackage の索引は使わない(hsjuiz の依存は GHC に付いてくるパッケージだけ)。
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <juiz_sdk dir>" >&2
    exit 2
fi
sdk_dir="$(cd "$1" && pwd)"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
case "$(uname -s)" in
    Darwin) ext=dylib ;;
    *) ext=so ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/repo" "$work/proj/src"

(cd "$repo_root/bindings/hsjuiz" && cabal check && cabal sdist -o "$work/repo")
hsjuiz_version=$(sed -n 's/^version: *//p' "$repo_root/bindings/hsjuiz/hsjuiz.cabal")

cp "$repo_root/examples/haskell/src/IncrementProcess.hs" "$work/proj/src/"
cat > "$work/proj/cabal.project" <<EOF
packages: .
repository local
  url: file+noindex://$work/repo
active-repositories: local

package hsjuiz
  extra-lib-dirs: $sdk_dir/lib
  ghc-options: -optl-Wl,-rpath,$sdk_dir/lib
EOF
cat > "$work/proj/check.cabal" <<EOF
cabal-version: 3.0
name:          hsjuiz-package-check
version:       0
build-type:    Simple

foreign-library increment_process_hs
    type:             native-shared
    default-language: Haskell2010
    hs-source-dirs:   src
    other-modules:    IncrementProcess
    build-depends:    base, hsjuiz ==$hsjuiz_version
    ghc-options:      -threaded
EOF

# 手元の ~/.cabal/store に同じ版の hsjuiz が残っていると、そちらが使われるので別の置き場にする
(cd "$work/proj" && cabal --store-dir="$work/store" build)

lib=$(find "$work/proj/dist-newstyle" -name "libincrement_process_hs.$ext" -type f | head -n 1)
if [[ -z "$lib" ]]; then
    echo "error: libincrement_process_hs.$ext ができていません" >&2
    exit 1
fi
expected_sdk=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$repo_root/juiz_sdk/Cargo.toml" | head -n 1)

python3 - "$lib" "$expected_sdk" <<'EOF'
import ctypes, sys
lib = ctypes.CDLL(sys.argv[1])
lib.juiz_hs_init.restype = ctypes.c_int64
ret = lib.juiz_hs_init()
assert ret == 0, f"juiz_hs_init returned {ret}"
lib.juiz_sdk_build_info.restype = ctypes.c_char_p
info = lib.juiz_sdk_build_info().decode()
print(f"juiz_sdk_build_info: {info}")
assert f"juiz_sdk={sys.argv[2]};" in info, info
print("ok: hsjuiz builds from its sdist with the packaged libjuiz_sdk")
EOF
