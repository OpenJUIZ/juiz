#!/usr/bin/env bash
# 同梱フォント Zen Old Mincho を画面で使う字だけに絞って woff2 にする(#111)。
#
#   pip install fonttools brotli
#   scripts/subset-fonts.sh <ZenOldMincho-Regular.ttf と ZenOldMincho-Black.ttf があるディレクトリ>
#
# 元の ttf は https://github.com/google/fonts/tree/main/ofl/zenoldmincho から取る(1 本 5MB あるので
# リポジトリには入れない)。字の一覧 src/assets/fonts/subset-chars.txt は npm run fonts:chars で作る。
# OFL には Reserved Font Name が無いので、絞ったものも名前は変えない。
set -euo pipefail

src="${1:?ttf のあるディレクトリを指定してください}"
here="$(cd "$(dirname "$0")/.." && pwd)"
out="$here/src/assets/fonts"

for weight in Regular Black; do
  pyftsubset "$src/ZenOldMincho-$weight.ttf" \
    --text-file="$out/subset-chars.txt" \
    --flavor=woff2 \
    --output-file="$out/ZenOldMincho-$weight.subset.woff2"
done
ls -l "$out"/*.woff2
