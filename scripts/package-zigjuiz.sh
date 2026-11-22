#!/usr/bin/env bash
# zigjuiz(bindings/zigjuiz)を、Zig のパッケージとして配るアーカイブにまとめる(issue #578)。
# 利用者は build.zig.zon に
#   zig fetch --save https://github.com/OpenJUIZ/juiz/releases/download/v<版>/zigjuiz-<版>.tar.gz
# で取り込む。juiz リポジトリの直下には build.zig が無く、git の URL では bindings/zigjuiz を指せないので、
# zigjuiz のディレクトリだけをアーカイブにする。v* タグで .github/workflows/sdk-release.yml が実行し、GitHub Releases に置く。
#
# 使い方: scripts/package-zigjuiz.sh [出力ディレクトリ]   (既定: target/sdk-dist)
#
# できるもの: <出力ディレクトリ>/zigjuiz-<版>.tar.gz と、その .sha256
#   zigjuiz-<版>/
#     build.zig, build.zig.zon, src/, README.md, LICENSE-MIT, LICENSE-APACHE
#     include/juiz_sdk.h     C のヘッダの写し(build.zig の header_path をここに書き換える)
#
# zigjuiz の版(build.zig.zon の version)は juiz_sdk/Cargo.toml の版と同じでなければならない
# (zigjuiz はその版の libjuiz_sdk を探す)。アーカイブの中身は Zig のパッケージのハッシュに効くので、
# 同じソースからは同じ中身になるようにする(ファイルの並び・時刻・所有者を固定する)。
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/target/sdk-dist}"
src="$repo_root/bindings/zigjuiz"

version=$(sed -n 's/^ *\.version *= *"\(.*\)",.*/\1/p' "$src/build.zig.zon" | head -n 1)
sdk_version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$repo_root/juiz_sdk/Cargo.toml" | head -n 1)
if [[ -z "$version" ]]; then
    echo "error: bindings/zigjuiz/build.zig.zon から版を読めません" >&2
    exit 1
fi
if [[ "$version" != "$sdk_version" ]]; then
    echo "error: zigjuiz の版 $version(bindings/zigjuiz/build.zig.zon)が juiz_sdk の版 $sdk_version と違います" >&2
    exit 1
fi

name="zigjuiz-$version"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
root="$work/$name"
mkdir -p "$root/include"

cp "$src/build.zig" "$src/build.zig.zon" "$src/README.md" "$src/LICENSE-MIT" "$src/LICENSE-APACHE" "$root/"
cp -R "$src/src" "$root/"
cp "$repo_root/bindings/cjuiz/include/juiz_sdk.h" "$root/include/"

# リポジトリの中では ../cjuiz/include/juiz_sdk.h を読む。アーカイブでは隣の include/ を読む
old='const header_path = "../cjuiz/include/juiz_sdk.h";'
new='const header_path = "include/juiz_sdk.h";'
if ! grep -qF "$old" "$root/build.zig"; then
    echo "error: bindings/zigjuiz/build.zig に $old がありません(書き換えられません)" >&2
    exit 1
fi
sed -i.bak "s|^const header_path = \"../cjuiz/include/juiz_sdk.h\";|$new|" "$root/build.zig"
rm -f "$root/build.zig.bak"
grep -qF "$new" "$root/build.zig"

# build.zig.zon の .paths に無いもの(テストなど)は入れていない。.paths に書いたものが全部あることを確かめる
for p in $(sed -n '/\.paths *= *\.{/,/}/s/^ *"\([^"]*\)",*$/\1/p' "$src/build.zig.zon"); do
    if [[ ! -e "$root/$p" ]]; then
        echo "error: build.zig.zon の .paths にある $p がアーカイブにありません" >&2
        exit 1
    fi
done

mkdir -p "$out_dir"
file="$name.tar.gz"
out_abs="$(cd "$out_dir" && pwd)"
find "$root" -exec touch -h -t 202001010000 {} +
if tar --version 2> /dev/null | grep -q GNU; then
    (cd "$work" && tar --sort=name --owner=0 --group=0 --numeric-owner -cf - "$name" | gzip -n > "$out_abs/$file")
else
    # macOS の bsdtar(並びは find で固定する)
    (cd "$work" && find "$name" | LC_ALL=C sort | tar --uid 0 --gid 0 -n -cf - -T - | gzip -n > "$out_abs/$file")
fi
(cd "$out_dir" && shasum -a 256 "$file" > "$file.sha256" 2>/dev/null \
    || sha256sum "$file" > "$file.sha256")
echo "$out_dir/$file"
