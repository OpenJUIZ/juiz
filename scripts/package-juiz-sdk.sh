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
# できるもの: <出力ディレクトリ>/juiz_sdk-<版>-<ターゲット名>.tar.gz(Windows は .zip)と、その .sha256
#   juiz_sdk-<版>-<ターゲット名>/
#     lib/libjuiz_sdk.so(macOS は .dylib)
#     lib/pkgconfig/juiz_sdk.pc
#     bin/juiz_sdk.dll, lib/juiz_sdk.dll.lib    Windows は .so と .pc の代わりに DLL とインポートライブラリ(issue #273)
#     lib/cmake/juiz_sdk/juiz_sdkConfig.cmake   CMake の find_package(juiz_sdk) 用(issue #162)
#     cjuiz/include/juiz_sdk.h          C のヘッダ(Haskell の hsjuiz などが使う)
#     cppjuiz/include/juiz/*.h          C++ のヘッダ(#include "juiz_sdk.h" で C のヘッダを読むので、cjuiz/include もインクルードパスに入れる)
#     cppjuiz/cmake/juiz_idl.cmake      juiz-idl で IDL から C++ の型を作る CMake の関数(juiz_sdkConfig.cmake が読み込む)
#     README.md, LICENSE-MIT, LICENSE-APACHE
#
# Windows(ターゲット名が *-windows-msvc)は Git Bash で実行する。zip は 7z か zip で作る。
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

windows=false
case "$target" in
    *-windows-*)
        windows=true
        lib_name=juiz_sdk.dll
        ;;
    *-apple-*) lib_name=libjuiz_sdk.dylib ;;
    *) lib_name=libjuiz_sdk.so ;;
esac
for f in "$lib_name" $($windows && echo juiz_sdk.dll.lib); do
    if [[ ! -f "$lib_dir/$f" ]]; then
        echo "error: $lib_dir/$f がありません。先に cargo build --release -p juiz_sdk を実行してください。" >&2
        exit 1
    fi
done

name="juiz_sdk-$version-$target"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/$name"
mkdir -p "$root/lib/cmake/juiz_sdk" "$root/cjuiz" "$root/cppjuiz/cmake"

if $windows; then
    # DLL は bin/、リンクに使うインポートライブラリは lib/(CMake でインストールしたときと同じ配置)
    mkdir -p "$root/bin"
    cp "$lib_dir/juiz_sdk.dll" "$root/bin/"
    cp "$lib_dir/juiz_sdk.dll.lib" "$root/lib/"
else
    cp "$lib_dir/$lib_name" "$root/lib/"
fi
if [[ "$lib_name" == *.dylib ]]; then
    # rustc は dylib の install name をビルドしたときの絶対パスにするので、
    # プラグインが rpath で見つけられるよう @rpath/ に変える
    install_name_tool -id "@rpath/$lib_name" "$root/lib/$lib_name"
fi
cp -R "$repo_root/bindings/cjuiz/include" "$root/cjuiz/"
cp -R "$repo_root/bindings/cppjuiz/include" "$root/cppjuiz/"
cp "$repo_root/bindings/cppjuiz/cmake/juiz_idl.cmake" "$root/cppjuiz/cmake/"
cp "$repo_root/bindings/cppjuiz/cmake/juiz_sdkConfig.cmake" "$root/lib/cmake/juiz_sdk/"
cp "$repo_root/LICENSE-MIT" "$repo_root/LICENSE-APACHE" "$root/"

# juiz は libjuiz_sdk の版が自分と違うプラグインを読み込まないので、find_package で版を指定したときは
# 同じ版だけを受け付ける
cat > "$root/lib/cmake/juiz_sdk/juiz_sdkConfigVersion.cmake" <<EOF
set(PACKAGE_VERSION "$version")
if (PACKAGE_FIND_VERSION AND NOT PACKAGE_FIND_VERSION VERSION_EQUAL PACKAGE_VERSION)
  set(PACKAGE_VERSION_COMPATIBLE FALSE)
else ()
  set(PACKAGE_VERSION_COMPATIBLE TRUE)
  if (PACKAGE_FIND_VERSION)
    set(PACKAGE_VERSION_EXACT TRUE)
  endif ()
endif ()
# libjuiz_sdk ($target) は 64 ビット
if (NOT CMAKE_SIZEOF_VOID_P STREQUAL "8")
  set(PACKAGE_VERSION "\${PACKAGE_VERSION} (64bit)")
  set(PACKAGE_VERSION_UNSUITABLE TRUE)
endif ()
EOF

if ! $windows; then
    mkdir -p "$root/lib/pkgconfig"
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
fi

cat > "$root/README.md" <<EOF
# juiz_sdk $version ($target)

libjuiz_sdk and the C / C++ headers for writing C++ and Haskell plugins of [juiz](https://github.com/OpenJUIZ/juiz).
Use the same version as the \`juiz\` command (\`juiz --version\`). juiz refuses to load a plugin built with a different version of libjuiz_sdk.

- \`lib/\`: libjuiz_sdk (\`libjuiz_sdk.so\` / \`libjuiz_sdk.dylib\`). Pass this directory to the linker (\`-L\`) and to the plugin's rpath.
- \`bin/juiz_sdk.dll\` and \`lib/juiz_sdk.dll.lib\` (Windows): the DLL and its import library. Link the import library,
  and put \`juiz_sdk.dll\` next to your plugin's DLL. juiz looks for the DLLs a plugin depends on in the plugin's directory (and in PATH).
- \`cjuiz/include/juiz_sdk.h\`: C header.
- \`cppjuiz/include/juiz/\`: C++ headers. Add both \`cjuiz/include\` and \`cppjuiz/include\` to the include path.
- \`lib/cmake/juiz_sdk/\`: CMake package (\`find_package(juiz_sdk)\`).
- \`lib/pkgconfig/juiz_sdk.pc\` (Linux / macOS): pkg-config file (\`PKG_CONFIG_PATH=<this dir>/lib/pkgconfig\`).

For CMake, add this directory to \`CMAKE_PREFIX_PATH\` (\`-DCMAKE_PREFIX_PATH=<this dir>\`) and:

\`\`\`cmake
find_package(juiz_sdk $version EXACT CONFIG REQUIRED)
add_library(my_process SHARED my_process.cpp)
target_link_libraries(my_process PRIVATE juiz_sdk::juiz_sdk)
juiz_sdk_copy_runtime(my_process)  # Windows: copies juiz_sdk.dll next to my_process.dll
\`\`\`

For Haskell (hsjuiz), add this to \`cabal.project\`:

\`\`\`cabal
package hsjuiz
  extra-lib-dirs: <this dir>/lib
  ghc-options: -optl-Wl,-rpath,<this dir>/lib
\`\`\`

License: MIT OR Apache-2.0
EOF

mkdir -p "$out_dir"
if $windows; then
    file="$name.zip"
    rm -f "$out_dir/$file"
    out_abs="$(cd "$out_dir" && pwd)"
    if command -v 7z > /dev/null 2>&1; then
        (cd "$work" && 7z a -tzip -bd "$out_abs/$file" "$name" > /dev/null)
    else
        (cd "$work" && zip -qr "$out_abs/$file" "$name")
    fi
else
    file="$name.tar.gz"
    tar -C "$work" -czf "$out_dir/$file" "$name"
fi
(cd "$out_dir" && shasum -a 256 "$file" > "$file.sha256" 2>/dev/null \
    || sha256sum "$file" > "$file.sha256")
echo "$out_dir/$file"
