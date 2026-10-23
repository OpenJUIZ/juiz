#!/bin/sh
# GitHub Releases から libjuiz_sdk とヘッダ(juiz_sdk-<版>-<ターゲット名>.tar.gz)を取得して展開する(issue #136)。
# C++ や Haskell のプラグインを、juiz のソースを取得せずにビルドするために使う。
#
# 使い方: install-juiz-sdk.sh <版> [展開先]
#   版は juiz コマンドと同じもの(juiz --version、例: 0.0.22)。
#   展開先の既定は ~/.juiz/sdk/<版>(環境変数 JUIZ_SDK_HOME で ~/.juiz/sdk を変えられる)。
#
#   curl -fsSL https://raw.githubusercontent.com/OpenJUIZ/juiz/main/scripts/install-juiz-sdk.sh | sh -s -- 0.0.22
#
# 対応: Linux(x86_64・aarch64)、macOS(arm64・x86_64)
set -eu

if [ $# -lt 1 ]; then
    echo "usage: $0 <version> [dir]" >&2
    exit 2
fi
version="${1#v}"
dest="${2:-${JUIZ_SDK_HOME:-$HOME/.juiz/sdk}/$version}"
base_url="${JUIZ_SDK_BASE_URL:-https://github.com/OpenJUIZ/juiz/releases/download}"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    *)
        echo "error: $(uname -s) $(uname -m) 向けの libjuiz_sdk は配布していません。juiz のリポジトリで cargo build -p juiz_sdk を実行してください。" >&2
        exit 1
        ;;
esac

name="juiz_sdk-$version-$target"
url="$base_url/v$version/$name.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $url"
if ! curl -fsSL -o "$tmp/$name.tar.gz" "$url"; then
    echo "error: $url を取得できません。版($version)が公開されているか https://github.com/OpenJUIZ/juiz/releases を確認してください。" >&2
    exit 1
fi
curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$url.sha256"
expected=$(cut -d ' ' -f 1 < "$tmp/$name.tar.gz.sha256")
if command -v sha256sum > /dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
else
    actual=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
fi
if [ "$expected" != "$actual" ]; then
    echo "error: $name.tar.gz のチェックサムが合いません" >&2
    exit 1
fi

tar -C "$tmp" -xzf "$tmp/$name.tar.gz"
mkdir -p "$(dirname "$dest")"
rm -rf "$dest"
mv "$tmp/$name" "$dest"

cat <<EOF
installed libjuiz_sdk $version to $dest

Haskell (hsjuiz): add this to your cabal.project
  package hsjuiz
    extra-lib-dirs: $dest/lib
    ghc-options: -optl-Wl,-rpath,$dest/lib

C++: add include directories $dest/cjuiz/include and $dest/cppjuiz/include,
     and link $dest/lib/libjuiz_sdk with rpath $dest/lib
     (or PKG_CONFIG_PATH=$dest/lib/pkgconfig pkg-config --cflags --libs juiz_sdk)
EOF
