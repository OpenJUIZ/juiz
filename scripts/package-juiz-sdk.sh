#!/usr/bin/env bash
# libjuiz_sdk(juiz_sdk クレートの cdylib)と C / C++ のヘッダを、配布用のアーカイブにまとめる(issue #136)。
# C++ や Haskell のプラグインは、このアーカイブがあれば juiz のソースを取得してビルドしなくても作れる。
# v* タグで .github/workflows/sdk-release.yml が OS・CPU ごとに実行し、GitHub Releases に置く。
#
# 使い方: scripts/package-juiz-sdk.sh <ターゲット名> [出力ディレクトリ]
#   先に cargo build --release -p juiz_sdk を済ませておく(target/release の libjuiz_sdk を使う)。
#   ターゲット名は rustc のターゲット(x86_64-unknown-linux-gnu など)。アーカイブの名前に使う。
#   環境変数 JUIZ_SDK_LIB_DIR で libjuiz_sdk のあるディレクトリを変えられる。
#
# できるもの: <出力ディレクトリ>/juiz_sdk-<版>-<ターゲット名>.tar.gz と、その .sha256
#   juiz_sdk-<版>-<ターゲット名>/
#     lib/libjuiz_sdk.so(macOS は .dylib)
#     lib/pkgconfig/juiz_sdk.pc
#     cjuiz/include/juiz_sdk.h          C のヘッダ(Haskell の hsjuiz などが使う)
#     cppjuiz/include/juiz/*.h          C++ のヘッダ(cjuiz のヘッダを相対パスで include するので、この配置のまま使う)
#     README.md, LICENSE-MIT, LICENSE-APACHE
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <target> [out-dir]" >&2
    exit 2
fi
target="$1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${2:-$repo_root/target/sdk-dist}"
lib_dir="${JUIZ_SDK_LIB_DIR:-$repo_root/target/release}"

version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$repo_root/juiz_sdk/Cargo.toml" | head -n 1)
if [[ -z "$version" ]]; then
    echo "error: juiz_sdk/Cargo.toml から版を読めません" >&2
    exit 1
fi

case "$(uname -s)" in
    Darwin) lib_name=libjuiz_sdk.dylib ;;
    *) lib_name=libjuiz_sdk.so ;;
esac
if [[ ! -f "$lib_dir/$lib_name" ]]; then
    echo "error: $lib_dir/$lib_name がありません。先に cargo build --release -p juiz_sdk を実行してください。" >&2
    exit 1
fi

name="juiz_sdk-$version-$target"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/$name"
mkdir -p "$root/lib/pkgconfig" "$root/cjuiz" "$root/cppjuiz"

cp "$lib_dir/$lib_name" "$root/lib/"
if [[ "$(uname -s)" == Darwin ]]; then
    # rustc は dylib の install name をビルドしたときの絶対パスにするので、
    # プラグインが rpath で見つけられるよう @rpath/ に変える
    install_name_tool -id "@rpath/$lib_name" "$root/lib/$lib_name"
fi
cp -R "$repo_root/bindings/cjuiz/include" "$root/cjuiz/"
cp -R "$repo_root/bindings/cppjuiz/include" "$root/cppjuiz/"
cp "$repo_root/LICENSE-MIT" "$repo_root/LICENSE-APACHE" "$root/"

cat > "$root/lib/pkgconfig/juiz_sdk.pc" <<EOF
prefix=\${pcfiledir}/../..
libdir=\${prefix}/lib

Name: juiz_sdk
Description: C ABI of juiz_sdk for C++ / Haskell plugins of juiz
Version: $version
URL: https://github.com/OpenJUIZ/juiz
Libs: -L\${libdir} -Wl,-rpath,\${libdir} -ljuiz_sdk
Cflags: -I\${prefix}/cjuiz/include -I\${prefix}/cppjuiz/include
EOF

cat > "$root/README.md" <<EOF
# juiz_sdk $version ($target)

libjuiz_sdk and the C / C++ headers for writing C++ and Haskell plugins of [juiz](https://github.com/OpenJUIZ/juiz).
Use the same version as the \`juiz\` command (\`juiz --version\`). juiz refuses to load a plugin built with a different version of libjuiz_sdk.

- \`lib/\`: libjuiz_sdk. Pass this directory to the linker (\`-L\`) and to the plugin's rpath.
- \`cjuiz/include/juiz_sdk.h\`: C header.
- \`cppjuiz/include/juiz/\`: C++ headers. Add both \`cjuiz/include\` and \`cppjuiz/include\` to the include path.
- \`lib/pkgconfig/juiz_sdk.pc\`: pkg-config file (\`PKG_CONFIG_PATH=<this dir>/lib/pkgconfig\`).

For Haskell (hsjuiz), add this to \`cabal.project\`:

\`\`\`cabal
package hsjuiz
  extra-lib-dirs: <this dir>/lib
  ghc-options: -optl-Wl,-rpath,<this dir>/lib
\`\`\`

License: MIT OR Apache-2.0
EOF

mkdir -p "$out_dir"
archive="$out_dir/$name.tar.gz"
tar -C "$work" -czf "$archive" "$name"
(cd "$out_dir" && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256" 2>/dev/null \
    || sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "$archive"
